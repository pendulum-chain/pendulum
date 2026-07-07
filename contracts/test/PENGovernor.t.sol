// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {TimelockController} from "@openzeppelin/contracts/governance/TimelockController.sol";
import {IGovernor} from "@openzeppelin/contracts/governance/IGovernor.sol";
import {IVotes} from "@openzeppelin/contracts/governance/utils/IVotes.sol";
import {PEN} from "../src/PEN.sol";
import {PENGovernor} from "../src/PENGovernor.sol";
import {MigrationVault} from "../src/MigrationVault.sol";

contract PENGovernorTest is Test {
    uint256 internal constant MAX_ISSUANCE = 160_000_000e18;
    uint256 internal constant TIMELOCK_DELAY = 2 days;
    uint48 internal constant VOTING_DELAY = 1 days;
    uint32 internal constant VOTING_PERIOD = 5 days;

    PEN internal pen;
    PENGovernor internal governor;
    TimelockController internal timelock;
    MigrationVault internal vault;

    address internal alice = makeAddr("alice");
    address internal guardian = makeAddr("guardian");

    function setUp() public {
        // Token held by alice directly so she has voting power without going
        // through a migration flow; the vault under governance is separate.
        pen = new PEN(alice, MAX_ISSUANCE);

        address[] memory empty = new address[](0);
        timelock = new TimelockController(TIMELOCK_DELAY, empty, empty, address(this));

        governor = new PENGovernor(
            IVotes(address(pen)), timelock, VOTING_DELAY, VOTING_PERIOD, 1_000e18, 4
        );

        timelock.grantRole(timelock.PROPOSER_ROLE(), address(governor));
        timelock.grantRole(timelock.CANCELLER_ROLE(), address(governor));
        timelock.grantRole(timelock.EXECUTOR_ROLE(), address(0));
        timelock.renounceRole(timelock.DEFAULT_ADMIN_ROLE(), address(this));

        // A vault administered by the timelock, as after governance handover.
        address[] memory attestors = new address[](3);
        attestors[0] = makeAddr("a0");
        attestors[1] = makeAddr("a1");
        attestors[2] = makeAddr("a2");
        vault = new MigrationVault(
            address(timelock), guardian, attestors, 2, 1e6, 1e24, 2e24, block.timestamp + 365 days
        );

        vm.prank(alice);
        pen.delegate(alice);
        // Advance the clock so the delegation checkpoint is in the past.
        vm.warp(block.timestamp + 1);
    }

    function test_TimelockIsSelfAdministered() public view {
        assertFalse(timelock.hasRole(timelock.DEFAULT_ADMIN_ROLE(), address(this)));
        assertTrue(timelock.hasRole(timelock.PROPOSER_ROLE(), address(governor)));
        assertTrue(timelock.hasRole(timelock.EXECUTOR_ROLE(), address(0)));
    }

    function test_GovernorUsesTimestampClock() public view {
        assertEq(governor.clock(), uint48(block.timestamp));
        assertEq(governor.CLOCK_MODE(), "mode=timestamp");
    }

    function test_FullProposalLifecycle_SetVaultCaps() public {
        address[] memory targets = new address[](1);
        targets[0] = address(vault);
        uint256[] memory values = new uint256[](1);
        bytes[] memory calldatas = new bytes[](1);
        calldatas[0] = abi.encodeCall(MigrationVault.setCaps, (5e24, 9e24));
        string memory description = "Raise migration vault caps";

        vm.prank(alice);
        uint256 proposalId = governor.propose(targets, values, calldatas, description);

        vm.warp(block.timestamp + VOTING_DELAY + 1);
        vm.prank(alice);
        governor.castVote(proposalId, 1); // For

        vm.warp(block.timestamp + VOTING_PERIOD + 1);
        assertEq(uint256(governor.state(proposalId)), uint256(IGovernor.ProposalState.Succeeded));

        governor.queue(targets, values, calldatas, keccak256(bytes(description)));
        vm.warp(block.timestamp + TIMELOCK_DELAY + 1);
        governor.execute(targets, values, calldatas, keccak256(bytes(description)));

        assertEq(vault.perReleaseCap(), 5e24);
        assertEq(vault.dailyCap(), 9e24);
    }

    function test_ProposalBelowThresholdReverts() public {
        address pleb = makeAddr("pleb");
        address[] memory targets = new address[](1);
        targets[0] = address(vault);
        uint256[] memory values = new uint256[](1);
        bytes[] memory calldatas = new bytes[](1);
        calldatas[0] = abi.encodeCall(MigrationVault.setCaps, (1, 1));

        vm.prank(pleb);
        vm.expectRevert();
        governor.propose(targets, values, calldatas, "no voting power");
    }
}
