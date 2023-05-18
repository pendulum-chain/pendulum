use crate::*;
use sp_core::{Decode, Encode, MaxEncodedLen};
use sp_runtime::{ArithmeticError, TokenError, codec};
use scale_info::prelude::vec::Vec;
use dia_oracle::dia;

pub use spacewalk_primitives::{Asset, CurrencyId};

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
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
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
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
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
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
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

pub type Blockchain = [u8; 32];
pub type Symbol = [u8; 32];

pub trait ToTrimmedVec {
	fn to_trimmed_vec(&self) -> Vec<u8>;
}
impl ToTrimmedVec for [u8; 32] {
	fn to_trimmed_vec(&self) -> Vec<u8> {
		trim_trailing_zeros(self).to_vec()
	}
}

fn trim_trailing_zeros(slice: &[u8]) -> &[u8] {
	let mut trim_amount = 0;
	for el in slice.iter().rev() {
		if *el == 0 {
			trim_amount += 1;
		} else {
			break
		}
	}
	&slice[..slice.len() - trim_amount]
}

#[derive(Debug, Clone, PartialEq, Eq, codec::Encode, codec::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub struct CoinInfo {
	pub symbol: Vec<u8>,
	pub name: Vec<u8>,
	pub blockchain: Vec<u8>,
	pub supply: u128,
	pub last_update_timestamp: u64,
	pub price: u128,
}
impl From<dia::CoinInfo> for CoinInfo {
	fn from(coin_info: dia::CoinInfo) -> Self {
		Self {
			symbol: coin_info.symbol,
			name: coin_info.name,
			blockchain: coin_info.blockchain,
			supply: coin_info.supply,
			last_update_timestamp: coin_info.last_update_timestamp,
			price: coin_info.price,
		}
	}
}
