// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

import {Script, console} from "forge-std/Script.sol";
import {TimelockController} from "@openzeppelin/contracts/governance/TimelockController.sol";
import {PENGovernor} from "../src/PENGovernor.sol";
import {IVotes} from "@openzeppelin/contracts/governance/utils/IVotes.sol";

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
/// Environment:
///   PEN_TOKEN             deployed PEN address
///   TIMELOCK_DELAY        seconds (PRD V5: >= 48h = 172800)
///   VOTING_DELAY          seconds before voting starts (timestamp clock)
///   VOTING_PERIOD         seconds of voting
///   PROPOSAL_THRESHOLD    token units needed to propose
///   QUORUM_FRACTION       percent of CIRCULATING supply (total minus the
///                         vault's unmigrated balance parked at the vote
///                         sink, see PRD G1 as revised in review round 8)
///   QUORUM_FLOOR          absolute quorum lower bound, 18-decimal token
///                         units; keeps early proposals from being trivially
///                         cheap while circulating supply is still small
///   TIMELOCK_CANCELLER    optional: an address (the guardian/council Safe)
///                         that can cancel a QUEUED operation during the
///                         timelock delay. Without it only the Governor holds
///                         CANCELLER_ROLE, and OZ Governor lets only the
///                         proposer cancel, only before voting starts — so a
///                         hostile proposal that passed could not be stopped
///                         during the 48h delay by anyone (review round 9)
contract DeployGovernance is Script {
    function run() external {
        address token = vm.envAddress("PEN_TOKEN");
        uint256 timelockDelay = vm.envUint("TIMELOCK_DELAY");
        uint48 votingDelay = uint48(vm.envUint("VOTING_DELAY"));
        uint32 votingPeriod = uint32(vm.envUint("VOTING_PERIOD"));
        uint256 proposalThreshold = vm.envUint("PROPOSAL_THRESHOLD");
        uint256 quorumFraction = vm.envUint("QUORUM_FRACTION");
        uint256 quorumFloor = vm.envUint("QUORUM_FLOOR");
        address canceller = vm.envOr("TIMELOCK_CANCELLER", address(0));

        vm.startBroadcast();

        // Deployer is temporary admin so the roles below can be wired.
        address[] memory empty = new address[](0);
        TimelockController timelock =
            new TimelockController(timelockDelay, empty, empty, msg.sender);

        PENGovernor governor = new PENGovernor(
            IVotes(token), timelock, votingDelay, votingPeriod, proposalThreshold, quorumFraction, quorumFloor
        );

        // Only the Governor proposes; anyone may execute after the delay. The
        // Governor cancels its own operations, and an optional human canceller
        // can veto a queued operation during the delay — the reaction window
        // the timelock exists to provide.
        timelock.grantRole(timelock.PROPOSER_ROLE(), address(governor));
        timelock.grantRole(timelock.CANCELLER_ROLE(), address(governor));
        if (canceller != address(0)) timelock.grantRole(timelock.CANCELLER_ROLE(), canceller);
        timelock.grantRole(timelock.EXECUTOR_ROLE(), address(0));
        // Leave the timelock self-administered: changes require a proposal.
        timelock.renounceRole(timelock.DEFAULT_ADMIN_ROLE(), msg.sender);

        vm.stopBroadcast();

        console.log("TimelockController: ", address(timelock));
        console.log("PENGovernor:        ", address(governor));
    }
}
