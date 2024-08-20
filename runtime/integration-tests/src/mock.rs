use crate::{definitions::asset_hub};
use amplitude_runtime::CurrencyId as AmplitudeCurrencyId;
use pendulum_runtime::CurrencyId as PendulumCurrencyId;
use polkadot_core_primitives::Balance;
use codec::Encode;
use frame_support::BoundedVec;
use runtime_common::asset_registry::{CustomMetadata, DiaKeys, StringLimit};
use xcm::{v3::MultiLocation, VersionedMultiLocation};
use pendulum_runtime::definitions::moonbeam;


pub const UNIT: Balance = 1_000_000_000_000;
pub const TEN_UNITS: Balance = 10_000_000_000_000;

pub const USDT_ASSET_ID: u32 = 1984; //Real USDT Asset ID of both Polkadot's and Kusama's Asset Hub
pub const INCORRECT_ASSET_ID: u32 = 0; //asset id that pendulum/amplitude does NOT SUPPORT

pub fn units(amount: Balance) -> Balance {
	amount * 10u128.saturating_pow(9)
}
pub fn assets_metadata_for_registry_pendulum() -> Vec<(PendulumCurrencyId, Vec<u8>)> {
	vec![
		(
			PendulumCurrencyId::Native,
			orml_asset_registry::AssetMetadata {
				decimals: 12u32,
				name: BoundedVec::<u8, StringLimit>::truncate_from("Pendulum".as_bytes().to_vec()),
				symbol: BoundedVec::<u8, StringLimit>::truncate_from("PEN".as_bytes().to_vec()),
				existential_deposit: 1_000u128,
				location: Some(VersionedMultiLocation::V3(MultiLocation::new(
					0u8,
					xcm::latest::Junctions::X1(xcm::latest::Junction::PalletInstance(10)),
				))),
				additional: CustomMetadata {
					dia_keys: DiaKeys::<StringLimit> {
						blockchain: BoundedVec::truncate_from(vec![1, 2, 3]),
						symbol: BoundedVec::truncate_from(vec![1, 2, 3]),
					},
					fee_per_second: UNIT,
				},
			}
			.encode(),
		),
		(
			PendulumCurrencyId::XCM(1),
			orml_asset_registry::AssetMetadata {
				decimals: 12u32,
				name: BoundedVec::<u8, StringLimit>::truncate_from("USDT Assethub".as_bytes().to_vec()),
				symbol: BoundedVec::<u8, StringLimit>::truncate_from("USDT".as_bytes().to_vec()),
				existential_deposit: 1_000u128,
				location: Some(VersionedMultiLocation::V3(asset_hub::USDT_location())),
				additional: CustomMetadata {
					dia_keys: DiaKeys::<StringLimit> {
						blockchain: BoundedVec::truncate_from(vec![1, 2, 3]),
						symbol: BoundedVec::truncate_from(vec![1, 2, 3]),
					},
					fee_per_second: UNIT / 2,
				},
			}
			.encode(),
		),
		(
			PendulumCurrencyId::XCM(0),
			orml_asset_registry::AssetMetadata {
				decimals: 12u32,
				name: BoundedVec::<u8, StringLimit>::truncate_from("Polkadot".as_bytes().to_vec()),
				symbol: BoundedVec::<u8, StringLimit>::truncate_from("DOT".as_bytes().to_vec()),
				existential_deposit: 1_000u128,
				location: Some(VersionedMultiLocation::V3(MultiLocation::parent())),
				additional: CustomMetadata {
					dia_keys: DiaKeys::<StringLimit> {
						blockchain: BoundedVec::truncate_from(vec![1, 2, 3]),
						symbol: BoundedVec::truncate_from(vec![1, 2, 3]),
					},
					fee_per_second: UNIT / 4,
				},
			}
			.encode(),
		),
		(
			PendulumCurrencyId::XCM(6),
			orml_asset_registry::AssetMetadata {
				decimals: 12u32,
				name: BoundedVec::<u8, StringLimit>::truncate_from("Moonbeam BRZ".as_bytes().to_vec()),
				symbol: BoundedVec::<u8, StringLimit>::truncate_from("BRZ".as_bytes().to_vec()),
				existential_deposit: 1_000u128,
				location: Some(xcm::VersionedMultiLocation::V3(moonbeam::BRZ_location())),
				additional: CustomMetadata {
					dia_keys: DiaKeys::<StringLimit> {
						blockchain: BoundedVec::truncate_from(vec![1, 2, 3]),
						symbol: BoundedVec::truncate_from(vec![1, 2, 3]),
					},
					fee_per_second: 2 * UNIT,
				},
			}
			.encode(),
		),
	]
}

pub fn assets_metadata_for_registry_amplitude() -> Vec<(AmplitudeCurrencyId, Vec<u8>)> {
	vec![
		(
			AmplitudeCurrencyId::Native,
			orml_asset_registry::AssetMetadata {
				decimals: 12u32,
				name: BoundedVec::<u8, StringLimit>::truncate_from("Amplitude".as_bytes().to_vec()),
				symbol: BoundedVec::<u8, StringLimit>::truncate_from("AMPE".as_bytes().to_vec()),
				existential_deposit: 1_000u128,
				location: Some(VersionedMultiLocation::V3(MultiLocation::new(
					0u8,
					xcm::latest::Junctions::X1(xcm::latest::Junction::PalletInstance(10)),
				))),
				additional: CustomMetadata {
					dia_keys: DiaKeys::<StringLimit> {
						blockchain: BoundedVec::truncate_from(vec![1, 2, 3]),
						symbol: BoundedVec::truncate_from(vec![1, 2, 3]),
					},
					fee_per_second: UNIT,
				},
			}
			.encode(),
		),
		(
			AmplitudeCurrencyId::XCM(1),
			orml_asset_registry::AssetMetadata {
				decimals: 12u32,
				name: BoundedVec::<u8, StringLimit>::truncate_from("USDT Assethub".as_bytes().to_vec()),
				symbol: BoundedVec::<u8, StringLimit>::truncate_from("USDT".as_bytes().to_vec()),
				existential_deposit: 1_000u128,
				location: Some(VersionedMultiLocation::V3(asset_hub::USDT_location())),
				additional: CustomMetadata {
					dia_keys: DiaKeys::<StringLimit> {
						blockchain: BoundedVec::truncate_from(vec![1, 2, 3]),
						symbol: BoundedVec::truncate_from(vec![1, 2, 3]),
					},
					fee_per_second: UNIT / 10,
				},
			}
			.encode(),
		),
		(
			AmplitudeCurrencyId::XCM(0),
			orml_asset_registry::AssetMetadata {
				decimals: 12u32,
				name: BoundedVec::<u8, StringLimit>::truncate_from("Kusama".as_bytes().to_vec()),
				symbol: BoundedVec::<u8, StringLimit>::truncate_from("KSM".as_bytes().to_vec()),
				existential_deposit: 1_000u128,
				location: Some(VersionedMultiLocation::V3(MultiLocation::parent())),
				additional: CustomMetadata {
					dia_keys: DiaKeys::<StringLimit> {
						blockchain: BoundedVec::truncate_from(vec![1, 2, 3]),
						symbol: BoundedVec::truncate_from(vec![1, 2, 3]),
					},
					fee_per_second: UNIT / 20,
				},
			}
			.encode(),
		),
	]
}

