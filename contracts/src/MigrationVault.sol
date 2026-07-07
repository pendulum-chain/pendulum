// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

/// @title MigrationVault — releases pre-minted PEN as holders migrate from Pendulum
/// @notice Holds the entire unmigrated PEN supply. Each attestor independently
///         observes finalized `MigrationInitiated` events on the Pendulum
///         parachain and submits an on-chain approval for the exact
///         (nonce, recipient, amount) tuple. The `threshold`-th matching
///         approval releases the tokens (on-chain-approvals model, ADR-001).
///
///         Trust and blast-radius model (PRD §8):
///         - fewer than `threshold` attestors can release nothing;
///         - a compromised quorum is bounded by `perReleaseCap`/`dailyCap`
///           and can be stopped by the guardian's `pause`;
///         - all parameter changes go through `admin`, expected to be a
///           TimelockController (>= 48h) after the bootstrap phase.
contract MigrationVault {
    using SafeERC20 for IERC20;

    // ---------------------------------------------------------------- errors

    error NotAdmin();
    error NotGuardianOrAdmin();
    error NotAttestor();
    error NotPendingAdmin();
    error ZeroAddress();
    error TokenAlreadySet();
    error TokenNotSet();
    error VaultMustHoldFullSupply();
    error InvalidThreshold();
    error DuplicateAttestor();
    error UnknownAttestor();
    error ThresholdWouldExceedAttestors();
    error NonceAlreadyConsumed(uint64 nonce);
    error AlreadyApproved(address attestor);
    error NotEnoughApprovals(uint256 active, uint256 required);
    error EnforcedPause();
    error NotPaused();
    error ZeroAmount();
    error ExceedsPerReleaseCap(uint256 amount, uint256 cap);
    error ExceedsDailyCap(uint256 wouldBeReleasedToday, uint256 cap);
    error SweepNotYetAllowed(uint256 earliest);

    // ---------------------------------------------------------------- events

    event TokenSet(address indexed token);
    event Approved(uint64 indexed nonce, address indexed recipient, uint256 palletAmount, address indexed attestor);
    event Released(uint64 indexed nonce, address indexed recipient, uint256 palletAmount, uint256 tokenAmount);
    event Paused(address indexed by);
    event Unpaused(address indexed by);
    event AttestorAdded(address indexed attestor);
    event AttestorRemoved(address indexed attestor);
    event ThresholdUpdated(uint256 threshold);
    event CapsUpdated(uint256 perReleaseCap, uint256 dailyCap);
    event GuardianUpdated(address indexed guardian);
    event AdminTransferStarted(address indexed pendingAdmin);
    event AdminTransferred(address indexed newAdmin);
    event RemainderSwept(address indexed to, uint256 amount);

    // ---------------------------------------------------------------- state

    /// @notice The PEN token. Set exactly once, after which the vault must
    ///         hold the token's entire supply (pre-mint model, ADR-001).
    IERC20 public token;

    /// @notice Admin of all parameters; a TimelockController post-bootstrap.
    address public admin;
    address public pendingAdmin;

    /// @notice Can pause releases instantly (incident response). Unpause is
    ///         admin-only, so a compromised guardian can at worst halt.
    address public guardian;

    mapping(address => bool) public isAttestor;
    uint256 public attestorCount;
    /// @notice Number of distinct active attestors that must approve the
    ///         identical (nonce, recipient, amount) tuple to release.
    uint256 public threshold;

    /// @notice Multiplier from pallet units (12 decimals on Pendulum) to token
    ///         units. The decimal conversion happens here and nowhere else
    ///         (PRD V7); 1e6 for an 18-decimal token.
    uint256 public immutable conversionFactor;

    /// @notice Maximum token units released in a single migration.
    uint256 public perReleaseCap;
    /// @notice Maximum token units released per UTC day.
    uint256 public dailyCap;
    uint256 public currentDay;
    uint256 public releasedToday;

    /// @notice Earliest timestamp at which the admin may sweep the unmigrated
    ///         remainder (end-of-window handling, PRD V9 / decision D5).
    uint256 public immutable earliestSweepTimestamp;

    bool public paused;

    /// @notice Consumed migration nonces; a nonce can never release twice.
    mapping(uint64 => bool) public nonceConsumed;

    /// @dev Approvers per payload hash. Approvals are counted at release time
    ///      against the *current* attestor set, so removing a compromised
    ///      attestor retroactively invalidates its approvals (PRD V6).
    mapping(bytes32 => address[]) internal _approvers;
    mapping(bytes32 => mapping(address => bool)) public hasApproved;

    /// @notice Total token units released so far (for the invariant monitor:
    ///         balanceOf(vault) + totalReleased == totalSupply).
    uint256 public totalReleased;

    // ---------------------------------------------------------------- modifiers

    modifier onlyAdmin() {
        if (msg.sender != admin) revert NotAdmin();
        _;
    }

    modifier onlyAttestor() {
        if (!isAttestor[msg.sender]) revert NotAttestor();
        _;
    }

    // ---------------------------------------------------------------- setup

    constructor(
        address admin_,
        address guardian_,
        address[] memory attestors_,
        uint256 threshold_,
        uint256 conversionFactor_,
        uint256 perReleaseCap_,
        uint256 dailyCap_,
        uint256 earliestSweepTimestamp_
    ) {
        if (admin_ == address(0) || guardian_ == address(0)) revert ZeroAddress();
        if (threshold_ < 2 || threshold_ > attestors_.length) revert InvalidThreshold();
        if (conversionFactor_ == 0) revert ZeroAmount();

        admin = admin_;
        guardian = guardian_;
        threshold = threshold_;
        conversionFactor = conversionFactor_;
        perReleaseCap = perReleaseCap_;
        dailyCap = dailyCap_;
        earliestSweepTimestamp = earliestSweepTimestamp_;

        for (uint256 i = 0; i < attestors_.length; i++) {
            address attestor = attestors_[i];
            if (attestor == address(0)) revert ZeroAddress();
            if (isAttestor[attestor]) revert DuplicateAttestor();
            isAttestor[attestor] = true;
            emit AttestorAdded(attestor);
        }
        attestorCount = attestors_.length;
    }

    /// @notice One-time wiring of the token, required because vault and token
    ///         reference each other: the vault is deployed first, then PEN
    ///         mints its full supply here, then the admin calls this.
    function setToken(IERC20 token_) external onlyAdmin {
        if (address(token) != address(0)) revert TokenAlreadySet();
        if (address(token_) == address(0)) revert ZeroAddress();
        uint256 supply = token_.totalSupply();
        if (supply == 0 || token_.balanceOf(address(this)) != supply) revert VaultMustHoldFullSupply();
        token = token_;
        emit TokenSet(address(token_));
    }

    // ---------------------------------------------------------------- attestation

    /// @notice Approve the release for Pendulum migration `nonce`. `palletAmount`
    ///         is the burned amount in pallet units (12 decimals), exactly as
    ///         emitted by the `MigrationInitiated` event; the vault converts.
    ///         Recording approvals stays possible while paused so that releases
    ///         resume without re-attestation after an unpause.
    function approve(uint64 nonce, address recipient, uint256 palletAmount) external onlyAttestor {
        if (recipient == address(0)) revert ZeroAddress();
        if (palletAmount == 0) revert ZeroAmount();
        if (nonceConsumed[nonce]) revert NonceAlreadyConsumed(nonce);

        bytes32 payload = payloadHash(nonce, recipient, palletAmount);
        if (hasApproved[payload][msg.sender]) revert AlreadyApproved(msg.sender);
        hasApproved[payload][msg.sender] = true;
        _approvers[payload].push(msg.sender);
        emit Approved(nonce, recipient, palletAmount, msg.sender);

        // Opportunistic release: skipped (not reverted) when paused or a cap
        // is hit, so the approval is recorded either way. `release` can be
        // called by anyone later to retry.
        if (!paused && address(token) != address(0) && activeApprovals(payload) >= threshold) {
            uint256 tokenAmount = palletAmount * conversionFactor;
            if (tokenAmount <= perReleaseCap && _releasedTodayAfterRoll() + tokenAmount <= dailyCap) {
                _release(nonce, recipient, palletAmount);
            }
        }
    }

    /// @notice Execute a sufficiently-approved release. Callable by anyone;
    ///         used to retry releases deferred by pause or caps.
    function release(uint64 nonce, address recipient, uint256 palletAmount) external {
        if (paused) revert EnforcedPause();
        if (address(token) == address(0)) revert TokenNotSet();
        if (nonceConsumed[nonce]) revert NonceAlreadyConsumed(nonce);

        bytes32 payload = payloadHash(nonce, recipient, palletAmount);
        uint256 active = activeApprovals(payload);
        if (active < threshold) revert NotEnoughApprovals(active, threshold);

        uint256 tokenAmount = palletAmount * conversionFactor;
        if (tokenAmount > perReleaseCap) revert ExceedsPerReleaseCap(tokenAmount, perReleaseCap);
        uint256 releasedAfter = _releasedTodayAfterRoll() + tokenAmount;
        if (releasedAfter > dailyCap) revert ExceedsDailyCap(releasedAfter, dailyCap);

        _release(nonce, recipient, palletAmount);
    }

    /// @dev Caller must have verified pause state, approvals and caps.
    function _release(uint64 nonce, address recipient, uint256 palletAmount) internal {
        uint256 tokenAmount = palletAmount * conversionFactor;
        nonceConsumed[nonce] = true;
        releasedToday += tokenAmount;
        totalReleased += tokenAmount;
        token.safeTransfer(recipient, tokenAmount);
        emit Released(nonce, recipient, palletAmount, tokenAmount);
    }

    // ---------------------------------------------------------------- views

    function payloadHash(uint64 nonce, address recipient, uint256 palletAmount) public pure returns (bytes32) {
        return keccak256(abi.encode(nonce, recipient, palletAmount));
    }

    /// @notice Approvals for a payload counted against the current attestor
    ///         set. Removed attestors no longer count; re-added ones do.
    function activeApprovals(bytes32 payload) public view returns (uint256 count) {
        address[] storage approvers = _approvers[payload];
        for (uint256 i = 0; i < approvers.length; i++) {
            if (isAttestor[approvers[i]]) count++;
        }
    }

    function approversOf(bytes32 payload) external view returns (address[] memory) {
        return _approvers[payload];
    }

    /// @dev Rolls the daily accounting window forward if a new UTC day started.
    function _releasedTodayAfterRoll() internal returns (uint256) {
        uint256 day = block.timestamp / 1 days;
        if (day != currentDay) {
            currentDay = day;
            releasedToday = 0;
        }
        return releasedToday;
    }

    // ---------------------------------------------------------------- pause

    function pause() external {
        if (msg.sender != guardian && msg.sender != admin) revert NotGuardianOrAdmin();
        if (paused) revert EnforcedPause();
        paused = true;
        emit Paused(msg.sender);
    }

    function unpause() external onlyAdmin {
        if (!paused) revert NotPaused();
        paused = false;
        emit Unpaused(msg.sender);
    }

    // ---------------------------------------------------------------- admin

    function addAttestor(address attestor) external onlyAdmin {
        if (attestor == address(0)) revert ZeroAddress();
        if (isAttestor[attestor]) revert DuplicateAttestor();
        isAttestor[attestor] = true;
        attestorCount += 1;
        emit AttestorAdded(attestor);
    }

    function removeAttestor(address attestor) external onlyAdmin {
        if (!isAttestor[attestor]) revert UnknownAttestor();
        if (attestorCount - 1 < threshold) revert ThresholdWouldExceedAttestors();
        isAttestor[attestor] = false;
        attestorCount -= 1;
        emit AttestorRemoved(attestor);
    }

    function setThreshold(uint256 threshold_) external onlyAdmin {
        if (threshold_ < 2 || threshold_ > attestorCount) revert InvalidThreshold();
        threshold = threshold_;
        emit ThresholdUpdated(threshold_);
    }

    function setCaps(uint256 perReleaseCap_, uint256 dailyCap_) external onlyAdmin {
        perReleaseCap = perReleaseCap_;
        dailyCap = dailyCap_;
        emit CapsUpdated(perReleaseCap_, dailyCap_);
    }

    function setGuardian(address guardian_) external onlyAdmin {
        if (guardian_ == address(0)) revert ZeroAddress();
        guardian = guardian_;
        emit GuardianUpdated(guardian_);
    }

    function transferAdmin(address newAdmin) external onlyAdmin {
        if (newAdmin == address(0)) revert ZeroAddress();
        pendingAdmin = newAdmin;
        emit AdminTransferStarted(newAdmin);
    }

    function acceptAdmin() external {
        if (msg.sender != pendingAdmin) revert NotPendingAdmin();
        admin = msg.sender;
        pendingAdmin = address(0);
        emit AdminTransferred(msg.sender);
    }

    /// @notice Sweep the unmigrated remainder after the migration window
    ///         closes (destination decided by governance, PRD D5).
    function sweepRemainder(address to) external onlyAdmin {
        if (block.timestamp < earliestSweepTimestamp) revert SweepNotYetAllowed(earliestSweepTimestamp);
        if (to == address(0)) revert ZeroAddress();
        if (address(token) == address(0)) revert TokenNotSet();
        uint256 balance = token.balanceOf(address(this));
        token.safeTransfer(to, balance);
        emit RemainderSwept(to, balance);
    }
}
