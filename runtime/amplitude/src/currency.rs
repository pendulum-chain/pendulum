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
	XCM(ForeignCurrencyId),
}

impl Default for CurrencyId {
	fn default() -> Self {
		CurrencyId::Native
	}
}

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

pub enum ForeignCurrencyId {
	KSM,   // Kusama relay chain
	KAR,   // Karura
	AUSD,  // Karura
	BNC,   // Bifrost
	VsKSM, // Bifrost
	HKO,   // Heiko
	MOVR,  // Moonriver
	SDN,   // Shiden
	KINT,  // Kintsugi
	KBTC,  // Kintsugi
	GENS,  // Genshiro
	XOR,   // Sora
	TEER,  // Integritee
	KILT,  // KILT
	PHA,   // KHALA
	ZTG,   // Zeitgeist
	USD,   // Statemine
}
