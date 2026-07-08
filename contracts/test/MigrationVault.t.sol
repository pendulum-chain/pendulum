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

    function test_DailyCapRefillsGraduallyOverRollingWindow() public {
        uint256 palletAmount = 1_000_000e12; // converts to exactly the per-release cap

        // Consume the full daily cap (2 × 1M = DAILY_CAP).
        for (uint64 nonce = 0; nonce < 2; nonce++) {
            approveAs(0, nonce, recipient, palletAmount);
            approveAs(1, nonce, recipient, palletAmount);
            approveAs(2, nonce, recipient, palletAmount);
        }
        assertEq(pen.balanceOf(recipient), 2_000_000e18);
        assertEq(vault.availableDailyAllowance(), 0);

        // A third release is deferred: the bucket is empty.
        approveAs(0, 2, recipient, palletAmount);
        approveAs(1, 2, recipient, palletAmount);
        approveAs(2, 2, recipient, palletAmount);
        assertEq(pen.balanceOf(recipient), 2_000_000e18);
        vm.expectRevert(abi.encodeWithSelector(MigrationVault.ExceedsDailyCap.selector, 1_000_000e18, 0));
        vault.release(2, recipient, palletAmount);

        // Half a day later, exactly half the cap has refilled.
        vm.warp(block.timestamp + 12 hours);
        assertEq(vault.availableDailyAllowance(), 1_000_000e18);
        vault.release(2, recipient, palletAmount);
        assertEq(pen.balanceOf(recipient), 3_000_000e18);
    }

    // The exploit the leaky bucket fixes: the old calendar-day bucket reset to
    // zero at the UTC boundary, letting a compromised quorum release 2× the cap
    // seconds apart. The rolling window must NOT refill instantly.
    function test_DailyCapHasNoInstantResetAtBoundary() public {
        // Sit one second before a UTC day boundary and consume the full cap.
        vm.warp(10 days - 1);
        uint256 palletAmount = 1_000_000e12;
        for (uint64 nonce = 0; nonce < 2; nonce++) {
            approveAs(0, nonce, recipient, palletAmount);
            approveAs(1, nonce, recipient, palletAmount);
            approveAs(2, nonce, recipient, palletAmount);
        }
        assertEq(pen.balanceOf(recipient), 2_000_000e18);

        // Cross the boundary by two seconds — negligible refill. The old
        // calendar-day bucket would have fully reset to the cap here.
        vm.warp(10 days + 1);
        assertLt(vault.availableDailyAllowance(), 1_000e18);
        approveAs(0, 2, recipient, palletAmount);
        approveAs(1, 2, recipient, palletAmount);
        approveAs(2, 2, recipient, palletAmount);
        assertEq(pen.balanceOf(recipient), 2_000_000e18, "no instant reset at the day boundary");
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

    function test_ReaddingAttestorNeverCrossesThresholdSilently() public {
        // Two approvals, then the first approver is removed and later re-added.
        approveAs(0, 0, recipient, 5e12);
        approveAs(1, 0, recipient, 5e12);

        vm.startPrank(admin);
        vault.removeAttestor(attestors[0]);
        vault.addAttestor(attestors[0]);
        vm.stopPrank();

        // The re-add must NOT resurrect the pre-removal approval: crossing the
        // threshold outside approve() would bypass pending-release accounting
        // and let sweepRemainder strand the migration.
        bytes32 payload = vault.payloadHash(0, recipient, 5e12);
        assertEq(vault.activeApprovals(payload), 1, "old-generation approval must not count");
        assertFalse(vault.hasApproved(payload, attestors[0]));

        // The re-added attestor approves again (new generation) — allowed, and
        // together with a third attestor the release executes through approve().
        approveAs(0, 0, recipient, 5e12);
        assertEq(vault.activeApprovals(payload), 2);
        approveAs(2, 0, recipient, 5e12);
        assertEq(pen.balanceOf(recipient), 5e18);
        assertEq(vault.pendingApprovedAmount(), 0);
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
        vault.sweepRemainder(makeAddr("x"), 1);
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
        vault.sweepRemainder(treasury, MAX_ISSUANCE);

        vm.warp(earliestSweep);
        vm.prank(admin);
        vault.sweepRemainder(treasury, MAX_ISSUANCE);
        assertEq(pen.balanceOf(treasury), MAX_ISSUANCE);
        assertEq(pen.balanceOf(address(vault)), 0);
        assertEq(vault.totalSwept(), MAX_ISSUANCE);
    }

    // ---------------------------------------------------------------- pending-release accounting

    function test_SweepExcludesPendingApprovedReleases() public {
        // A migration larger than the per-release cap reaches quorum but is
        // deferred; its owed amount must survive a remainder sweep.
        uint256 palletAmount = 2_000_000e12; // > perReleaseCap after conversion
        approveAs(0, 0, recipient, palletAmount);
        approveAs(1, 0, recipient, palletAmount);
        approveAs(2, 0, recipient, palletAmount);
        assertEq(vault.pendingApprovedAmount(), 2_000_000e18);

        address treasury = makeAddr("treasury");
        vm.warp(earliestSweep);
        // The pending (owed) amount is not sweepable.
        vm.prank(admin);
        vm.expectRevert(
            abi.encodeWithSelector(MigrationVault.ExceedsSweepable.selector, MAX_ISSUANCE, MAX_ISSUANCE - 2_000_000e18)
        );
        vault.sweepRemainder(treasury, MAX_ISSUANCE);

        vm.prank(admin);
        vault.sweepRemainder(treasury, MAX_ISSUANCE - 2_000_000e18);
        assertEq(pen.balanceOf(treasury), MAX_ISSUANCE - 2_000_000e18);
        assertEq(pen.balanceOf(address(vault)), 2_000_000e18, "owed amount stays in the vault");

        // After governance raises the cap, the deferred release still succeeds.
        vm.prank(admin);
        vault.setCaps(3_000_000e18, 3_000_000e18);
        vault.release(0, recipient, palletAmount);
        assertEq(pen.balanceOf(recipient), 2_000_000e18);
        assertEq(vault.pendingApprovedAmount(), 0);
    }

    function test_PendingAccountingClearsOnRelease() public {
        vm.prank(guardian);
        vault.pause();

        approveAs(0, 0, recipient, 5e12);
        approveAs(1, 0, recipient, 5e12);
        approveAs(2, 0, recipient, 5e12);
        assertEq(vault.pendingApprovedAmount(), 5e18, "deferred by pause -> pending");

        vm.prank(admin);
        vault.unpause();
        vault.release(0, recipient, 5e12);
        assertEq(vault.pendingApprovedAmount(), 0);
        assertFalse(vault.pendingRelease(vault.payloadHash(0, recipient, 5e12)));
    }

    function test_ClearStalePendingOnlyForConsumedNonce() public {
        address mallory = makeAddr("mallory");
        vm.prank(guardian);
        vault.pause();

        // Both a legitimate and a conflicting tuple for nonce 0 reach quorum
        // while paused (attestors may approve two different tuples).
        for (uint256 i = 0; i < 3; i++) {
            approveAs(i, 0, recipient, 5e12);
            approveAs(i, 0, mallory, 5e12);
        }
        assertEq(vault.pendingApprovedAmount(), 10e18);

        // The stale (unreleased, unconsumed) pending cannot be cleared yet.
        vm.prank(admin);
        vm.expectRevert(MigrationVault.PendingNotStale.selector);
        vault.clearStalePending(0, mallory, 5e12);

        vm.prank(admin);
        vault.unpause();
        vault.release(0, recipient, 5e12);

        // Now the conflicting tuple's pending entry is stale and clearable.
        vm.prank(admin);
        vault.clearStalePending(0, mallory, 5e12);
        assertEq(vault.pendingApprovedAmount(), 0);
    }

    // A migration that was still gathering approvals when the vault was
    // over-swept must NOT crash the attestor fleet, and must stay recoverable.
    function test_OverSweptInFlightMigrationDefersAndRecovers() public {
        // Bob's migration has 2 of 3 approvals — sub-threshold, so nothing is
        // reserved in pendingApprovedAmount yet.
        approveAs(0, 42, recipient, 5e12);
        approveAs(1, 42, recipient, 5e12);
        assertEq(vault.pendingApprovedAmount(), 0);

        // Admin sweeps the entire (unreserved) balance at window close.
        address treasury = makeAddr("treasury");
        vm.warp(earliestSweep);
        vm.prank(admin);
        vault.sweepRemainder(treasury, MAX_ISSUANCE);
        assertEq(pen.balanceOf(address(vault)), 0);

        // The 3rd approval crosses the threshold with an empty vault. This must
        // NOT revert (which would crash-loop every attestor); it defers instead.
        approveAs(2, 42, recipient, 5e12);
        assertFalse(vault.nonceConsumed(42));
        assertEq(vault.pendingApprovedAmount(), 5e18, "owed amount now tracked as pending");

        // A standalone release attempt reverts cleanly (distinct error).
        vm.expectRevert(MigrationVault.InsufficientVaultBalance.selector);
        vault.release(42, recipient, 5e12);

        // Governance refunds the vault; the release then completes — recoverable.
        vm.prank(treasury);
        pen.transfer(address(vault), 5e18);
        vault.release(42, recipient, 5e12);
        assertEq(pen.balanceOf(recipient), 5e18);
        assertEq(vault.pendingApprovedAmount(), 0);
    }

    function test_HasApprovedFalseForRemovedAttestor() public {
        bytes32 payload = vault.payloadHash(0, recipient, 5e12);
        approveAs(0, 0, recipient, 5e12);
        assertTrue(vault.hasApproved(payload, attestors[0]));

        // Removed and never re-added (the standard RB-1 response): hasApproved
        // must agree with activeApprovals and report false.
        vm.prank(admin);
        vault.removeAttestor(attestors[0]);
        assertFalse(vault.hasApproved(payload, attestors[0]));
        assertEq(vault.activeApprovals(payload), 0);
    }

    // Lowering the threshold can retroactively qualify a sub-threshold payload
    // outside approve(), so a sweep is blocked for a settling period afterwards
    // — giving ops time to release the now-qualifying payload first.
    function test_ThresholdCutBlocksSweepDuringSettling() public {
        uint256 settle = vault.SWEEP_SETTLING_PERIOD();
        address treasury = makeAddr("treasury");

        // A payload sits at 2 approvals under threshold 3 — sub-threshold, so
        // nothing is reserved in pendingApprovedAmount.
        approveAs(0, 5, recipient, 5e12);
        approveAs(1, 5, recipient, 5e12);
        assertEq(vault.pendingApprovedAmount(), 0);

        // Reach the sweep window, then lower the threshold within it.
        vm.warp(earliestSweep);
        vm.prank(admin);
        vault.setThreshold(2);
        uint256 reducedAt = block.timestamp;

        // The sweep is blocked during settling, even though earliestSweep passed.
        vm.prank(admin);
        vm.expectRevert(
            abi.encodeWithSelector(MigrationVault.SweepSettlingAfterThresholdCut.selector, reducedAt + settle)
        );
        vault.sweepRemainder(treasury, 1e18);

        // Ops release the now-qualifying payload during settling (permissionless).
        vault.release(5, recipient, 5e12);
        assertEq(pen.balanceOf(recipient), 5e18);

        // After settling, the sweep proceeds normally.
        vm.warp(reducedAt + settle);
        vm.prank(admin);
        vault.sweepRemainder(treasury, 1e18);
        assertEq(pen.balanceOf(treasury), 1e18);
    }

    // ---------------------------------------------------------------- fuzz

    function testFuzz_ReleasePreservesSupplyInvariant(uint64 nonce, uint96 palletAmount) public {
        palletAmount = uint96(bound(palletAmount, 1, PER_RELEASE_CAP / CONVERSION_FACTOR));
        approveAs(0, nonce, recipient, palletAmount);
        approveAs(1, nonce, recipient, palletAmount);
        approveAs(2, nonce, recipient, palletAmount);

        assertEq(pen.balanceOf(recipient), uint256(palletAmount) * CONVERSION_FACTOR);
        // Conservation incl. the sweep accumulator (monitor's M2b formula).
        assertEq(
            pen.balanceOf(address(vault)) + vault.totalReleased() + vault.totalSwept(),
            pen.totalSupply()
        );
    }
}
