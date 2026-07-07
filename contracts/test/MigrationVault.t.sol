// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {PEN} from "../src/PEN.sol";
import {MigrationVault} from "../src/MigrationVault.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

contract MigrationVaultTest is Test {
    uint256 internal constant MAX_ISSUANCE = 160_000_000e18;
    // Pallet amounts are 12-decimal; the vault scales to the 18-decimal token.
    uint256 internal constant CONVERSION_FACTOR = 1e6;
    uint256 internal constant PER_RELEASE_CAP = 1_000_000e18;
    uint256 internal constant DAILY_CAP = 2_000_000e18;

    MigrationVault internal vault;
    PEN internal pen;

    address internal admin = makeAddr("admin");
    address internal guardian = makeAddr("guardian");
    address internal recipient = makeAddr("recipient");
    address[] internal attestors;
    uint256 internal earliestSweep;

    function setUp() public {
        for (uint256 i = 0; i < 5; i++) {
            attestors.push(makeAddr(string(abi.encodePacked("attestor", i))));
        }
        earliestSweep = block.timestamp + 365 days;

        vault = new MigrationVault(
            admin, guardian, attestors, 3, CONVERSION_FACTOR, PER_RELEASE_CAP, DAILY_CAP, earliestSweep
        );
        pen = new PEN(address(vault), MAX_ISSUANCE);
        vm.prank(admin);
        vault.setToken(IERC20(address(pen)));
    }

    function approveAs(uint256 attestorIndex, uint64 nonce, address to, uint256 palletAmount) internal {
        vm.prank(attestors[attestorIndex]);
        vault.approve(nonce, to, palletAmount);
    }

    // ---------------------------------------------------------------- setup & wiring

    function test_SetTokenOnlyOnce() public {
        vm.prank(admin);
        vm.expectRevert(MigrationVault.TokenAlreadySet.selector);
        vault.setToken(IERC20(address(pen)));
    }

    function test_SetTokenRequiresFullSupplyInVault() public {
        MigrationVault fresh = new MigrationVault(
            admin, guardian, attestors, 3, CONVERSION_FACTOR, PER_RELEASE_CAP, DAILY_CAP, earliestSweep
        );
        // PEN was minted to the *other* vault, so this one holds nothing.
        vm.prank(admin);
        vm.expectRevert(MigrationVault.VaultMustHoldFullSupply.selector);
        fresh.setToken(IERC20(address(pen)));
    }

    function test_ConstructorRejectsThresholdBelowTwo() public {
        vm.expectRevert(MigrationVault.InvalidThreshold.selector);
        new MigrationVault(admin, guardian, attestors, 1, CONVERSION_FACTOR, PER_RELEASE_CAP, DAILY_CAP, earliestSweep);
    }

    // ---------------------------------------------------------------- happy path

    function test_ThresholdApprovalsRelease() public {
        uint256 palletAmount = 5e12; // 5 PEN in 12-decimal pallet units

        approveAs(0, 0, recipient, palletAmount);
        approveAs(1, 0, recipient, palletAmount);
        assertEq(pen.balanceOf(recipient), 0, "must not release below threshold");

        approveAs(2, 0, recipient, palletAmount);
        assertEq(pen.balanceOf(recipient), 5e18, "decimal conversion 12 -> 18");
        assertTrue(vault.nonceConsumed(0));
        assertEq(vault.totalReleased(), 5e18);
        // Invariant the monitor watches: vault balance + released == total supply.
        assertEq(pen.balanceOf(address(vault)) + vault.totalReleased(), pen.totalSupply());
    }

    // ---------------------------------------------------------------- replay & dedup

    function test_ConsumedNonceCannotReleaseAgain() public {
        uint256 palletAmount = 5e12;
        approveAs(0, 0, recipient, palletAmount);
        approveAs(1, 0, recipient, palletAmount);
        approveAs(2, 0, recipient, palletAmount);

        vm.prank(attestors[3]);
        vm.expectRevert(abi.encodeWithSelector(MigrationVault.NonceAlreadyConsumed.selector, 0));
        vault.approve(0, recipient, palletAmount);

        vm.expectRevert(abi.encodeWithSelector(MigrationVault.NonceAlreadyConsumed.selector, 0));
        vault.release(0, recipient, palletAmount);
    }

    function test_SameAttestorCannotApproveTwice() public {
        approveAs(0, 0, recipient, 5e12);
        vm.prank(attestors[0]);
        vm.expectRevert(abi.encodeWithSelector(MigrationVault.AlreadyApproved.selector, attestors[0]));
        vault.approve(0, recipient, 5e12);
    }

    function test_NonAttestorCannotApprove() public {
        vm.prank(makeAddr("mallory"));
        vm.expectRevert(MigrationVault.NotAttestor.selector);
        vault.approve(0, recipient, 5e12);
    }

    function test_ConflictingTuplesNeverMerge() public {
        address mallory = makeAddr("mallory");
        // Two attestors approve the honest tuple, two approve a conflicting one.
        approveAs(0, 0, recipient, 5e12);
        approveAs(1, 0, recipient, 5e12);
        approveAs(2, 0, mallory, 5e12);
        approveAs(3, 0, mallory, 5e12);
        assertEq(pen.balanceOf(recipient), 0);
        assertEq(pen.balanceOf(mallory), 0);

        // The honest tuple reaches threshold and wins; the nonce is consumed.
        approveAs(4, 0, recipient, 5e12);
        assertEq(pen.balanceOf(recipient), 5e18);
        assertEq(pen.balanceOf(mallory), 0);
    }

    // ---------------------------------------------------------------- caps

    function test_PerReleaseCapDefersUntilAdminRaisesIt() public {
        // 2M PEN in pallet units converts to 2Me18 > perReleaseCap.
        uint256 palletAmount = 2_000_000e12;
        approveAs(0, 0, recipient, palletAmount);
        approveAs(1, 0, recipient, palletAmount);
        approveAs(2, 0, recipient, palletAmount);
        assertEq(pen.balanceOf(recipient), 0, "capped release must be deferred, not executed");

        vm.expectRevert(
            abi.encodeWithSelector(MigrationVault.ExceedsPerReleaseCap.selector, 2_000_000e18, PER_RELEASE_CAP)
        );
        vault.release(0, recipient, palletAmount);

        vm.prank(admin);
        vault.setCaps(3_000_000e18, 3_000_000e18);
        vault.release(0, recipient, palletAmount);
        assertEq(pen.balanceOf(recipient), 2_000_000e18);
    }

    function test_DailyCapRollsOverAtNextDay() public {
        uint256 palletAmount = 1_000_000e12; // converts to exactly the per-release cap

        for (uint64 nonce = 0; nonce < 2; nonce++) {
            approveAs(0, nonce, recipient, palletAmount);
            approveAs(1, nonce, recipient, palletAmount);
            approveAs(2, nonce, recipient, palletAmount);
        }
        // Both releases fit the daily cap exactly.
        assertEq(pen.balanceOf(recipient), 2_000_000e18);

        // A third release today is deferred by the daily cap...
        approveAs(0, 2, recipient, palletAmount);
        approveAs(1, 2, recipient, palletAmount);
        approveAs(2, 2, recipient, palletAmount);
        assertEq(pen.balanceOf(recipient), 2_000_000e18);
        vm.expectRevert(
            abi.encodeWithSelector(MigrationVault.ExceedsDailyCap.selector, 3_000_000e18, DAILY_CAP)
        );
        vault.release(2, recipient, palletAmount);

        // ...and anyone can retry it the next day.
        vm.warp(block.timestamp + 1 days);
        vault.release(2, recipient, palletAmount);
        assertEq(pen.balanceOf(recipient), 3_000_000e18);
    }

    // ---------------------------------------------------------------- pause

    function test_PauseBlocksReleasesButKeepsRecordingApprovals() public {
        vm.prank(guardian);
        vault.pause();

        // Approvals are still recorded while paused (no re-attestation needed).
        approveAs(0, 0, recipient, 5e12);
        approveAs(1, 0, recipient, 5e12);
        approveAs(2, 0, recipient, 5e12);
        assertEq(pen.balanceOf(recipient), 0);

        vm.expectRevert(MigrationVault.EnforcedPause.selector);
        vault.release(0, recipient, 5e12);

        // Guardian cannot unpause; only the admin can.
        vm.prank(guardian);
        vm.expectRevert(MigrationVault.NotAdmin.selector);
        vault.unpause();

        vm.prank(admin);
        vault.unpause();
        vault.release(0, recipient, 5e12);
        assertEq(pen.balanceOf(recipient), 5e18);
    }

    function test_OnlyGuardianOrAdminCanPause() public {
        vm.prank(makeAddr("mallory"));
        vm.expectRevert(MigrationVault.NotGuardianOrAdmin.selector);
        vault.pause();
    }

    // ---------------------------------------------------------------- attestor rotation

    function test_RemovedAttestorApprovalsStopCounting() public {
        approveAs(0, 0, recipient, 5e12);
        approveAs(1, 0, recipient, 5e12);

        // Attestor 0 turns out compromised and is removed: its approval must
        // no longer count towards the threshold.
        vm.prank(admin);
        vault.removeAttestor(attestors[0]);

        approveAs(2, 0, recipient, 5e12);
        assertEq(pen.balanceOf(recipient), 0, "only 2 active approvals remain");

        // A replacement attestor is added and completes the quorum.
        address replacement = makeAddr("replacement");
        vm.prank(admin);
        vault.addAttestor(replacement);
        vm.prank(replacement);
        vault.approve(0, recipient, 5e12);
        assertEq(pen.balanceOf(recipient), 5e18);
    }

    function test_CannotRemoveAttestorBelowThreshold() public {
        vm.startPrank(admin);
        vault.removeAttestor(attestors[0]);
        vault.removeAttestor(attestors[1]);
        vm.expectRevert(MigrationVault.ThresholdWouldExceedAttestors.selector);
        vault.removeAttestor(attestors[2]);
        vm.stopPrank();
    }

    // ---------------------------------------------------------------- admin & sweep

    function test_AdminFunctionsRejectNonAdmin() public {
        vm.startPrank(makeAddr("mallory"));
        vm.expectRevert(MigrationVault.NotAdmin.selector);
        vault.setCaps(1, 1);
        vm.expectRevert(MigrationVault.NotAdmin.selector);
        vault.addAttestor(makeAddr("x"));
        vm.expectRevert(MigrationVault.NotAdmin.selector);
        vault.sweepRemainder(makeAddr("x"));
        vm.stopPrank();
    }

    function test_AdminTransferIsTwoStep() public {
        address newAdmin = makeAddr("timelock");
        vm.prank(admin);
        vault.transferAdmin(newAdmin);
        assertEq(vault.admin(), admin, "no effect before acceptance");

        vm.prank(newAdmin);
        vault.acceptAdmin();
        assertEq(vault.admin(), newAdmin);
    }

    function test_SweepOnlyAfterEarliestTimestamp() public {
        address treasury = makeAddr("treasury");
        vm.prank(admin);
        vm.expectRevert(abi.encodeWithSelector(MigrationVault.SweepNotYetAllowed.selector, earliestSweep));
        vault.sweepRemainder(treasury);

        vm.warp(earliestSweep);
        vm.prank(admin);
        vault.sweepRemainder(treasury);
        assertEq(pen.balanceOf(treasury), MAX_ISSUANCE);
        assertEq(pen.balanceOf(address(vault)), 0);
    }

    // ---------------------------------------------------------------- fuzz

    function testFuzz_ReleasePreservesSupplyInvariant(uint64 nonce, uint96 palletAmount) public {
        palletAmount = uint96(bound(palletAmount, 1, PER_RELEASE_CAP / CONVERSION_FACTOR));
        approveAs(0, nonce, recipient, palletAmount);
        approveAs(1, nonce, recipient, palletAmount);
        approveAs(2, nonce, recipient, palletAmount);

        assertEq(pen.balanceOf(recipient), uint256(palletAmount) * CONVERSION_FACTOR);
        assertEq(pen.balanceOf(address(vault)) + vault.totalReleased(), pen.totalSupply());
    }
}
