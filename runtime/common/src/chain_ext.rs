use crate::*;
use sp_core::{Decode, Encode, MaxEncodedLen};
use sp_runtime::{ArithmeticError, TokenError};
use spacewalk_primitives::{Asset, CurrencyId};
#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
pub enum OriginType {
	Caller,
	Address,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
struct PalletAssetRequest {
	origin_type: OriginType,
	asset_id: u32,
	target_address: [u8; 32],
	amount: u128,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
struct PalletAssetBalanceRequest {
	asset_id: u32,
	address: [u8; 32],
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
pub enum ChainExtensionError {
	/// Some error occurred.
	Other,
	/// Failed to lookup some data.
	CannotLookup,
	/// A bad origin.
	BadOrigin,
	/// A custom error in a module.
	Module,
	/// At least one consumer is remaining so the account cannot be destroyed.
	ConsumerRemaining,
	/// There are no providers so the account cannot be created.
	NoProviders,
	/// There are too many consumers so the account cannot be created.
	TooManyConsumers,
	/// An error to do with tokens.
	Token(PalletAssetTokenError),
	/// An arithmetic error.
	Arithmetic(PalletAssetArithmeticError),
	/// Unknown error
	Unknown,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
pub enum PalletAssetArithmeticError {
	/// Underflow.
	Underflow,
	/// Overflow.
	Overflow,
	/// Division by zero.
	DivisionByZero,
	/// Unknown error
	Unknown,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
pub enum PalletAssetTokenError {
	/// Funds are unavailable.
	NoFunds,
	/// Account that must exist would die.
	WouldDie,
	/// Account cannot exist with the funds that would be given.
	BelowMinimum,
	/// Account cannot be created.
	CannotCreate,
	/// The asset in question is unknown.
	UnknownAsset,
	/// Funds exist but are frozen.
	Frozen,
	/// Operation is not supported by the asset.
	Unsupported,
	/// Unknown error
	Unknown,
}

impl From<DispatchError> for ChainExtensionError {
	fn from(e: DispatchError) -> Self {
		match e {
			DispatchError::Other(_) => ChainExtensionError::Other,
			DispatchError::CannotLookup => ChainExtensionError::CannotLookup,
			DispatchError::BadOrigin => ChainExtensionError::BadOrigin,
			DispatchError::Module(_) => ChainExtensionError::Module,
			DispatchError::ConsumerRemaining => ChainExtensionError::ConsumerRemaining,
			DispatchError::NoProviders => ChainExtensionError::NoProviders,
			DispatchError::TooManyConsumers => ChainExtensionError::TooManyConsumers,
			DispatchError::Token(token_err) =>
				ChainExtensionError::Token(PalletAssetTokenError::from(token_err)),
			DispatchError::Arithmetic(arithmetic_error) =>
				ChainExtensionError::Arithmetic(PalletAssetArithmeticError::from(arithmetic_error)),
			_ => ChainExtensionError::Unknown,
		}
	}
}

impl From<ArithmeticError> for PalletAssetArithmeticError {
	fn from(e: ArithmeticError) -> Self {
		match e {
			ArithmeticError::Underflow => PalletAssetArithmeticError::Underflow,
			ArithmeticError::Overflow => PalletAssetArithmeticError::Overflow,
			ArithmeticError::DivisionByZero => PalletAssetArithmeticError::DivisionByZero,
		}
	}
}

impl From<TokenError> for PalletAssetTokenError {
	fn from(e: TokenError) -> Self {
		match e {
			TokenError::NoFunds => PalletAssetTokenError::NoFunds,
			TokenError::WouldDie => PalletAssetTokenError::WouldDie,
			TokenError::BelowMinimum => PalletAssetTokenError::BelowMinimum,
			TokenError::CannotCreate => PalletAssetTokenError::CannotCreate,
			TokenError::UnknownAsset => PalletAssetTokenError::UnknownAsset,
			TokenError::Frozen => PalletAssetTokenError::Frozen,
			TokenError::Unsupported => PalletAssetTokenError::Unsupported,
		}
	}
}

pub fn try_get_currency_id_from(
	type_id: u8,
	code: [u8; 12],
	issuer: [u8; 32],
) -> Result<CurrencyId, ()> {
	match type_id {
		0 => {
			let foreign_currency_id = code[0];
			Ok(CurrencyId::XCM(foreign_currency_id))
		},
		1 => Ok(CurrencyId::Native),
		2 => Ok(CurrencyId::StellarNative),
		3 => {
			let code = [code[0], code[1], code[2], code[3]];
			Ok(CurrencyId::Stellar(Asset::AlphaNum4 { code, issuer }))
		},
		4 => Ok(CurrencyId::Stellar(Asset::AlphaNum12 { code, issuer })),
		_ => Err(()),
	}
}
