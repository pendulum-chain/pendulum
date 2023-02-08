use core::convert::TryInto;

use super::*;

use orml_traits::MultiCurrency;
use sp_runtime::{DispatchError, DispatchResult};
use sp_std::marker::PhantomData;

use zenlink_protocol::*;
pub type ZenlinkAssetId = zenlink_protocol::AssetId;

parameter_types! {
	pub SelfParaId: u32 = ParachainInfo::parachain_id().into();
	pub const ZenlinkPalletId: PalletId = PalletId(*b"/zenlink");
	pub ZenlinkRegisteredParaChains: Vec<(MultiLocation, u128)> = vec![
		// (make_x2_location(2001), 10_000_000_000),
	];
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
		if let Ok(currency_id) = asset_id.try_into() {
			return TryInto::<AssetBalance>::try_into(Local::free_balance(currency_id, &who))
				.unwrap_or_default()
		}
		AssetBalance::default()
	}

	fn local_total_supply(asset_id: ZenlinkAssetId) -> AssetBalance {
		if let Ok(currency_id) = asset_id.try_into() {
			return TryInto::<AssetBalance>::try_into(Local::total_issuance(currency_id))
				.unwrap_or_default()
		}
		AssetBalance::default()
	}

	fn local_is_exists(asset_id: ZenlinkAssetId) -> bool {
		let currency_id: Result<CurrencyId, ()> = asset_id.try_into();
		match currency_id {
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
		if let Ok(currency_id) = asset_id.try_into() {
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
		if let Ok(currency_id) = asset_id.try_into() {
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
		if let Ok(currency_id) = asset_id.try_into() {
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

impl TryFrom<CurrencyId> for ZenlinkAssetId {
	type Error = ();

	fn try_from(currency_id: CurrencyId) -> Result<Self, Self::Error> {
		let para_chain_id: u32 = ParachainInfo::parachain_id().into();
		match currency_id {
			CurrencyId::Native =>
				Ok(ZenlinkAssetId { chain_id: para_chain_id, asset_type: NATIVE, asset_index: 0 }),
			CurrencyId::XCM(xcm) => Ok(ZenlinkAssetId {
				chain_id: para_chain_id,
				asset_type: LOCAL,
				asset_index: xcm as u64,
			}),
		}
	}
}

impl TryFrom<ZenlinkAssetId> for CurrencyId {
	type Error = ();
	fn try_from(asset_id: ZenlinkAssetId) -> Result<Self, Self::Error> {
		let para_chain_id: u32 = ParachainInfo::parachain_id().into();
		if asset_id.chain_id != para_chain_id {
			return Err(())
		}

		match asset_id.asset_type {
			NATIVE => Ok(CurrencyId::Native),
			LOCAL => Ok(CurrencyId::XCM(asset_id.asset_index.into())),
			_ => Err(()),
		}
	}
}

impl From<u64> for ForeignCurrencyId {
	fn from(num: u64) -> Self {
		match num {
			0 => ForeignCurrencyId::KSM,
			1 => ForeignCurrencyId::KAR,
			2 => ForeignCurrencyId::AUSD,
			3 => ForeignCurrencyId::BNC,
			4 => ForeignCurrencyId::VsKSM,
			5 => ForeignCurrencyId::HKO,
			6 => ForeignCurrencyId::MOVR,
			7 => ForeignCurrencyId::SDN,
			8 => ForeignCurrencyId::KINT,
			9 => ForeignCurrencyId::KBTC,
			10 => ForeignCurrencyId::GENS,
			11 => ForeignCurrencyId::XOR,
			12 => ForeignCurrencyId::TEER,
			13 => ForeignCurrencyId::KILT,
			14 => ForeignCurrencyId::PHA,
			15 => ForeignCurrencyId::ZTG,
			16 => ForeignCurrencyId::USD,
			_ => panic!("Unknown ForeignCurrencyId"),
		}
	}
}
