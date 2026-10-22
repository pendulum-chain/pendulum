// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

import {Script, console} from "forge-std/Script.sol";
import {PEN} from "../src/PEN.sol";
import {MigrationVault} from "../src/MigrationVault.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

/// @notice Deploys the migration stack on Base (PRD rollout phase 3):
///         1. MigrationVault with the deployer as interim admin
///         2. PEN, minting the full max issuance into the vault
///         3. vault.setToken (verifies the vault holds 100% of supply)
///         4. hands vault admin to the bootstrap Safe (two-step; the Safe
///            must call acceptAdmin() to complete)
///
/// Environment:
///   ADMIN_SAFE            bootstrap Safe that becomes vault admin; must have
///                         code on this chain (not an EOA or an EIP-7702
///                         delegated EOA)
///   GUARDIAN_SAFE         fast pause guardian
///   ATTESTOR_1..4         attestor transaction-sender addresses (D4: 3-of-4)
///   MAX_ISSUANCE          max issuance in 18-decimal units (decision D3);
///                         must equal EXPECTED_MAX_ISSUANCE
///   PER_RELEASE_CAP       initial per-release cap, 18-decimal units
///   DAILY_CAP             initial daily cap, 18-decimal units
///   EARLIEST_SWEEP_TS     unix timestamp in SECONDS before which no remainder
///                         sweep (D5); at least MIN_SWEEP_FLOOR_AHEAD after
///                         the deploy block and at most MAX_SWEEP_FLOOR_AHEAD
///   TEST_DEPLOY           optional, testnet drills and local runs only: allows
///                         an ADMIN_SAFE without code and an EARLIEST_SWEEP_TS
///                         any time in the future. Refused on Base mainnet.
contract Deploy is Script {
    uint256 internal constant BASE_MAINNET = 8453;
    // 12-decimal pallet amounts -> 18-decimal token amounts (decision D2).
    uint256 internal constant CONVERSION_FACTOR = 1e6;
    uint256 internal constant THRESHOLD = 3;
    // MAX_ISSUANCE and EARLIEST_SWEEP_TS are permanent once deployed, so the
    // script checks them instead of trusting the environment.
    // Decision D3: 150M PEN.
    uint256 internal constant EXPECTED_MAX_ISSUANCE = 150_000_000e18;
    // A sweep floor further out than this is a units slip (milliseconds land
    // ~1000x further) or a typo, not a decision.
    uint256 internal constant MAX_SWEEP_FLOOR_AHEAD = 2 * 365 days;
    // A sweep floor closer than this is a drill value, not a migration window.
    uint256 internal constant MIN_SWEEP_FLOOR_AHEAD = 30 days;

    function run() external returns (PEN pen, MigrationVault vault) {
        bool testDeploy = vm.envOr("TEST_DEPLOY", false);
        require(!testDeploy || block.chainid != BASE_MAINNET, "TEST_DEPLOY is refused on Base mainnet");

        // The bootstrap admin holds every vault setting until governance takes
        // over, so it must be a Safe, not a single key.
        address adminSafe = vm.envAddress("ADMIN_SAFE");
        if (!testDeploy) {
            require(adminSafe.code.length > 0, "ADMIN_SAFE has no code on this chain (create the Safe first)");
            require(bytes3(adminSafe.code) != 0xef0100, "ADMIN_SAFE is an EIP-7702 delegated EOA, not a Safe");
        }
        address guardianSafe = vm.envAddress("GUARDIAN_SAFE");
        uint256 maxIssuance = vm.envUint("MAX_ISSUANCE");
        uint256 perReleaseCap = vm.envUint("PER_RELEASE_CAP");
        uint256 dailyCap = vm.envUint("DAILY_CAP");
        uint256 earliestSweepTs = vm.envUint("EARLIEST_SWEEP_TS");
        require(maxIssuance == EXPECTED_MAX_ISSUANCE, "MAX_ISSUANCE must be 150M PEN in 18-decimal units");
        require(
            earliestSweepTs > block.timestamp && earliestSweepTs <= block.timestamp + MAX_SWEEP_FLOOR_AHEAD,
            "EARLIEST_SWEEP_TS must be a future unix time in seconds, at most two years ahead"
        );
        require(
            earliestSweepTs >= block.timestamp + MIN_SWEEP_FLOOR_AHEAD || testDeploy,
            "EARLIEST_SWEEP_TS is less than 30 days after the deploy block"
        );

        address[] memory attestors = new address[](4);
        attestors[0] = vm.envAddress("ATTESTOR_1");
        attestors[1] = vm.envAddress("ATTESTOR_2");
        attestors[2] = vm.envAddress("ATTESTOR_3");
        attestors[3] = vm.envAddress("ATTESTOR_4");

        // Broadcast as msg.sender (forge sets it to --sender or the single
        // signer), the address the interim admin role below is given to.
        vm.startBroadcast(msg.sender);

        vault = new MigrationVault(
            msg.sender, // interim admin for setToken; handed over below
            guardianSafe,
            attestors,
            THRESHOLD,
            CONVERSION_FACTOR,
            perReleaseCap,
            dailyCap,
            earliestSweepTs
        );
        pen = new PEN(address(vault), maxIssuance);
        vault.setToken(IERC20(address(pen)));
        vault.transferAdmin(adminSafe);

        vm.stopBroadcast();

        console.log("PEN:            ", address(pen));
        console.log("MigrationVault: ", address(vault));
        console.log("NEXT STEP: the bootstrap Safe must call vault.acceptAdmin()");
    }
}
