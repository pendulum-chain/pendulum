use crate::*;
use frame_support::{
	dispatch::RawOrigin,
	sp_std::marker::PhantomData,
	traits::{EnsureOrigin, EnsureOriginWithArg},
};
use frame_system::EnsureRoot;
use orml_traits::asset_registry::{AssetMetadata, AssetProcessor};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;
use spacewalk_primitives::CurrencyId;

pub use spacewalk_primitives::CustomMetadata;

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

pub struct AssetAuthority<Origin>(PhantomData<Origin>);
impl<Origin: Into<Result<RawOrigin<AccountId>, Origin>> + From<RawOrigin<AccountId>>>
	EnsureOriginWithArg<Origin, Option<CurrencyId>> for AssetAuthority<Origin>
{
	type Success = ();

	fn try_origin(origin: Origin, _asset_id: &Option<CurrencyId>) -> Result<Self::Success, Origin> {
		EnsureRoot::try_origin(origin)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin(_: &Option<CurrencyId>) -> Result<Origin, ()> {
		EnsureRoot::try_successful_origin()
	}
}
