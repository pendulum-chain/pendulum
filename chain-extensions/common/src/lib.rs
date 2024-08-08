#![cfg_attr(not(feature = "std"), no_std)]
#![allow(non_snake_case)]

use scale_info::prelude::vec::Vec;
use sp_core::{Decode, Encode, MaxEncodedLen};
use sp_runtime::{codec, ArithmeticError, DispatchError, TokenError};

/// Address is a type alias for easier readability of address (accountId) communicated between contract and chain extension.
pub type Address = [u8; 32];
/// Amount is a type alias for easier readability of amount communicated between contract and chain extension.
pub type Amount = u128;
/// Blockchain is a type alias for easier readability of dia blockchain name communicated between contract and chain extension.
pub type Blockchain = [u8; 32];
/// Symbol is a type alias for easier readability of dia blockchain symbol communicated between contract and chain extension.
pub type Symbol = [u8; 32];

/// ChainExtensionOutcome is almost the same as DispatchError, but with some modifications to make it compatible with being communicated between contract and chain extension. It implements the necessary From<T> conversions with DispatchError and other nested errors.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum ChainExtensionOutcome {
	/// Chain extension function executed correctly
	Success,
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
	/// Cannot decode
	DecodingError,
	/// Failed to save some data
	WriteError,
	/// Function id not implemented for chain extension
	UnimplementedFuncId,
	/// An error to do with tokens.
	Token(ChainExtensionTokenError),
	/// An arithmetic error.
	Arithmetic(ChainExtensionArithmeticError),
	/// Unknown error
	Unknown,
}

/// ChainExtensionTokenError is a nested error in ChainExtensionOutcome, similar to DispatchError's TokenError.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum ChainExtensionTokenError {
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
	/// Funds are unavailable.
	FundsUnavailable,
	/// Some part of the balance gives the only provider reference to the account and thus cannot
	/// be (re)moved.
	OnlyProvider,
	/// Account cannot be created for a held balance.
	CannotCreateHold,
	/// Withdrawal would cause unwanted loss of account.
	NotExpendable,
	/// Blocked
	Blocked,
	/// Unknown error
	Unknown,
}

/// ChainExtensionArithmeticError is a nested error in ChainExtensionOutcome, similar to DispatchError's ArithmeticError.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum ChainExtensionArithmeticError {
	/// Underflow.
	Underflow,
	/// Overflow.
	Overflow,
	/// Division by zero.
	DivisionByZero,
	/// Unknown error
	Unknown,
}

impl From<DispatchError> for ChainExtensionOutcome {
	fn from(e: DispatchError) -> Self {
		match e {
			DispatchError::Other(_) => ChainExtensionOutcome::Other,
			DispatchError::CannotLookup => ChainExtensionOutcome::CannotLookup,
			DispatchError::BadOrigin => ChainExtensionOutcome::BadOrigin,
			DispatchError::Module(_) => ChainExtensionOutcome::Module,
			DispatchError::ConsumerRemaining => ChainExtensionOutcome::ConsumerRemaining,
			DispatchError::NoProviders => ChainExtensionOutcome::NoProviders,
			DispatchError::TooManyConsumers => ChainExtensionOutcome::TooManyConsumers,
			DispatchError::Token(token_err) =>
				ChainExtensionOutcome::Token(ChainExtensionTokenError::from(token_err)),
			DispatchError::Arithmetic(arithmetic_error) => ChainExtensionOutcome::Arithmetic(
				ChainExtensionArithmeticError::from(arithmetic_error),
			),
			_ => ChainExtensionOutcome::Unknown,
		}
	}
}

impl From<TokenError> for ChainExtensionTokenError {
	fn from(e: TokenError) -> Self {
		match e {
			TokenError::BelowMinimum => ChainExtensionTokenError::BelowMinimum,
			TokenError::CannotCreate => ChainExtensionTokenError::CannotCreate,
			TokenError::UnknownAsset => ChainExtensionTokenError::UnknownAsset,
			TokenError::Frozen => ChainExtensionTokenError::Frozen,
			TokenError::FundsUnavailable => ChainExtensionTokenError::FundsUnavailable,
			TokenError::OnlyProvider => ChainExtensionTokenError::OnlyProvider,
			TokenError::CannotCreateHold => ChainExtensionTokenError::CannotCreateHold,
			TokenError::NotExpendable => ChainExtensionTokenError::NotExpendable,
			TokenError::Unsupported => ChainExtensionTokenError::Unsupported,
			TokenError::Blocked => ChainExtensionTokenError::Blocked,
		}
	}
}

impl From<ArithmeticError> for ChainExtensionArithmeticError {
	fn from(e: ArithmeticError) -> Self {
		match e {
			ArithmeticError::Underflow => ChainExtensionArithmeticError::Underflow,
			ArithmeticError::Overflow => ChainExtensionArithmeticError::Overflow,
			ArithmeticError::DivisionByZero => ChainExtensionArithmeticError::DivisionByZero,
		}
	}
}

/// ToTrimmedVec is a trait implemented for [u8; 32] to allow both types Blockchain and Symbol (which are [u8; 32]) to have the trim_trailing_zeros function.
pub trait ToTrimmedVec {
	fn to_trimmed_vec(&self) -> Vec<u8>;
}
impl ToTrimmedVec for [u8; 32] {
	fn to_trimmed_vec(&self) -> Vec<u8> {
		trim_trailing_zeros(self).to_vec()
	}
}

/// trim_trailing_zeros takes an input slice and returns it without the trailing zeros.
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

/// decode gets the slice from a Vec<u8> to decode it into its scale encoded type.
pub fn decode<T: Decode>(input: Vec<u8>) -> Result<T, codec::Error> {
	let mut input = input.as_slice();
	T::decode(&mut input)
}

impl ChainExtensionOutcome {
	pub fn as_u32(&self) -> u32 {
		match self {
			ChainExtensionOutcome::Success => 0,
			ChainExtensionOutcome::Other => 1,
			ChainExtensionOutcome::CannotLookup => 2,
			ChainExtensionOutcome::BadOrigin => 3,
			ChainExtensionOutcome::Module => 4,
			ChainExtensionOutcome::ConsumerRemaining => 5,
			ChainExtensionOutcome::NoProviders => 6,
			ChainExtensionOutcome::TooManyConsumers => 7,
			ChainExtensionOutcome::DecodingError => 8,
			ChainExtensionOutcome::WriteError => 9,
			ChainExtensionOutcome::UnimplementedFuncId => 10,
			ChainExtensionOutcome::Token(token_error) => 1000 + token_error.as_u32(),
			ChainExtensionOutcome::Arithmetic(arithmetic_error) => 2000 + arithmetic_error.as_u32(),
			ChainExtensionOutcome::Unknown => 999,
		}
	}
}

impl ChainExtensionTokenError {
	pub fn as_u32(&self) -> u32 {
		match self {
			ChainExtensionTokenError::BelowMinimum => 0,
			ChainExtensionTokenError::CannotCreate => 1,
			ChainExtensionTokenError::UnknownAsset => 2,
			ChainExtensionTokenError::Frozen => 3,
			ChainExtensionTokenError::Unsupported => 4,
			ChainExtensionTokenError::FundsUnavailable => 5,
			ChainExtensionTokenError::OnlyProvider => 6,
			ChainExtensionTokenError::CannotCreateHold => 7,
			ChainExtensionTokenError::NotExpendable => 8,
			ChainExtensionTokenError::Blocked => 9,
			ChainExtensionTokenError::Unknown => 999,
		}
	}
}

impl ChainExtensionArithmeticError {
	pub fn as_u32(&self) -> u32 {
		match self {
			ChainExtensionArithmeticError::Underflow => 0,
			ChainExtensionArithmeticError::Overflow => 1,
			ChainExtensionArithmeticError::DivisionByZero => 2,
			ChainExtensionArithmeticError::Unknown => 999,
		}
	}
}

impl TryFrom<u32> for ChainExtensionOutcome {
	type Error = DispatchError;

	fn try_from(value: u32) -> Result<Self, Self::Error> {
		match value {
			0 => Ok(ChainExtensionOutcome::Success),
			1 => Ok(ChainExtensionOutcome::Other),
			2 => Ok(ChainExtensionOutcome::CannotLookup),
			3 => Ok(ChainExtensionOutcome::BadOrigin),
			4 => Ok(ChainExtensionOutcome::Module),
			5 => Ok(ChainExtensionOutcome::ConsumerRemaining),
			6 => Ok(ChainExtensionOutcome::NoProviders),
			7 => Ok(ChainExtensionOutcome::TooManyConsumers),
			8 => Ok(ChainExtensionOutcome::DecodingError),
			9 => Ok(ChainExtensionOutcome::WriteError),
			10 => Ok(ChainExtensionOutcome::UnimplementedFuncId),
			999 => Ok(ChainExtensionOutcome::Unknown),
			1000..=1999 =>
				Ok(ChainExtensionOutcome::Token(ChainExtensionTokenError::try_from(value - 1000)?)),
			2000..=2999 => Ok(ChainExtensionOutcome::Arithmetic(
				ChainExtensionArithmeticError::try_from(value - 2000)?,
			)),
			_ => Err(DispatchError::Other("Invalid ChainExtensionOutcome value")),
		}
	}
}

impl TryFrom<u32> for ChainExtensionTokenError {
	type Error = DispatchError;

	fn try_from(value: u32) -> Result<Self, Self::Error> {
		match value {
			0 => Ok(ChainExtensionTokenError::BelowMinimum),
			1 => Ok(ChainExtensionTokenError::CannotCreate),
			2 => Ok(ChainExtensionTokenError::UnknownAsset),
			3 => Ok(ChainExtensionTokenError::Frozen),
			4 => Ok(ChainExtensionTokenError::Unsupported),
			5 => Ok(ChainExtensionTokenError::FundsUnavailable),
			6 => Ok(ChainExtensionTokenError::OnlyProvider),
			7 => Ok(ChainExtensionTokenError::CannotCreateHold),
			8 => Ok(ChainExtensionTokenError::NotExpendable),
			999 => Ok(ChainExtensionTokenError::Unknown),
			_ => Err(DispatchError::Other("Invalid ChainExtensionTokenError value")),
		}
	}
}

impl TryFrom<u32> for ChainExtensionArithmeticError {
	type Error = DispatchError;

	fn try_from(value: u32) -> Result<Self, Self::Error> {
		match value {
			0 => Ok(ChainExtensionArithmeticError::Underflow),
			1 => Ok(ChainExtensionArithmeticError::Overflow),
			2 => Ok(ChainExtensionArithmeticError::DivisionByZero),
			999 => Ok(ChainExtensionArithmeticError::Unknown),
			_ => Err(DispatchError::Other("Invalid ChainExtensionArithmeticError value")),
		}
	}
}
