use core::convert::TryInto;

use super::*;

use orml_traits::MultiCurrency;
use sp_runtime::{DispatchError, DispatchResult};
use sp_std::marker::PhantomData;

use zenlink_protocol::{
	AssetId, AssetIdConverter, LocalAssetHandler, PairLpGenerate, ZenlinkMultiAssets, LOCAL, NATIVE,
};
pub type ZenlinkAssetId = zenlink_protocol::AssetId;

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

// impl TryFrom<CurrencyId> for ZenlinkAssetId {
// 	type Error = ();
//
// 	fn try_from(currency_id: CurrencyId) -> Result<Self, Self::Error> {
// 		let para_chain_id: u32 = ParachainInfo::parachain_id().into();
// 		match currency_id {
// 			CurrencyId::Native =>
// 				Ok(ZenlinkAssetId { chain_id: para_chain_id, asset_type: NATIVE, asset_index: 0 }),
// 			CurrencyId::XCM(xcm) => Ok(ZenlinkAssetId {
// 				chain_id: para_chain_id,
// 				asset_type: LOCAL,
// 				asset_index: xcm as u64,
// 			}),
// 		}
// 	}
// }

// impl TryFrom<ZenlinkAssetId> for CurrencyId {
// 	type Error = ();
// 	fn try_from(asset_id: ZenlinkAssetId) -> Result<Self, Self::Error> {
// 		let para_chain_id: u32 = ParachainInfo::parachain_id().into();
// 		if asset_id.chain_id != para_chain_id {
// 			return Err(())
// 		}
//
// 		match asset_id.asset_type {
// 			NATIVE => Ok(CurrencyId::Native),
// 			LOCAL => {
// 				let foreign_currency_id_option = asset_id.asset_index.try_into();
// 				match foreign_currency_id_option {
// 					Ok(foreign_currency_id) => Ok(CurrencyId::XCM(foreign_currency_id)),
// 					Err(e) => Err(e),
// 				}
// 			},
// 			_ => Err(()),
// 		}
// 	}
// }

// impl TryFrom<u64> for ForeignCurrencyId {
// 	type Error = ();
// 	fn try_from(num: u64) -> Result<Self, Self::Error> {
// 		match num {
// 			0 => Ok(ForeignCurrencyId::KSM),
// 			1 => Ok(ForeignCurrencyId::KAR),
// 			2 => Ok(ForeignCurrencyId::AUSD),
// 			3 => Ok(ForeignCurrencyId::BNC),
// 			4 => Ok(ForeignCurrencyId::VsKSM),
// 			5 => Ok(ForeignCurrencyId::HKO),
// 			6 => Ok(ForeignCurrencyId::MOVR),
// 			7 => Ok(ForeignCurrencyId::SDN),
// 			8 => Ok(ForeignCurrencyId::KINT),
// 			9 => Ok(ForeignCurrencyId::KBTC),
// 			10 => Ok(ForeignCurrencyId::GENS),
// 			11 => Ok(ForeignCurrencyId::XOR),
// 			12 => Ok(ForeignCurrencyId::TEER),
// 			13 => Ok(ForeignCurrencyId::KILT),
// 			14 => Ok(ForeignCurrencyId::PHA),
// 			15 => Ok(ForeignCurrencyId::ZTG),
// 			16 => Ok(ForeignCurrencyId::USD),
// 			_ => Err(()),
// 		}
// 	}
// }
