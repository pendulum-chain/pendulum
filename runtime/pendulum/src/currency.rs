use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

#[derive(
	Encode,
	Decode,
	MaxEncodedLen,
	Eq,
	PartialEq,
	Copy,
	Clone,
	PartialOrd,
	RuntimeDebug,
	Ord,
	TypeInfo,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum CurrencyId {
	Native,
	XCM(u8),
}

impl Default for CurrencyId {
	fn default() -> Self {
		CurrencyId::Native
	}
}
