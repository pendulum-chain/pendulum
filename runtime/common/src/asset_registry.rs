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
pub struct StringLimit; 
impl Get<u32> for StringLimit {
	fn get() -> u32 {
		50
	}
}

#[derive(Clone, PartialOrd, Ord, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct CustomMetadata {
	pub dia_keys: DiaKeys<StringLimit>,
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

impl AssetProcessor<CurrencyId, AssetMetadata<Balance, CustomMetadata>> for CustomAssetProcessor {
	fn pre_register(
		id: Option<CurrencyId>,
		metadata: AssetMetadata<Balance, CustomMetadata>,
	) -> Result<(CurrencyId, AssetMetadata<Balance, CustomMetadata>), DispatchError> {
		match id {
			Some(id) => Ok((id, metadata)),
			None => Err(DispatchError::Other("asset-registry: AssetId is required")),
		}
	}

	fn post_register(
		_id: CurrencyId,
		_asset_metadata: AssetMetadata<Balance, CustomMetadata>,
	) -> Result<(), DispatchError> {
		Ok(())
	}
}

pub type AssetAuthority = AsEnsureOriginWithArg<EnsureRoot<AccountId>>;
