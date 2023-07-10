use core::convert::TryInto;

use super::*;

use orml_traits::MultiCurrency;
use sp_runtime::{DispatchError, DispatchResult};
use sp_std::marker::PhantomData;

use spacewalk_primitives::{Asset, CurrencyId};

use zenlink_protocol::{
	AssetId, Config as ZenlinkConfig, GenerateLpAssetId, LocalAssetHandler, ZenlinkMultiAssets,
	LOCAL, NATIVE,
};
pub type ZenlinkAssetId = zenlink_protocol::AssetId;

use runtime_common::{zenlink, zenlink::*};

parameter_types! {
	pub SelfParaId: u32 = ParachainInfo::parachain_id().into();
	pub const ZenlinkPalletId: PalletId = PalletId(*b"/zenlink");
	pub ZenlinkRegisteredParaChains: Vec<(MultiLocation, u128)> = vec![];
}

impl ZenlinkConfig for Runtime {
	type RuntimeEvent = super::RuntimeEvent;
	type MultiAssetsHandler = MultiAssets;
	type PalletId = ZenlinkPalletId;
	type AssetId = AssetId;
	type LpGenerate = SpacewalkPairLPGenerate<Self>;
	type TargetChains = ZenlinkRegisteredParaChains;
	type SelfParaId = SelfParaId;
	type WeightInfo = ();
}

type MultiAssets = ZenlinkMultiAssets<ZenlinkProtocol, Balances, LocalAssetAdaptor<Tokens>>;

pub struct LocalAssetAdaptor<Local>(PhantomData<Local>);

pub struct SpacewalkPairLPGenerate<T>(PhantomData<T>);
impl<T: ZenlinkConfig> GenerateLpAssetId<ZenlinkAssetId> for SpacewalkPairLPGenerate<T> {
	fn generate_lp_asset_id(
		asset_0: ZenlinkAssetId,
		asset_1: ZenlinkAssetId,
	) -> Option<ZenlinkAssetId> {
		zenlink::generate_lp_asset_id(asset_0, asset_1, ParachainInfo::parachain_id().into())
	}
}

impl<Local, AccountId> LocalAssetHandler<AccountId> for LocalAssetAdaptor<Local>
where
	Local: MultiCurrency<AccountId, CurrencyId = CurrencyId>,
{
	fn local_balance_of(asset_id: ZenlinkAssetId, who: &AccountId) -> AssetBalance {
		if let Ok(currency_id) =
			zenlink_id_to_currency_id(asset_id, ParachainInfo::parachain_id().into())
		{
			return TryInto::<AssetBalance>::try_into(Local::free_balance(currency_id, &who))
				.unwrap_or_default()
		}
		AssetBalance::default()
	}

	fn local_total_supply(asset_id: ZenlinkAssetId) -> AssetBalance {
		if let Ok(currency_id) =
			zenlink_id_to_currency_id(asset_id, ParachainInfo::parachain_id().into())
		{
			return TryInto::<AssetBalance>::try_into(Local::total_issuance(currency_id))
				.unwrap_or_default()
		}
		AssetBalance::default()
	}

	fn local_is_exists(asset_id: ZenlinkAssetId) -> bool {
		match zenlink_id_to_currency_id(asset_id, ParachainInfo::parachain_id().into()) {
			Ok(_) => true,
			Err(_) => false,
		}
	}

	fn local_transfer(
		asset_id: ZenlinkAssetId,
		origin: &AccountId,
		target: &AccountId,
		amount: AssetBalance,
	) -> DispatchResult {
		if let Ok(currency_id) =
			zenlink_id_to_currency_id(asset_id, ParachainInfo::parachain_id().into())
		{
			Local::transfer(
				currency_id,
				&origin,
				&target,
				amount
					.try_into()
					.map_err(|_| DispatchError::Other("convert amount in local transfer"))?,
			)
		} else {
			Err(DispatchError::Other("unknown asset in local transfer"))
		}
	}

	fn local_deposit(
		asset_id: ZenlinkAssetId,
		origin: &AccountId,
		amount: AssetBalance,
	) -> Result<AssetBalance, DispatchError> {
		if let Ok(currency_id) =
			zenlink_id_to_currency_id(asset_id, ParachainInfo::parachain_id().into())
		{
			Local::deposit(
				currency_id,
				&origin,
				amount
					.try_into()
					.map_err(|_| DispatchError::Other("convert amount in local deposit"))?,
			)?;
		} else {
			return Err(DispatchError::Other("unknown asset in local transfer"))
		}

		Ok(amount)
	}

	fn local_withdraw(
		asset_id: ZenlinkAssetId,
		origin: &AccountId,
		amount: AssetBalance,
	) -> Result<AssetBalance, DispatchError> {
		if let Ok(currency_id) =
			zenlink_id_to_currency_id(asset_id, ParachainInfo::parachain_id().into())
		{
			Local::withdraw(
				currency_id,
				&origin,
				amount
					.try_into()
					.map_err(|_| DispatchError::Other("convert amount in local withdraw"))?,
			)?;
		} else {
			return Err(DispatchError::Other("unknown asset in local transfer"))
		}

		Ok(amount)
	}
}
