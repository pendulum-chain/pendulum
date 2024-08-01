use crate::asset_registry::{CustomMetadata, DiaKeys};
use frame_benchmarking::v2::benchmarks;
use frame_support::assert_ok;
use frame_support::traits::ConstU32;
use frame_system::RawOrigin;
use orml_asset_registry::AssetMetadata;
use sp_runtime::BoundedVec;
use sp_std::{vec, vec::Vec};
use spacewalk_primitives::CurrencyId;
use xcm::{
	latest::MultiLocation,
	opaque::lts::{Junction::*, Junctions::*},
};

pub struct Pallet<T: Config>(orml_asset_registry::Pallet<T>);
pub trait Config:
	orml_asset_registry::Config<CustomMetadata = CustomMetadata, Balance = u128, AssetId = CurrencyId>
{
}

#[benchmarks]
pub mod benchmarks {
	use super::{Config, Pallet, *};
	use orml_asset_registry::Call;
	use crate::asset_registry::StringLimit;

	fn longest_vec<T: sp_core::Get<u32>>() -> BoundedVec<u8, T> {
		// there is no fixed upperbound, but all actions are root-only so an assumed upperbound of 128 will do
		let longest_vec = vec![b'a', 128];
		BoundedVec::truncate_from(longest_vec)
	}

	fn longest_multilocation() -> MultiLocation {
		let key = GeneralKey { length: 32, data: [0; 32] };
		MultiLocation::new(1, X8(key, key, key, key, key, key, key, key))
	}

	fn get_asset_metadata<T :sp_core::Get<u32>>() -> AssetMetadata<u128, CustomMetadata, T> {
		AssetMetadata::<u128, CustomMetadata, T> {
			decimals: 12,
			name: longest_vec::<T>(),
			symbol: longest_vec::<T>(),
			existential_deposit: 0,
			location: Some(longest_multilocation().into()),
			additional: CustomMetadata {
				dia_keys: DiaKeys {
					blockchain: longest_vec::<StringLimit>(),
					symbol: longest_vec::<StringLimit>(),
				},
				fee_per_second: 123,
			},
		}
	}

	#[benchmark]
	fn register_asset() {
		let metadata = get_asset_metadata();

		#[extrinsic_call]
		register_asset(RawOrigin::Root, metadata, Some(CurrencyId::Native));
	}

	#[benchmark]
	fn update_asset() {
		let metadata = get_asset_metadata::<<T as orml_asset_registry::Config>::StringLimit>();

		assert_ok!(orml_asset_registry::Pallet::<T>::register_asset(
			RawOrigin::Root.into(),
			metadata,
			Some(CurrencyId::Native),
		));

		// update values, and make sure to change the actual values in case there is some optimization
		// somewhere in the codepath
		let key = GeneralKey { length: 32, data: [1; 32] };
		let location = MultiLocation::new(1, X8(key, key, key, key, key, key, key, key));
		#[extrinsic_call]
		update_asset(
			RawOrigin::Root,
			CurrencyId::Native,
			Some(123),
			Some(BoundedVec::truncate_from(vec![b'b', 128])),
			Some(BoundedVec::truncate_from(vec![b'b', 128])),
			Some(1234),
			Some(Some(location.into())),
			Some(CustomMetadata {
				dia_keys: DiaKeys {
					blockchain: longest_vec(),
					symbol: longest_vec(),
				},
				fee_per_second: 123,
			}),
		);
	}

	#[benchmark]
	fn set_asset_location() {
		#[block]
		{
			// todo: remove this benchmark when this unused item is removed from the weight type upstream
		}
	}
}
