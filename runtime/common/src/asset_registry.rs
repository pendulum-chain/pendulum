use crate::*;
use frame_support::traits::AsEnsureOriginWithArg;
use frame_system::EnsureRoot;
use orml_traits::{FixedConversionRateProvider as FixedConversionRateProviderTrait,
	asset_registry::{AssetMetadata, AssetProcessor, Inspect}};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::Get;
use sp_runtime::{BoundedVec, DispatchError, traits::PhantomData};
use sp_std::fmt::Debug;
use spacewalk_primitives::CurrencyId;
use xcm::opaque::v3::{Junction,MultiLocation};

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


pub struct FixedConversionRateProvider<OrmlAssetRegistry>(PhantomData<OrmlAssetRegistry>);

impl<
		OrmlAssetRegistry: Inspect<
			AssetId = CurrencyId,
			Balance = Balance,
			CustomMetadata = asset_registry::CustomMetadata,
		>,
	> FixedConversionRateProviderTrait for FixedConversionRateProvider<OrmlAssetRegistry>
{
	fn get_fee_per_second(location: &MultiLocation) -> Option<u128> {
		log::warn!("getting for location: {:?}", location);
		
		// fix
		let unanchored_location = match location {
            MultiLocation { parents: 0, interior } => {
     
                match interior.pushed_front_with(Junction::Parachain(2094u32)) {
                    Ok(new_interior) => MultiLocation {
                        parents: 1,
                        interior: new_interior,
                    },
                    Err(_) => return None, 
                }
            },
  
            x => *x,
        };
		log::warn!("getting for location adjusted: {:?}", unanchored_location);
		let metadata = OrmlAssetRegistry::metadata_by_location(&unanchored_location)?;
		Some(metadata.additional.fee_per_second)
	}
}