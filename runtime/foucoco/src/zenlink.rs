use core::convert::TryInto;

use super::*;

use orml_traits::MultiCurrency;
use sp_runtime::{DispatchError, DispatchResult};
use sp_std::marker::PhantomData;

use spacewalk_primitives::{CurrencyId, Asset};

use zenlink_protocol::{
	AssetId, Config as ZenlinkConfig, LocalAssetHandler, PairLpGenerate, ZenlinkMultiAssets, LOCAL,
	NATIVE,
};
pub type ZenlinkAssetId = zenlink_protocol::AssetId;

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
	type LpGenerate = PairLpGenerate<Self>;
	type TargetChains = ZenlinkRegisteredParaChains;
	type SelfParaId = SelfParaId;
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

pub const USDC_ISSUER: [u8; 32] = [
	59, 153, 17, 56, 14, 254, 152, 139, 160, 168, 144, 14, 177, 207, 228, 79, 54, 111, 125, 190,
	148, 107, 237, 7, 114, 64, 247, 246, 36, 223, 21, 197,
];

pub const BRL_ISSUER: [u8; 32] = [
	234, 172, 104, 212, 208, 227, 123, 76, 36, 194, 83, 105, 22, 232, 48, 115, 95, 3, 45, 13, 107,
	42, 28, 143, 202, 59, 197, 162, 94, 8, 62, 58,
];

pub const TZS_ISSUER: [u8; 32] = [
	52, 201, 75, 42, 75, 169, 232, 181, 123, 34, 84, 125, 203, 179, 15, 68, 60, 76, 176, 45, 163,
	130, 154, 137, 170, 27, 212, 120, 14, 68, 102, 186,
];

fn discriminant(currency: &CurrencyId) -> u8 {
	match currency {
		CurrencyId::Native => 0,
		CurrencyId::XCM(_) => 1,
		CurrencyId::Stellar(_) => 2,
		CurrencyId::ZenlinkLPToken(_, _, _, _) => 6,
	}
}

struct WrappedCurrencyId(CurrencyId);

impl TryFrom<WrappedCurrencyId> for ZenlinkAssetId {
	type Error = ();

	fn try_from(wrapped_currency: WrappedCurrencyId) -> Result<Self, Self::Error> {
		let disc = discriminant(&wrapped_currency.0) as u64;
		let currency = wrapped_currency.0;
		let parachain_id: u32 = ParachainInfo::parachain_id().into();
		match currency {
			CurrencyId::Native => Ok(ZenlinkAssetId {
				chain_id: parachain_id,
				asset_type: NATIVE,
				asset_index: 0 as u64,
			}),
			CurrencyId::XCM(token_id) => Ok(ZenlinkAssetId {
				chain_id: parachain_id,
				asset_type: LOCAL,
				asset_index: ((disc << 8) + token_id as u64) as u64,
			}),
			CurrencyId::Stellar(asset) => {
				let _id = match asset {
					Asset::StellarNative => 0u64,
					Asset::AlphaNum4 { code, .. } => match &code {
						b"USDC" => 1u64,
						b"TZS\0" => 2u64,
						b"BRL\0" => 3u64,
						_ => return Err(())
					},
					_ => return Err(()),
				};
				Ok(ZenlinkAssetId {
					chain_id: parachain_id,
					asset_type: LOCAL,
					asset_index: ((disc << 8) + _id) as u64,
				})
			},
			CurrencyId::ZenlinkLPToken(token1_id, token1_type, token2_id, token2_type) => {
				let _index = ((disc as u64) << 8) +
					((token1_id as u64) << 16) +
					((token1_type as u64) << 24) +
					((token2_id as u64) << 32) +
					((token2_type as u64) << 40);
				Ok(ZenlinkAssetId {
					chain_id: parachain_id,
					asset_type: LOCAL,
					asset_index: _index,
				})
			},
		}
	}
}

impl TryFrom<ZenlinkAssetId> for WrappedCurrencyId {
	type Error = ();

	fn try_from(asset: ZenlinkAssetId) -> Result<Self, Self::Error> {
		let _index = asset.asset_index;
		let disc = ((_index & 0x0000_0000_0000_ff00) >> 8) as u8;
		let symbol = (_index & 0x0000_0000_0000_00ff) as u8;
		match disc {
			0 => Ok(WrappedCurrencyId(CurrencyId::Native)),
			1 => Ok(WrappedCurrencyId(CurrencyId::XCM(symbol))),
			2 => {
				match symbol {
					0 => Ok(WrappedCurrencyId(CurrencyId::Stellar(Asset::StellarNative))),
					1 => Ok(WrappedCurrencyId(CurrencyId::Stellar(Asset::AlphaNum4 {
						code: *b"USDC",
						issuer: USDC_ISSUER,
					}))),
					2 => Ok(WrappedCurrencyId(CurrencyId::Stellar(Asset::AlphaNum4 {
						code: *b"TZS\0",
						issuer: TZS_ISSUER,
					}))),
					3 => Ok(WrappedCurrencyId(CurrencyId::Stellar(Asset::AlphaNum4 {
						code: *b"BRL\0",
						issuer: BRL_ISSUER,
					}))),
					_ => return Err(()),
				}
			},
			6 => {
				let token1_id = ((_index & 0x0000_0000_00FF_0000) >> 16) as u8;
				let token1_type = ((_index & 0x0000_0000_FF00_0000) >> 24) as u8;

				let token2_id = ((_index & 0x0000_00FF_0000_0000) >> 32) as u8;
				let token2_type = ((_index & 0x0000_FF00_0000_0000) >> 40) as u8;

				Ok(WrappedCurrencyId(CurrencyId::ZenlinkLPToken(token1_id, token1_type, token2_id, token2_type)))
			},
			_ => Err(()),
		}
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
			let foreign_id: u8 = asset_id.asset_index.try_into().map_err(|_| {
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
