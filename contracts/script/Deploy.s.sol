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
///   ADMIN_SAFE            bootstrap Safe that becomes vault admin
///   GUARDIAN_SAFE         fast pause guardian
///   ATTESTOR_1..5         attestor transaction-sender addresses
///   MAX_ISSUANCE          max issuance in 18-decimal units (decision D3)
///   PER_RELEASE_CAP       initial per-release cap, 18-decimal units
///   DAILY_CAP             initial daily cap, 18-decimal units
///   EARLIEST_SWEEP_TS     unix timestamp before which no remainder sweep (D5)
contract Deploy is Script {
    // 12-decimal pallet amounts -> 18-decimal token amounts (decision D2).
    uint256 internal constant CONVERSION_FACTOR = 1e6;
    uint256 internal constant THRESHOLD = 3;

    function run() external {
        address adminSafe = vm.envAddress("ADMIN_SAFE");
        address guardianSafe = vm.envAddress("GUARDIAN_SAFE");
        uint256 maxIssuance = vm.envUint("MAX_ISSUANCE");
        uint256 perReleaseCap = vm.envUint("PER_RELEASE_CAP");
        uint256 dailyCap = vm.envUint("DAILY_CAP");
        uint256 earliestSweepTs = vm.envUint("EARLIEST_SWEEP_TS");

        address[] memory attestors = new address[](5);
        attestors[0] = vm.envAddress("ATTESTOR_1");
        attestors[1] = vm.envAddress("ATTESTOR_2");
        attestors[2] = vm.envAddress("ATTESTOR_3");
        attestors[3] = vm.envAddress("ATTESTOR_4");
        attestors[4] = vm.envAddress("ATTESTOR_5");

        vm.startBroadcast();

        MigrationVault vault = new MigrationVault(
            msg.sender, // interim admin for setToken; handed over below
            guardianSafe,
            attestors,
            THRESHOLD,
            CONVERSION_FACTOR,
            perReleaseCap,
            dailyCap,
            earliestSweepTs
        );
        PEN pen = new PEN(address(vault), maxIssuance);
        vault.setToken(IERC20(address(pen)));
        vault.transferAdmin(adminSafe);

        vm.stopBroadcast();

        console.log("PEN:            ", address(pen));
        console.log("MigrationVault: ", address(vault));
        console.log("NEXT STEP: the bootstrap Safe must call vault.acceptAdmin()");
    }
}
