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
pub enum ChainExtensionErr {
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
	Token(PalletAssetTokenErr),
	/// An arithmetic error.
	Arithmetic(PalletAssetArithmeticErr),
	/// Unknown error
	Unknown,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
pub enum PalletAssetArithmeticErr {
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
pub enum PalletAssetTokenErr {
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

impl From<DispatchError> for ChainExtensionErr {
	fn from(e: DispatchError) -> Self {
		match e {
			DispatchError::Other(_) => ChainExtensionErr::Other,
			DispatchError::CannotLookup => ChainExtensionErr::CannotLookup,
			DispatchError::BadOrigin => ChainExtensionErr::BadOrigin,
			DispatchError::Module(_) => ChainExtensionErr::Module,
			DispatchError::ConsumerRemaining => ChainExtensionErr::ConsumerRemaining,
			DispatchError::NoProviders => ChainExtensionErr::NoProviders,
			DispatchError::TooManyConsumers => ChainExtensionErr::TooManyConsumers,
			DispatchError::Token(token_err) =>
				ChainExtensionErr::Token(PalletAssetTokenErr::from(token_err)),
			DispatchError::Arithmetic(arithmetic_error) =>
				ChainExtensionErr::Arithmetic(PalletAssetArithmeticErr::from(arithmetic_error)),
			_ => ChainExtensionErr::Unknown,
		}
	}
}

impl From<ArithmeticError> for PalletAssetArithmeticErr {
	fn from(e: ArithmeticError) -> Self {
		match e {
			ArithmeticError::Underflow => PalletAssetArithmeticErr::Underflow,
			ArithmeticError::Overflow => PalletAssetArithmeticErr::Overflow,
			ArithmeticError::DivisionByZero => PalletAssetArithmeticErr::DivisionByZero,
		}
	}
}

impl From<TokenError> for PalletAssetTokenErr {
	fn from(e: TokenError) -> Self {
		match e {
			TokenError::NoFunds => PalletAssetTokenErr::NoFunds,
			TokenError::WouldDie => PalletAssetTokenErr::WouldDie,
			TokenError::BelowMinimum => PalletAssetTokenErr::BelowMinimum,
			TokenError::CannotCreate => PalletAssetTokenErr::CannotCreate,
			TokenError::UnknownAsset => PalletAssetTokenErr::UnknownAsset,
			TokenError::Frozen => PalletAssetTokenErr::Frozen,
			TokenError::Unsupported => PalletAssetTokenErr::Unsupported,
		}
	}
}

pub fn try_from(type_id: u8, code: [u8; 12], issuer: [u8; 32]) -> Result<CurrencyId, ()> {
	match type_id {
		0 => {
			let foreign_currency_id = code[0];
			match foreign_currency_id {
				0 => Ok(CurrencyId::XCM(0)),
				1 => Ok(CurrencyId::XCM(1)),
				2 => Ok(CurrencyId::XCM(2)),
				3 => Ok(CurrencyId::XCM(3)),
				4 => Ok(CurrencyId::XCM(4)),
				5 => Ok(CurrencyId::XCM(5)),
				6 => Ok(CurrencyId::XCM(6)),
				7 => Ok(CurrencyId::XCM(7)),
				8 => Ok(CurrencyId::XCM(8)),
				9 => Ok(CurrencyId::XCM(9)),
				10 => Ok(CurrencyId::XCM(10)),
				11 => Ok(CurrencyId::XCM(11)),
				12 => Ok(CurrencyId::XCM(12)),
				13 => Ok(CurrencyId::XCM(13)),
				14 => Ok(CurrencyId::XCM(14)),
				15 => Ok(CurrencyId::XCM(15)),
				16 => Ok(CurrencyId::XCM(16)),
				_ => Err(()),
			}
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
