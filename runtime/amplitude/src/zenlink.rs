use core::convert::TryInto;

use super::*;

use orml_traits::MultiCurrency;
use sp_runtime::{DispatchError, DispatchResult};
use sp_std::marker::PhantomData;

use zenlink_protocol::{
	AssetId, AssetIdConverter, LocalAssetHandler, PairLpGenerate, ZenlinkMultiAssets, LOCAL, NATIVE,
};
pub type ZenlinkAssetId = AssetId;

parameter_types! {
	pub SelfParaId: u32 = ParachainInfo::parachain_id().into();
	pub const ZenlinkPalletId: PalletId = PalletId(*b"/zenlink");
	pub ZenlinkRegisteredParaChains: Vec<(MultiLocation, u128)> = vec![];
}

impl zenlink_protocol::Config for Runtime {
	type RuntimeEvent = super::RuntimeEvent;
	type MultiAssetsHandler = MultiAssets;
	type PalletId = ZenlinkPalletId;
	type AssetId = AssetId;
	type LpGenerate = PairLpGenerate<Self>;
	type TargetChains = ZenlinkRegisteredParaChains;
	type SelfParaId = SelfParaId;
	type AccountIdConverter = ();
	type AssetIdConverter = AssetIdConverter;
	type XcmExecutor = ();
	type WeightInfo = ();
}

type MultiAssets = ZenlinkMultiAssets<ZenlinkProtocol, Balances, LocalAssetAdaptor<Tokens>>;

pub struct LocalAssetAdaptor<Local>(PhantomData<Local>);

impl<Local, AccountId> LocalAssetHandler<AccountId> for LocalAssetAdaptor<Local>
where
	Local: MultiCurrency<AccountId, CurrencyId = CurrencyId>,
{
	fn local_balance_of(asset_id: ZenlinkAssetId, who: &AccountId) -> AssetBalance {
		if let Ok(currency_id) = zenlink_id_to_currency_id(asset_id) {
			return TryInto::<AssetBalance>::try_into(Local::free_balance(currency_id, &who))
				.unwrap_or_default()
		}
		AssetBalance::default()
	}

	fn local_total_supply(asset_id: ZenlinkAssetId) -> AssetBalance {
		if let Ok(currency_id) = zenlink_id_to_currency_id(asset_id) {
			return TryInto::<AssetBalance>::try_into(Local::total_issuance(currency_id))
				.unwrap_or_default()
		}
		AssetBalance::default()
	}

	fn local_is_exists(asset_id: ZenlinkAssetId) -> bool {
		match zenlink_id_to_currency_id(asset_id) {
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
		if let Ok(currency_id) = zenlink_id_to_currency_id(asset_id) {
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
		if let Ok(currency_id) = zenlink_id_to_currency_id(asset_id) {
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
		if let Ok(currency_id) = zenlink_id_to_currency_id(asset_id) {
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

fn zenlink_id_to_currency_id(asset_id: ZenlinkAssetId) -> Result<CurrencyId, ()> {
	let para_chain_id: u32 = ParachainInfo::parachain_id().into();
	if asset_id.chain_id != para_chain_id {
		log::error!("Asset Chain Id {} not compatibile with the Parachain Id.", asset_id.chain_id);
		return Err(())
	}

	match asset_id.asset_type {
		NATIVE => Ok(CurrencyId::Native),
		LOCAL => {
			let foreign_id: ForeignCurrencyId = asset_id.asset_index.try_into().map_err(|_| {
				log::error!("{} has no Foreign Currency Id match.", asset_id.asset_index);
				()
			})?;

			Ok(XCM(foreign_id))
		},
		other => {
			log::error!("Unsupported asset type index:{other}");
			Err(())
		},
	}
}
