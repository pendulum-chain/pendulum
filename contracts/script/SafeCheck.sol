// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

// A 1-of-n Safe is a single key.
uint256 constant MIN_SAFE_THRESHOLD = 2;

/// @notice Refuses `account`, named `name` in the messages, unless it is a
///         Safe on this chain with a threshold of at least MIN_SAFE_THRESHOLD.
///         A Safe answers getThreshold() with its non-zero threshold. An
///         EIP-7702 delegated EOA stays a single key whatever it delegates to.
function requireMultisigSafe(address account, string memory name) view {
    require(account.code.length > 0, string.concat(name, " has no code on this chain (create the Safe first)"));
    require(bytes3(account.code) != 0xef0100, string.concat(name, " is an EIP-7702 delegated EOA, not a Safe"));
    (bool ok, bytes memory threshold) = account.staticcall(abi.encodeWithSignature("getThreshold()"));
    require(ok && threshold.length == 32, string.concat(name, " is not a Safe (getThreshold() failed)"));
    uint256 value = abi.decode(threshold, (uint256));
    require(value > 0, string.concat(name, " Safe has threshold 0 (not set up)"));
    require(value >= MIN_SAFE_THRESHOLD, string.concat(name, " Safe threshold is below 2"));
}
