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
    error ExceedsDailyCap(uint256 requested, uint256 available);
    error SweepNotYetAllowed(uint256 earliest);
    error PendingNotStale();
    error InsufficientVaultBalance();
    error ExceedsSweepable(uint256 requested, uint256 sweepable);
    error SweepSettlingAfterThresholdCut(uint256 allowedFrom);

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
    event ReleasePending(uint64 indexed nonce, address indexed recipient, uint256 tokenAmount);
    event StalePendingCleared(bytes32 indexed payload, uint256 tokenAmount);

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
    /// @notice Maximum token units released in any rolling 24h window (PRD V4).
    ///         Enforced as a leaky bucket of capacity `dailyCap` that refills
    ///         linearly at `dailyCap` per day: a burst is capped at `dailyCap`
    ///         and a second burst must wait for the bucket to refill. There is
    ///         no instant reset at a calendar boundary.
    uint256 public dailyCap;
    /// @dev Consumed allowance recorded at `windowUpdatedAt`, before decay.
    uint256 public windowConsumed;
    uint256 public windowUpdatedAt;

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
    mapping(bytes32 => mapping(address => bool)) internal _inApprovers;

    /// @dev Generation of each attestor address, bumped on every addAttestor.
    ///      An approval only counts while its recorded generation matches the
    ///      attestor's current one, so approvals from before a removal can
    ///      never count again after a re-add — the threshold can only ever be
    ///      crossed inside approve(), which maintains the pending-release
    ///      accounting that protects sweepRemainder.
    mapping(address => uint64) public attestorGeneration;
    mapping(bytes32 => mapping(address => uint64)) internal _approvalGeneration;

    /// @notice Total token units released so far (for the invariant monitor:
    ///         balanceOf(vault) + totalReleased == totalSupply).
    uint256 public totalReleased;

    /// @notice Token units owed to threshold-approved payloads whose release
    ///         was deferred (pause or caps). Excluded from `sweepRemainder`
    ///         so a sweep can never strand an already-earned release.
    uint256 public pendingApprovedAmount;
    mapping(bytes32 => bool) public pendingRelease;

    /// @notice Total token units swept out via `sweepRemainder`. Tracked so
    ///         the invariant monitor's conservation check stays exact after a
    ///         window-close sweep: balanceOf(vault) + totalReleased +
    ///         totalSwept == totalSupply at all times.
    uint256 public totalSwept;

    /// @notice Timestamp of the last threshold *decrease*. `sweepRemainder` is
    ///         blocked for `SWEEP_SETTLING_PERIOD` afterwards: lowering the
    ///         threshold can retroactively make a sub-threshold payload
    ///         releasable without registering it in `pendingApprovedAmount`
    ///         (that accounting is maintained only inside `approve()`), so the
    ///         delay gives the monitor and a permissionless `release()` time to
    ///         settle any newly-qualifying payload before a sweep could strand
    ///         it. See runbooks RB-6/RB-7.
    uint256 public thresholdReducedAt;
    uint256 public constant SWEEP_SETTLING_PERIOD = 7 days;

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
            attestorGeneration[attestor] = 1;
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
        uint64 generation = attestorGeneration[msg.sender];
        if (_approvalGeneration[payload][msg.sender] == generation) revert AlreadyApproved(msg.sender);
        _approvalGeneration[payload][msg.sender] = generation;
        if (!_inApprovers[payload][msg.sender]) {
            _inApprovers[payload][msg.sender] = true;
            _approvers[payload].push(msg.sender);
        }
        emit Approved(nonce, recipient, palletAmount, msg.sender);

        // Opportunistic release: skipped (not reverted) when paused or a cap
        // is hit, so the approval is recorded either way. `release` can be
        // called by anyone later to retry.
        if (activeApprovals(payload) >= threshold) {
            uint256 tokenAmount = palletAmount * conversionFactor;
            // Insufficient balance is included here deliberately: if the vault
            // was over-swept, the release is deferred (marked pending) rather
            // than reverting. A revert here would roll back this approval and,
            // because every attestor hits it identically, permanently
            // crash-loop the fleet on this block. Deferral keeps the debt
            // tracked and recoverable once the vault is refunded.
            bool releasable = !paused && address(token) != address(0) && tokenAmount <= perReleaseCap
                && tokenAmount <= availableDailyAllowance()
                && token.balanceOf(address(this)) >= tokenAmount;
            if (releasable) {
                _release(nonce, recipient, palletAmount, payload);
            } else if (!pendingRelease[payload]) {
                // Threshold reached but deferred: account for the owed amount
                // so `sweepRemainder` cannot strand it.
                pendingRelease[payload] = true;
                pendingApprovedAmount += tokenAmount;
                emit ReleasePending(nonce, recipient, tokenAmount);
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
        uint256 available = availableDailyAllowance();
        if (tokenAmount > available) revert ExceedsDailyCap(tokenAmount, available);
        if (token.balanceOf(address(this)) < tokenAmount) revert InsufficientVaultBalance();

        _release(nonce, recipient, palletAmount, payload);
    }

    /// @dev Caller must have verified pause state, approvals and caps.
    function _release(uint64 nonce, address recipient, uint256 palletAmount, bytes32 payload) internal {
        uint256 tokenAmount = palletAmount * conversionFactor;
        nonceConsumed[nonce] = true;
        if (pendingRelease[payload]) {
            pendingRelease[payload] = false;
            pendingApprovedAmount -= tokenAmount;
        }
        windowConsumed = _decayedConsumed() + tokenAmount;
        windowUpdatedAt = block.timestamp;
        totalReleased += tokenAmount;
        token.safeTransfer(recipient, tokenAmount);
        emit Released(nonce, recipient, palletAmount, tokenAmount);
    }

    /// @notice Clear the pending-release accounting of a payload whose nonce
    ///         was released via a DIFFERENT (conflicting) tuple. Restricted to
    ///         consumed nonces: an unconsumed pending payload is still owed to
    ///         its migrator and must never be cleared.
    function clearStalePending(uint64 nonce, address recipient, uint256 palletAmount) external onlyAdmin {
        if (!nonceConsumed[nonce]) revert PendingNotStale();
        bytes32 payload = payloadHash(nonce, recipient, palletAmount);
        if (!pendingRelease[payload]) revert PendingNotStale();
        pendingRelease[payload] = false;
        uint256 tokenAmount = palletAmount * conversionFactor;
        pendingApprovedAmount -= tokenAmount;
        emit StalePendingCleared(payload, tokenAmount);
    }

    // ---------------------------------------------------------------- views

    function payloadHash(uint64 nonce, address recipient, uint256 palletAmount) public pure returns (bytes32) {
        return keccak256(abi.encode(nonce, recipient, palletAmount));
    }

    /// @notice Approvals for a payload counted against the current attestor
    ///         set and generation. Removed attestors no longer count, and a
    ///         re-added attestor must approve again (its pre-removal approval
    ///         belongs to an older generation).
    function activeApprovals(bytes32 payload) public view returns (uint256 count) {
        address[] storage approvers = _approvers[payload];
        for (uint256 i = 0; i < approvers.length; i++) {
            address approver = approvers[i];
            if (isAttestor[approver] && _approvalGeneration[payload][approver] == attestorGeneration[approver]) {
                count++;
            }
        }
    }

    /// @notice Whether `attestor` holds a currently-valid approval for the
    ///         payload — i.e. it is a current attestor and its approval is
    ///         from its current generation. Mirrors the conditions
    ///         `activeApprovals` counts, so a removed attestor reports false.
    function hasApproved(bytes32 payload, address attestor) public view returns (bool) {
        return isAttestor[attestor] && _approvalGeneration[payload][attestor] == attestorGeneration[attestor];
    }

    function approversOf(bytes32 payload) external view returns (address[] memory) {
        return _approvers[payload];
    }

    /// @dev Consumed allowance after linear refill since the last release.
    function _decayedConsumed() internal view returns (uint256) {
        uint256 refilled = ((block.timestamp - windowUpdatedAt) * dailyCap) / 1 days;
        return windowConsumed > refilled ? windowConsumed - refilled : 0;
    }

    /// @notice Token units releasable right now under the rolling daily cap.
    function availableDailyAllowance() public view returns (uint256) {
        uint256 consumed = _decayedConsumed();
        return dailyCap > consumed ? dailyCap - consumed : 0;
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
        // New generation: any approvals this address recorded before a prior
        // removal stop counting, so this call can never cross a threshold.
        attestorGeneration[attestor] += 1;
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
        // A decrease can retroactively qualify a sub-threshold payload without
        // routing through approve() (which maintains pendingApprovedAmount);
        // gate sweeps for a settling period so it can be detected and released
        // first (round-4 finding).
        if (threshold_ < threshold) thresholdReducedAt = block.timestamp;
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

    /// @notice Sweep up to `amount` of the unmigrated remainder after the
    ///         migration window closes (destination decided by governance,
    ///         PRD D5). The caller must pass an explicit amount, bounded by
    ///         `balance − pendingApprovedAmount`, forcing a conscious
    ///         reconciliation against the monitor's outstanding-nonce count
    ///         (runbook RB-7) rather than blindly sweeping everything —
    ///         `pendingApprovedAmount` only reserves threshold-approved
    ///         releases, not migrations still gathering approvals.
    function sweepRemainder(address to, uint256 amount) external onlyAdmin {
        if (block.timestamp < earliestSweepTimestamp) revert SweepNotYetAllowed(earliestSweepTimestamp);
        if (block.timestamp < thresholdReducedAt + SWEEP_SETTLING_PERIOD) {
            revert SweepSettlingAfterThresholdCut(thresholdReducedAt + SWEEP_SETTLING_PERIOD);
        }
        if (to == address(0)) revert ZeroAddress();
        if (address(token) == address(0)) revert TokenNotSet();
        uint256 balanceHeld = token.balanceOf(address(this));
        // Saturating: a prior over-sweep can leave pending > balance; never revert on underflow.
        uint256 sweepable = balanceHeld > pendingApprovedAmount ? balanceHeld - pendingApprovedAmount : 0;
        if (amount > sweepable) revert ExceedsSweepable(amount, sweepable);
        totalSwept += amount;
        token.safeTransfer(to, amount);
        emit RemainderSwept(to, amount);
    }
}
