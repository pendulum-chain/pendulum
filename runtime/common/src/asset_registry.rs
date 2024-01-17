use crate::*;
use frame_support::traits::AsEnsureOriginWithArg;
use frame_system::EnsureRoot;
use orml_traits::asset_registry::{AssetMetadata, AssetProcessor};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::Get;
use sp_runtime::{BoundedVec, DispatchError};
use sp_std::fmt::Debug;
use spacewalk_primitives::CurrencyId;

#[derive(Clone, PartialOrd, Ord, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct CustomMetadata<T: Get<u32> + TypeInfo + Clone + Eq + Debug + Send + Sync> {
	pub dia_keys: DiaKeys<T>,
	pub fee_per_second: u128,
}

#[derive(Clone, PartialOrd, Ord, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct DiaKeys<T: Get<u32> + TypeInfo + Clone + Eq + Debug + Send + Sync> {
	pub blockchain: BoundedVec<u8, T>,
	pub symbol: BoundedVec<u8, T>,
}

#[derive(
	Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen,
)]
pub struct CustomAssetProcessor;

impl<T> AssetProcessor<CurrencyId, AssetMetadata<Balance, CustomMetadata<T>>>
	for CustomAssetProcessor
where
	T: Get<u32> + TypeInfo + Clone + Eq + Debug + Send + Sync + 'static,
{
	fn pre_register(
		id: Option<CurrencyId>,
		metadata: AssetMetadata<Balance, CustomMetadata<T>>,
	) -> Result<(CurrencyId, AssetMetadata<Balance, CustomMetadata<T>>), DispatchError> {
		match id {
			Some(id) => Ok((id, metadata)),
			None => Err(DispatchError::Other("asset-registry: AssetId is required")),
		}
	}

	fn post_register(
		_id: CurrencyId,
		_asset_metadata: AssetMetadata<Balance, CustomMetadata<T>>,
	) -> Result<(), DispatchError> {
		Ok(())
	}
}

pub type AssetAuthority = AsEnsureOriginWithArg<EnsureRoot<AccountId>>;
