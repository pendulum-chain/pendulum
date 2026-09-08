// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

import {Governor} from "@openzeppelin/contracts/governance/Governor.sol";
import {GovernorSettings} from "@openzeppelin/contracts/governance/extensions/GovernorSettings.sol";
import {GovernorCountingSimple} from "@openzeppelin/contracts/governance/extensions/GovernorCountingSimple.sol";
import {GovernorVotes} from "@openzeppelin/contracts/governance/extensions/GovernorVotes.sol";
import {GovernorVotesQuorumFraction} from
    "@openzeppelin/contracts/governance/extensions/GovernorVotesQuorumFraction.sol";
import {GovernorTimelockControl} from "@openzeppelin/contracts/governance/extensions/GovernorTimelockControl.sol";
import {TimelockController} from "@openzeppelin/contracts/governance/TimelockController.sol";
import {IVotes} from "@openzeppelin/contracts/governance/utils/IVotes.sol";

/// @title PENGovernor — on-chain governance for the Base-side PEN contracts
/// @notice Standard OZ Governor composition (hybrid governance model, ADR-001):
///         token-holder votes execute through a TimelockController, which is
///         the admin of the MigrationVault and the treasury after the
///         bootstrap phase. PEN uses the EIP-6372 timestamp clock, so all
///         Governor periods below are in seconds.
///
///         Quorum (PRD G1, revised in review round 8): quorum is a fraction of
///         the CIRCULATING supply — total supply minus the unmigrated balance
///         parked at `QUORUM_SINK` by the MigrationVault — bounded below by an
///         absolute `quorumFloor`. A full-supply denominator could deadlock
///         governance permanently: with the vault holding most of the supply
///         early on, a pause (whose unpause is admin-only, i.e. behind this
///         governor) could freeze releases while the votable supply can never
///         grow to quorum. The sink-based denominator tracks what can actually
///         vote; the floor keeps day-one capture from being trivial.
contract PENGovernor is
    Governor,
    GovernorSettings,
    GovernorCountingSimple,
    GovernorVotes,
    GovernorVotesQuorumFraction,
    GovernorTimelockControl
{
    /// @notice Where the MigrationVault parks the voting power of the
    ///         unmigrated supply (`MigrationVault.VOTE_SINK` — the two
    ///         constants must stay in exact lockstep, asserted in the tests).
    ///         Delegating checkpoints the vault's balance in the token's vote
    ///         history, which lets `quorum()` subtract it per timepoint.
    address public constant QUORUM_SINK = 0x000000000000000000000000000000000000dEaD;

    /// @notice Absolute lower bound on quorum, in token units. Guards the
    ///         early window in which circulating supply is small enough that a
    ///         purely fractional quorum would make proposals trivially cheap.
    uint256 public immutable quorumFloor;

    constructor(
        IVotes token,
        TimelockController timelock,
        uint48 votingDelay_, // seconds (timestamp clock)
        uint32 votingPeriod_, // seconds
        uint256 proposalThreshold_, // token units
        uint256 quorumFraction, // percent of circulating supply
        uint256 quorumFloor_ // token units
    )
        Governor("PENGovernor")
        GovernorSettings(votingDelay_, votingPeriod_, proposalThreshold_)
        GovernorVotes(token)
        GovernorVotesQuorumFraction(quorumFraction)
        GovernorTimelockControl(timelock)
    {
        quorumFloor = quorumFloor_;
    }

    /// @notice Quorum as a fraction of the circulating supply at `timepoint`,
    ///         never below `quorumFloor`. Circulating = past total supply
    ///         minus the votes parked at `QUORUM_SINK` (the vault's unmigrated
    ///         balance, plus anything a holder knowingly burns there — which
    ///         only ever costs the delegator its own voting power, so lowering
    ///         quorum this way is strictly dominated by just voting).
    function quorum(uint256 timepoint)
        public
        view
        override(Governor, GovernorVotesQuorumFraction)
        returns (uint256)
    {
        uint256 parked = token().getPastVotes(QUORUM_SINK, timepoint);
        uint256 circulating = token().getPastTotalSupply(timepoint) - parked;
        uint256 fractional = (circulating * quorumNumerator(timepoint)) / quorumDenominator();
        return fractional > quorumFloor ? fractional : quorumFloor;
    }

    // ----- required overrides for the Governor composition -----

    function votingDelay() public view override(Governor, GovernorSettings) returns (uint256) {
        return super.votingDelay();
    }

    function votingPeriod() public view override(Governor, GovernorSettings) returns (uint256) {
        return super.votingPeriod();
    }

    function proposalThreshold() public view override(Governor, GovernorSettings) returns (uint256) {
        return super.proposalThreshold();
    }

    function state(uint256 proposalId)
        public
        view
        override(Governor, GovernorTimelockControl)
        returns (ProposalState)
    {
        return super.state(proposalId);
    }

    function proposalNeedsQueuing(uint256 proposalId)
        public
        view
        override(Governor, GovernorTimelockControl)
        returns (bool)
    {
        return super.proposalNeedsQueuing(proposalId);
    }

    function _queueOperations(
        uint256 proposalId,
        address[] memory targets,
        uint256[] memory values,
        bytes[] memory calldatas,
        bytes32 descriptionHash
    ) internal override(Governor, GovernorTimelockControl) returns (uint48) {
        return super._queueOperations(proposalId, targets, values, calldatas, descriptionHash);
    }

    function _executeOperations(
        uint256 proposalId,
        address[] memory targets,
        uint256[] memory values,
        bytes[] memory calldatas,
        bytes32 descriptionHash
    ) internal override(Governor, GovernorTimelockControl) {
        super._executeOperations(proposalId, targets, values, calldatas, descriptionHash);
    }

    function _cancel(
        address[] memory targets,
        uint256[] memory values,
        bytes[] memory calldatas,
        bytes32 descriptionHash
    ) internal override(Governor, GovernorTimelockControl) returns (uint256) {
        return super._cancel(targets, values, calldatas, descriptionHash);
    }

    function _executor() internal view override(Governor, GovernorTimelockControl) returns (address) {
        return super._executor();
    }
}
