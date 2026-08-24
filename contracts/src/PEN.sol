// SPDX-License-Identifier: GPL-3.0-only
pragma solidity 0.8.26;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Permit} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Permit.sol";
import {ERC20Votes} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Votes.sol";
import {Nonces} from "@openzeppelin/contracts/utils/Nonces.sol";

/// @title PEN — the Pendulum token on Base
/// @notice Fixed-supply ERC-20. The entire maximum issuance is minted to the
///         MigrationVault at deployment; there is no mint function, no owner
///         and no upgradeability. Unmigrated supply sits in the vault and is
///         released as holders migrate from the Pendulum parachain.
///
///         Extensions (see docs/pen-token-contract-standards.md):
///         - ERC20Permit (EIP-2612): signature-based approvals
///         - ERC20Votes (EIP-5805): checkpointed voting power + delegation,
///           with the EIP-6372 clock in timestamp mode
contract PEN is ERC20, ERC20Permit, ERC20Votes {
    error ZeroVault();
    error ZeroIssuance();

    /// @param vault The MigrationVault that receives the full supply.
    /// @param maxIssuance The maximum issuance of PEN, in 18-decimal units
    ///        (PRD open decision D3 fixes the exact figure at deployment).
    constructor(address vault, uint256 maxIssuance) ERC20("Pendulum", "PEN") ERC20Permit("Pendulum") {
        if (vault == address(0)) revert ZeroVault();
        if (maxIssuance == 0) revert ZeroIssuance();
        _mint(vault, maxIssuance);
    }

    /// @dev EIP-6372 clock in timestamp mode (PRD T1). The Governor contract
    ///      must be deployed with the same clock mode.
    function clock() public view override returns (uint48) {
        return uint48(block.timestamp);
    }

    /// @dev EIP-6372 machine-readable clock description.
    // solhint-disable-next-line func-name-mixedcase
    function CLOCK_MODE() public pure override returns (string memory) {
        return "mode=timestamp";
    }

    // ----- required overrides for the ERC20Permit/ERC20Votes composition -----

    function _update(address from, address to, uint256 value) internal override(ERC20, ERC20Votes) {
        super._update(from, to, value);
    }

    function nonces(address owner) public view override(ERC20Permit, Nonces) returns (uint256) {
        return super.nonces(owner);
    }
}
