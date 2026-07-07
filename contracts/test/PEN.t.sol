// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

import {Test} from "forge-std/Test.sol";
import {PEN} from "../src/PEN.sol";

contract PENTest is Test {
    // Placeholder until PRD decision D3 confirms the exact figure.
    uint256 internal constant MAX_ISSUANCE = 160_000_000e18;

    PEN internal pen;
    address internal vault = makeAddr("vault");
    address internal alice;
    uint256 internal alicePk;

    function setUp() public {
        (alice, alicePk) = makeAddrAndKey("alice");
        pen = new PEN(vault, MAX_ISSUANCE);
    }

    function test_FullSupplyMintedToVault() public view {
        assertEq(pen.totalSupply(), MAX_ISSUANCE);
        assertEq(pen.balanceOf(vault), MAX_ISSUANCE);
        assertEq(pen.decimals(), 18);
    }

    function test_RevertWhen_ZeroVaultOrZeroIssuance() public {
        vm.expectRevert(PEN.ZeroVault.selector);
        new PEN(address(0), MAX_ISSUANCE);
        vm.expectRevert(PEN.ZeroIssuance.selector);
        new PEN(vault, 0);
    }

    function test_ClockIsTimestampMode() public {
        vm.warp(1_900_000_000);
        assertEq(pen.clock(), uint48(1_900_000_000));
        assertEq(pen.CLOCK_MODE(), "mode=timestamp");
    }

    function test_PermitSetsAllowance() public {
        address spender = makeAddr("spender");
        uint256 value = 123e18;
        uint256 deadline = block.timestamp + 1 hours;

        bytes32 structHash = keccak256(
            abi.encode(
                keccak256("Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)"),
                alice,
                spender,
                value,
                pen.nonces(alice),
                deadline
            )
        );
        bytes32 digest = keccak256(abi.encodePacked("\x19\x01", pen.DOMAIN_SEPARATOR(), structHash));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(alicePk, digest);

        pen.permit(alice, spender, value, deadline, v, r, s);
        assertEq(pen.allowance(alice, spender), value);
    }

    function test_VotesRequireDelegation() public {
        vm.prank(vault);
        pen.transfer(alice, 1_000e18);

        assertEq(pen.getVotes(alice), 0);
        vm.prank(alice);
        pen.delegate(alice);
        assertEq(pen.getVotes(alice), 1_000e18);

        // Checkpoints are queryable by past timestamp (EIP-6372 timestamp mode).
        uint256 before = block.timestamp;
        vm.warp(before + 1 days);
        assertEq(pen.getPastVotes(alice, before), 1_000e18);
    }
}
