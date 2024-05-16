use crate::*;
use frame_support::traits::AsEnsureOriginWithArg;
use frame_system::EnsureRoot;
use orml_traits::{
	asset_registry::{AssetMetadata, AssetProcessor, Inspect},
	FixedConversionRateProvider as FixedConversionRateProviderTrait,
};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::Get;
use sp_runtime::{traits::PhantomData, BoundedVec, DispatchError};
use sp_std::fmt::Debug;
use sp_std::vec::Vec;
use spacewalk_primitives::oracle::Key;
use spacewalk_primitives::CurrencyId;
use xcm::opaque::v3::MultiLocation;

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
		let metadata = OrmlAssetRegistry::metadata_by_location(&location)?;
		Some(metadata.additional.fee_per_second)
	}
}

// Define a convertor to convert between a CurrencyId and the dia oracle keys using the metadata
// stored in the asset registry
pub struct AssetRegistryToDiaOracleKeyConvertor<Runtime>(PhantomData<Runtime>);

impl<
		Runtime: orml_asset_registry::Config<AssetId = CurrencyId, CustomMetadata = CustomMetadata>,
	> Convert<Key, Option<(Vec<u8>, Vec<u8>)>> for AssetRegistryToDiaOracleKeyConvertor<Runtime>
{
	fn convert(spacewalk_oracle_key: Key) -> Option<(Vec<u8>, Vec<u8>)> {
		let currency_id = match spacewalk_oracle_key {
			Key::ExchangeRate(currency_id) => currency_id,
		};

		// Try to find the dia keys in the asset registry metadata
		orml_asset_registry::Pallet::<Runtime>::metadata(currency_id).and_then(|metadata| {
			let dia_keys = metadata.additional.dia_keys;
			if dia_keys.blockchain.is_empty() || dia_keys.symbol.is_empty() {
				return None;
			}
			return Some((dia_keys.blockchain, dia_keys.symbol));
		});

		// We didn't find the dia keys in the asset registry metadata
		None
	}
}

impl<
		Runtime: orml_asset_registry::Config<AssetId = CurrencyId, CustomMetadata = CustomMetadata>,
	> Convert<(Vec<u8>, Vec<u8>), Option<Key>> for AssetRegistryToDiaOracleKeyConvertor<Runtime>
{
	fn convert(dia_oracle_key: (Vec<u8>, Vec<u8>)) -> Option<Key> {
		// Try to find the currency id in the asset registry metadata for which the dia keys are
		// matching the ones provided
		let blockchain = dia_oracle_key.0;
		let symbol = dia_oracle_key.1;
		orml_asset_registry::Metadata::<Runtime>::iter().find_map(|(currency_id, metadata)| {
			let dia_keys = metadata.additional.dia_keys;
			if dia_keys.blockchain == blockchain && dia_keys.symbol == symbol {
				return Some(Key::ExchangeRate(currency_id));
			}
			None
		})
	}
}
