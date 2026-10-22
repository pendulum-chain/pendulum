// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

import {Script, console} from "forge-std/Script.sol";
import {TimelockController} from "@openzeppelin/contracts/governance/TimelockController.sol";
import {PENGovernor} from "../src/PENGovernor.sol";
import {IVotes} from "@openzeppelin/contracts/governance/utils/IVotes.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {MigrationVault} from "../src/MigrationVault.sol";

/// @notice Deploys the on-chain governance stack (PRD rollout phase 5):
///         TimelockController + PENGovernor, wired so that only the Governor
///         can propose to the timelock and anyone can execute after the delay.
///         The deployer's temporary timelock admin role is renounced at the
///         end, leaving the timelock self-administered.
///
///         After this script: transfer MigrationVault admin to the timelock
///         (bootstrap Safe calls vault.transferAdmin(timelock), then a
///         governance proposal calls vault.acceptAdmin()).
///
/// Environment (each value is checked before anything is broadcast: most are
/// fixed at deploy or changeable only by a proposal through this governor, so
/// a wrong one could not be corrected afterwards):
///   PEN_TOKEN             deployed PEN address; must be VAULT_ADDRESS's token
///   VAULT_ADDRESS         deployed MigrationVault (its setToken verified that
///                         PEN_TOKEN minted the whole supply into it)
///   TIMELOCK_DELAY        seconds, 48h (172800) to 30 days (PRD V5)
///   VOTING_DELAY          seconds before voting starts (timestamp clock),
///                         1 hour to 30 days
///   VOTING_PERIOD         seconds of voting, 1 day to 30 days
///   PROPOSAL_THRESHOLD    18-decimal token units needed to propose, 1 PEN
///                         (1e18) to total supply; a whole-PEN figure such as
///                         50000 is refused
///   QUORUM_FRACTION       percent of CIRCULATING supply (total minus the
///                         vault's unmigrated balance parked at the vote
///                         sink, see PRD G1)
///   QUORUM_FLOOR          absolute quorum lower bound, 18-decimal token
///                         units, 1 PEN (1e18) to total supply; PERMANENT.
///                         Keeps early proposals from being trivially cheap
///                         while circulating supply is still small
///   TIMELOCK_CANCELLER    required: a dedicated canceller Safe, separate
///                         from the guardian, that can cancel a QUEUED
///                         operation during the timelock delay. Without it
///                         only the Governor holds CANCELLER_ROLE, and OZ
///                         Governor lets only the proposer cancel, only before
///                         voting starts, so a hostile proposal that passed
///                         could not be stopped during the delay by anyone.
///                         Once the delay has passed anyone may execute, so
///                         the script refuses to deploy without it. It must
///                         be a Safe on this chain (getThreshold() returns a
///                         non-zero threshold, no EIP-7702 delegated EOA) and
///                         must not be the deployer, PEN_TOKEN, VAULT_ADDRESS
///                         or the vault's guardian.
///   TEST_DEPLOY           optional, testnet drills only: allows a shorter
///                         TIMELOCK_DELAY, VOTING_DELAY and VOTING_PERIOD, a
///                         TIMELOCK_CANCELLER that is not a Safe or is the
///                         guardian, and QUORUM_FLOOR=0. Refused on Base
///                         mainnet.
contract DeployGovernance is Script {
    uint256 internal constant BASE_MAINNET = 8453;
    uint256 internal constant MIN_TIMELOCK_DELAY = 2 days;
    // Shorter windows are a units slip (days entered as seconds), and a vote
    // too short for holders to take part in could not be undone by one.
    uint256 internal constant MIN_VOTING_DELAY = 1 hours;
    uint256 internal constant MIN_VOTING_PERIOD = 1 days;
    // Longer periods are a units slip (milliseconds land ~1000x further), and
    // a governor whose votes never end could not vote to shorten them.
    uint256 internal constant MAX_PERIOD = 30 days;

    function run() external returns (TimelockController timelock, PENGovernor governor) {
        bool testDeploy = vm.envOr("TEST_DEPLOY", false);
        require(!testDeploy || block.chainid != BASE_MAINNET, "TEST_DEPLOY is refused on Base mainnet");

        address token = vm.envAddress("PEN_TOKEN");
        MigrationVault vault = MigrationVault(vm.envAddress("VAULT_ADDRESS"));
        require(address(vault.token()) == token, "PEN_TOKEN is not the migration vault's token");
        uint256 supply = IERC20(token).totalSupply();

        uint256 timelockDelay = vm.envUint("TIMELOCK_DELAY");
        require(timelockDelay >= MIN_TIMELOCK_DELAY || testDeploy, "TIMELOCK_DELAY is below 48h");
        require(timelockDelay <= MAX_PERIOD, "TIMELOCK_DELAY is above 30 days (seconds expected)");
        uint256 votingDelay = vm.envUint("VOTING_DELAY");
        require(votingDelay >= MIN_VOTING_DELAY || testDeploy, "VOTING_DELAY is below 1h (seconds expected)");
        require(votingDelay <= MAX_PERIOD, "VOTING_DELAY is above 30 days (seconds expected)");
        uint256 votingPeriod = vm.envUint("VOTING_PERIOD");
        require(votingPeriod >= MIN_VOTING_PERIOD || testDeploy, "VOTING_PERIOD is below 1 day (seconds expected)");
        require(votingPeriod <= MAX_PERIOD, "VOTING_PERIOD is above 30 days (seconds expected)");

        // 18-decimal units: below one PEN is a whole-PEN figure entered as
        // wei, above the supply could never be met.
        uint256 proposalThreshold = vm.envUint("PROPOSAL_THRESHOLD");
        require(
            proposalThreshold >= 1e18 && proposalThreshold <= supply,
            "PROPOSAL_THRESHOLD must be 18-decimal units, 1 PEN to total supply"
        );
        uint256 quorumFraction = vm.envUint("QUORUM_FRACTION");
        uint256 quorumFloor = vm.envUint("QUORUM_FLOOR");
        require(
            (quorumFloor >= 1e18 || (testDeploy && quorumFloor == 0)) && quorumFloor <= supply,
            "QUORUM_FLOOR must be 18-decimal units, 1 PEN to total supply"
        );

        address canceller = vm.envOr("TIMELOCK_CANCELLER", address(0));
        require(canceller != address(0), "TIMELOCK_CANCELLER is required: no veto during the timelock delay otherwise");
        require(canceller != msg.sender, "TIMELOCK_CANCELLER must not be the deployer");
        require(canceller != token && canceller != address(vault), "TIMELOCK_CANCELLER must not be PEN or the vault");
        require(canceller != vault.guardian() || testDeploy, "TIMELOCK_CANCELLER must be separate from the guardian");
        if (!testDeploy) _safeThreshold(canceller);

        // Broadcast as msg.sender (forge sets it to --sender or the single
        // signer), the deployer that is temporary admin so the roles below
        // can be wired.
        vm.startBroadcast(msg.sender);

        address[] memory empty = new address[](0);
        timelock = new TimelockController(timelockDelay, empty, empty, msg.sender);

        governor = new PENGovernor(
            IVotes(token),
            timelock,
            uint48(votingDelay), // bounded by MAX_PERIOD above
            uint32(votingPeriod),
            proposalThreshold,
            quorumFraction,
            quorumFloor
        );

        // Only the Governor proposes; anyone may execute after the delay. The
        // Governor cancels its own operations, and the human canceller can
        // veto a queued operation during the delay — the reaction window the
        // timelock exists to provide.
        timelock.grantRole(timelock.PROPOSER_ROLE(), address(governor));
        timelock.grantRole(timelock.CANCELLER_ROLE(), address(governor));
        timelock.grantRole(timelock.CANCELLER_ROLE(), canceller);
        timelock.grantRole(timelock.EXECUTOR_ROLE(), address(0));
        // Leave the timelock self-administered: changes require a proposal.
        timelock.renounceRole(timelock.DEFAULT_ADMIN_ROLE(), msg.sender);

        vm.stopBroadcast();

        console.log("TimelockController: ", address(timelock));
        console.log("PENGovernor:        ", address(governor));
    }

    // A Safe answers getThreshold() with its non-zero threshold. An EIP-7702
    // delegated EOA stays a single key whatever it delegates to.
    function _safeThreshold(address account) internal view returns (uint256) {
        require(account.code.length > 0, "TIMELOCK_CANCELLER has no code on this chain (create the Safe first)");
        require(bytes3(account.code) != 0xef0100, "TIMELOCK_CANCELLER is an EIP-7702 delegated EOA, not a Safe");
        (bool ok, bytes memory threshold) = account.staticcall(abi.encodeWithSignature("getThreshold()"));
        require(ok && threshold.length == 32, "TIMELOCK_CANCELLER is not a Safe (getThreshold() failed)");
        uint256 value = abi.decode(threshold, (uint256));
        require(value > 0, "TIMELOCK_CANCELLER Safe has threshold 0 (not set up)");
        return value;
    }
}
