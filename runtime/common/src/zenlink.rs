use crate::stellar::{BRL_ISSUER, TZS_ISSUER, USDC_ISSUER};
use spacewalk_primitives::{Asset, CurrencyId};

use zenlink_protocol::{LOCAL, NATIVE};
pub type ZenlinkAssetId = zenlink_protocol::AssetId;

fn discriminant(currency: &CurrencyId) -> u8 {
	match currency {
		CurrencyId::Native => 0,
		CurrencyId::XCM(_) => 1,
		CurrencyId::Stellar(_) => 2,
		CurrencyId::ZenlinkLPToken(_, _, _, _) => 3,
		CurrencyId::Token(_) => 4,
	}
}

pub fn generate_lp_asset_id(
	asset_0: ZenlinkAssetId,
	asset_1: ZenlinkAssetId,
	parachain_id: u32,
) -> Option<ZenlinkAssetId> {
	let currency_0 = (asset_0.asset_index & 0x0000_0000_0000_ffff) << 16;
	let currency_1 = (asset_1.asset_index & 0x0000_0000_0000_ffff) << 32;
	let discr = 3u64 << 8;
	let index = currency_0 + currency_1 + discr;
	Some(ZenlinkAssetId { chain_id: parachain_id, asset_type: LOCAL, asset_index: index })
}

pub fn zenlink_id_to_currency_id(
	asset_id: ZenlinkAssetId,
	parachain_id: u32,
) -> Result<CurrencyId, ()> {
	if asset_id.chain_id != parachain_id {
		return Err(())
	}

	let index = asset_id.asset_index;
	let asset_type = asset_id.asset_type;
	let disc = ((index & 0x0000_0000_0000_ff00) >> 8) as u8;
	let symbol = (index & 0x0000_0000_0000_00ff) as u8;
	match (disc, asset_type) {
		(0, NATIVE) => Ok(CurrencyId::Native),
		(1, LOCAL) => Ok(CurrencyId::XCM(symbol)),
		(2, LOCAL) => match symbol {
			0 => Ok(CurrencyId::Stellar(Asset::StellarNative)),
			1 => Ok(CurrencyId::Stellar(Asset::AlphaNum4 { code: *b"USDC", issuer: USDC_ISSUER })),
			2 => Ok(CurrencyId::Stellar(Asset::AlphaNum4 { code: *b"TZS\0", issuer: TZS_ISSUER })),
			3 => Ok(CurrencyId::Stellar(Asset::AlphaNum4 { code: *b"BRL\0", issuer: BRL_ISSUER })),
			_ => return Err(()),
		},
		(3, LOCAL) => {
			let token1_id = ((index & 0x0000_0000_00FF_0000) >> 16) as u8;
			let token1_type = ((index & 0x0000_0000_FF00_0000) >> 24) as u8;

			let token2_id = ((index & 0x0000_00FF_0000_0000) >> 32) as u8;
			let token2_type = ((index & 0x0000_FF00_0000_0000) >> 40) as u8;

			Ok(CurrencyId::ZenlinkLPToken(token1_id, token1_type, token2_id, token2_type))
		},
		(4, LOCAL) =>{
			Ok(CurrencyId::Token(symbol.into()))
		}
		_ => Err(()),
	}
}

pub fn currency_id_to_zenlink_id(
	currency_id: CurrencyId,
	parachain_id: u32,
) -> Result<ZenlinkAssetId, ()> {
	let disc = discriminant(&currency_id) as u64;
	match currency_id {
		CurrencyId::Native =>
			Ok(ZenlinkAssetId { chain_id: parachain_id, asset_type: NATIVE, asset_index: 0 as u64 }),
		CurrencyId::XCM(token_id) => Ok(ZenlinkAssetId {
			chain_id: parachain_id,
			asset_type: LOCAL,
			asset_index: ((disc << 8) + token_id as u64) as u64,
		}),
		CurrencyId::Stellar(asset) => {
			let _id = match asset {
				Asset::StellarNative => 0u64,
				Asset::AlphaNum4 { code, issuer } => match (&code, &issuer) {
					(b"USDC", &USDC_ISSUER) => 1u64,
					(b"TZS\0", &TZS_ISSUER) => 2u64,
					(b"BRL\0", &BRL_ISSUER) => 3u64,
					_ => return Err(()),
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
			let index = ((disc as u64) << 8) +
				((token1_id as u64) << 16) +
				((token1_type as u64) << 24) +
				((token2_id as u64) << 32) +
				((token2_type as u64) << 40);
			Ok(ZenlinkAssetId { chain_id: parachain_id, asset_type: LOCAL, asset_index: index })
		},
		CurrencyId::Token(token_id) => Ok(ZenlinkAssetId {
			chain_id: parachain_id,
			asset_type: LOCAL,
			asset_index: ((disc << 8) + token_id as u64) as u64,
		})
	}
}

#[cfg(test)]
mod zenlink_tests {
	use super::*;

	#[test]
	fn convert_zenlink_native_to_native_currency() {
		// Native ZenlinkAsset index = 0x0000_0000_0000_0000
		let fake_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: NATIVE, asset_index: 0 as u64 };
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		assert_eq!(currency, Ok(CurrencyId::Native));
	}

	#[test]
	fn convert_zenlink_xcm_to_xcm_currency() {
		// XCM(0) ZenlinkAsset index = 0x0000_0000_0000_0100
		let index = 0x0100 as u64;
		let fake_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: index };
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		assert_eq!(currency, Ok(CurrencyId::XCM(0)));
	}

	fn get_stellar_asset(selector: u8) -> spacewalk_primitives::CurrencyId {
		match selector {
			0 => CurrencyId::Stellar(Asset::StellarNative),
			1 => CurrencyId::Stellar(Asset::AlphaNum4 { code: *b"USDC", issuer: USDC_ISSUER }),
			2 => CurrencyId::Stellar(Asset::AlphaNum4 { code: *b"TZS\0", issuer: TZS_ISSUER }),
			3 => CurrencyId::Stellar(Asset::AlphaNum4 { code: *b"BRL\0", issuer: BRL_ISSUER }),
			_ => CurrencyId::Stellar(Asset::StellarNative),
		}
	}

	#[test]
	fn convert_zenlink_stellar_to_stellar_currency() {
		// Stellar Native ZenlinkAsset index = 0x0000_0000_0000_0200
		let stellar_native_index = 0x0200 as u64;
		let fake_zenlink_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: stellar_native_index,
		};
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		assert_eq!(currency, Ok(get_stellar_asset(0u8)));

		// Stellar USDC ZenlinkAsset index = 0x0000_0000_0000_0201
		let usdc_index = 0x0201 as u64;
		let fake_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: usdc_index };
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		assert_eq!(currency, Ok(get_stellar_asset(1u8)));

		// Stellar TZS ZenlinkAsset index = 0x0000_0000_0000_0202
		let tzs_index = 0x0202 as u64;
		let fake_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: tzs_index };
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		assert_eq!(currency, Ok(get_stellar_asset(2u8)));

		// Stellar BRL ZenlinkAsset index = 0x0000_0000_0000_0203
		let brl_index = 0x0203 as u64;
		let fake_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: brl_index };
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		assert_eq!(currency, Ok(get_stellar_asset(3u8)));
	}

	#[test]
	fn convert_zenlink_lp_token_to_lp_token_currency() {
		// Native and XCM(0) LP token Zenlink index = 0x0000_0100_0000_0300
		let native_xcm_lp_token_index = 0x0000_0100_0000_0300 as u64;
		let fake_zenlink_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: native_xcm_lp_token_index,
		};
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		let expected_currency: CurrencyId = CurrencyId::ZenlinkLPToken(0, 0, 0, 1);
		assert_eq!(currency, Ok(expected_currency));

		// XCM(0) and XCM(1) LP token Zenlink index = 0x0000_0101_0100_0300
		let xcm0_xcm1_lp_token_index = 0x0000_0101_0100_0300 as u64;
		let fake_zenlink_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: xcm0_xcm1_lp_token_index,
		};
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		let expected_currency: CurrencyId = CurrencyId::ZenlinkLPToken(0, 1, 1, 1);
		assert_eq!(currency, Ok(expected_currency));

		// XCM(0) and Stellar Native LP Token Zenlink index = 0x0000_0200_0100_0300
		let xcm0_stellar_native_lp_token_index = 0x0000_0200_0100_0300 as u64;
		let fake_zenlink_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: xcm0_stellar_native_lp_token_index,
		};
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		let expected_currency: CurrencyId = CurrencyId::ZenlinkLPToken(0, 1, 0, 2);
		assert_eq!(currency, Ok(expected_currency));

		// XCM(0) and Stellar USDC LP Token Zenlink index = 0x0000_0201_0100_0300
		let xcm0_stellar_usdc_lp_token_index = 0x0000_0201_0100_0300 as u64;
		let fake_zenlink_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: xcm0_stellar_usdc_lp_token_index,
		};
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		let expected_currency: CurrencyId = CurrencyId::ZenlinkLPToken(0, 1, 1, 2);
		assert_eq!(currency, Ok(expected_currency));

		// Stellar Native and Stellar USDC LP Token Zenlink index = 0x0000_0201_0200_0300
		let stellar_native_stellar_usdc_lp_token_index = 0x0000_0201_0200_0300 as u64;
		let fake_zenlink_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: stellar_native_stellar_usdc_lp_token_index,
		};
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		let expected_currency: CurrencyId = CurrencyId::ZenlinkLPToken(0, 2, 1, 2);
		assert_eq!(currency, Ok(expected_currency));

		// XCM(0) and Token(0) LP Token Zenlink index = 0x0000_0400_0100_0300
		let xcm0_token0_lp_token_index = 0x0000_0400_0100_0300 as u64;
		let fake_zenlink_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: xcm0_token0_lp_token_index,
		};
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		let expected_currency: CurrencyId = CurrencyId::ZenlinkLPToken(0, 1, 0, 4);
		assert_eq!(currency, Ok(expected_currency));

		// XCM(5) and Token(58) LP Token Zenlink index = 0x0000_043A_0105_0300
		let xcm0_token58_lp_token_index = 0x0000_043A_0105_0300 as u64;
		let fake_zenlink_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: xcm0_token58_lp_token_index,
		};
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		let expected_currency: CurrencyId = CurrencyId::ZenlinkLPToken(5, 1, 58, 4);
		assert_eq!(currency, Ok(expected_currency));
	}

	#[test]
	fn convert_fake_zenlink_native_to_currency_id_error() {
		// Native ZenlinkAsset index = 0x0000_0000_0000_0000
		let fake_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: 0 as u64 };
		let currency: Result<CurrencyId, _> =
			zenlink_id_to_currency_id(fake_zenlink_asset, 1000u32);
		assert_eq!(currency, Err(()));
	}

	#[test]
	fn convert_zenlink_id_to_currency_id_error() {
		let fake_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: 0u64 };
		// We pass a parachain_id different than the asset chain_id
		assert_eq!(zenlink_id_to_currency_id(fake_zenlink_asset, 1001u32), Err(()));
	}

	#[test]
	fn convert_native_currency_to_zenlink_native() {
		let fake_currency_id = CurrencyId::Native;
		let expected_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: NATIVE, asset_index: 0u64 };
		assert_eq!(currency_id_to_zenlink_id(fake_currency_id, 1000), Ok(expected_zenlink_asset));
	}

	#[test]
	fn convert_xcm_currency_to_zenlink_xcm() {
		let fake_currency_id = CurrencyId::XCM(0);
		let expected_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: 0x0100 as u64 };
		assert_eq!(currency_id_to_zenlink_id(fake_currency_id, 1000), Ok(expected_zenlink_asset));
	}
	#[test]
	fn convert_xcm_1_currency_to_zenlink_xcm() {
		let fake_currency_id = CurrencyId::XCM(1);
		let expected_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: 0x0101 as u64 };
		assert_eq!(currency_id_to_zenlink_id(fake_currency_id, 1000), Ok(expected_zenlink_asset));
	}

	#[test]
	fn convert_stellar_currency_to_stellar_zenlink() {
		let stellar_assets_indexes: [u64; 4] = [0x0200u64, 0x0201u64, 0x0202u64, 0x0203u64];

		for i in 0..=3usize {
			let fake_currency_id = get_stellar_asset(i as u8);
			let expected_zenlink_asset = ZenlinkAssetId {
				chain_id: 1000u32,
				asset_type: LOCAL,
				asset_index: stellar_assets_indexes[i],
			};
			assert_eq!(
				currency_id_to_zenlink_id(fake_currency_id, 1000),
				Ok(expected_zenlink_asset)
			);
		}
	}

	#[test]
	fn convert_token_currency_to_zenlink_token() {
		let fake_currency_id = CurrencyId::Token(0);
		let expected_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: 0x0400 as u64 };
		assert_eq!(currency_id_to_zenlink_id(fake_currency_id, 1000), Ok(expected_zenlink_asset));
	}


	#[test]
	fn convert_token_currency_to_zenlink_token_2() {
		let fake_currency_id = CurrencyId::Token(6);
		let expected_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: 0x0406 as u64 };
		assert_eq!(currency_id_to_zenlink_id(fake_currency_id, 1000), Ok(expected_zenlink_asset));
	}

	#[test]
	fn convert_ZenlinkLPToken_currency_to_zenlink_lp_representation() {
		let fake_currency_id = CurrencyId::ZenlinkLPToken(158,4,101,1);
		let expected_zenlink_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: 0x0000_0165_049E_0300 as u64 };
		assert_eq!(currency_id_to_zenlink_id(fake_currency_id, 1000), Ok(expected_zenlink_asset));
	}
}
