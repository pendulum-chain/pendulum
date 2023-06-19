use spacewalk_primitives::{Asset, CurrencyId};

use zenlink_protocol::{LOCAL, NATIVE};
pub type ZenlinkAssetId = zenlink_protocol::AssetId;

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

pub fn zenlink_id_to_currency_id(asset_id: ZenlinkAssetId) -> Result<CurrencyId, ()> {

	let _index = asset_id.asset_index;
		let disc = ((_index & 0x0000_0000_0000_ff00) >> 8) as u8;
		let symbol = (_index & 0x0000_0000_0000_00ff) as u8;
		match disc {
			0 => Ok(CurrencyId::Native),
			1 => Ok(CurrencyId::XCM(symbol)),
			2 => match symbol {
				0 => Ok(CurrencyId::Stellar(Asset::StellarNative)),
				1 => Ok(CurrencyId::Stellar(Asset::AlphaNum4 {
					code: *b"USDC",
					issuer: USDC_ISSUER,
				})),
				2 => Ok(CurrencyId::Stellar(Asset::AlphaNum4 {
					code: *b"TZS\0",
					issuer: TZS_ISSUER,
				})),
				3 => Ok(CurrencyId::Stellar(Asset::AlphaNum4 {
					code: *b"BRL\0",
					issuer: BRL_ISSUER,
				})),
				_ => return Err(()),
			},
			6 => {
				let token1_id = ((_index & 0x0000_0000_00FF_0000) >> 16) as u8;
				let token1_type = ((_index & 0x0000_0000_FF00_0000) >> 24) as u8;

				let token2_id = ((_index & 0x0000_00FF_0000_0000) >> 32) as u8;
				let token2_type = ((_index & 0x0000_FF00_0000_0000) >> 40) as u8;

				Ok(CurrencyId::ZenlinkLPToken(
					token1_id,
					token1_type,
					token2_id,
					token2_type,
				))
			},
			_ => Err(()),
		}
}

pub fn currency_id_to_zenlink_id(currency_id: CurrencyId, parachain_id: u32) -> Result<ZenlinkAssetId, ()> {
	let disc = discriminant(&currency_id) as u64;
	match currency_id {
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


#[cfg(test)]
mod zenlink_tests {
	use super::*;

	#[test]
	fn convert_zenlink_native_to_native_currency() {
		// Native ZenlinkAsset index = 0x0000_0000_0000_0000
		let _index = 0 as u64;
		let fake_native_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: NATIVE, asset_index: 0 as u64 };
		let currency: Result<CurrencyId, _> = zenlink_id_to_currency_id(fake_native_asset);
		assert_eq!(currency, Ok(CurrencyId::Native));
	}

	#[test]
	fn convert_zenlink_xcm_to_xcm_currency() {
		// XCM(0) ZenlinkAsset index = 0x0000_0000_0000_0100
		let _index = 0x0100 as u64;
		let fake_native_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: _index };
		let currency: Result<CurrencyId, _> = zenlink_id_to_currency_id(fake_native_asset);
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
		let fake_native_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: stellar_native_index,
		};
		let currency: Result<CurrencyId, _> = zenlink_id_to_currency_id(fake_native_asset);
		assert_eq!(currency, Ok(get_stellar_asset(0u8)));

		// Stellar USDC ZenlinkAsset index = 0x0000_0000_0000_0201
		let usdc_index = 0x0201 as u64;
		let fake_native_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: usdc_index };
		let currency: Result<CurrencyId, _> = zenlink_id_to_currency_id(fake_native_asset);
		assert_eq!(currency, Ok(get_stellar_asset(1u8)));

		// Stellar TZS ZenlinkAsset index = 0x0000_0000_0000_0202
		let tzs_index = 0x0202 as u64;
		let fake_native_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: tzs_index };
		let currency: Result<CurrencyId, _> = zenlink_id_to_currency_id(fake_native_asset);
		assert_eq!(currency, Ok(get_stellar_asset(2u8)));

		// Stellar BRL ZenlinkAsset index = 0x0000_0000_0000_0203
		let brl_index = 0x0203 as u64;
		let fake_native_asset =
			ZenlinkAssetId { chain_id: 1000u32, asset_type: LOCAL, asset_index: brl_index };
		let currency: Result<CurrencyId, _> = zenlink_id_to_currency_id(fake_native_asset);
		assert_eq!(currency, Ok(get_stellar_asset(3u8)));
	}

	#[test]
	fn convert_zenlink_lp_token_to_lp_token_currency() {
		// Native and XCM(0) LP token Zenlink index = 0x0000_0100_0000_0600
		let native_xcm_lp_token_index = 0x0000_0100_0000_0600 as u64;
		let fake_native_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: native_xcm_lp_token_index,
		};
		let currency: Result<CurrencyId, _> = zenlink_id_to_currency_id(fake_native_asset);
		let expected_currency: CurrencyId =
			CurrencyId::ZenlinkLPToken(0, 0, 0, 1);
		assert_eq!(currency, Ok(expected_currency));

		// XCM(0) and XCM(1) LP token Zenlink index = 0x0000_0101_0100_0600
		let xcm0_xcm1_lp_token_index = 0x0000_0101_0100_0600 as u64;
		let fake_native_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: xcm0_xcm1_lp_token_index,
		};
		let currency: Result<CurrencyId, _> = zenlink_id_to_currency_id(fake_native_asset);
		let expected_currency: CurrencyId =
			CurrencyId::ZenlinkLPToken(0, 1, 1, 1);
		assert_eq!(currency, Ok(expected_currency));

		// XCM(0) and Stellar Native LP Token Zenlink index = 0x0000_0200_0100_0600
		let xcm0_stellar_native_lp_token_index = 0x0000_0200_0100_0600 as u64;
		let fake_native_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: xcm0_stellar_native_lp_token_index,
		};
		let currency: Result<CurrencyId, _> = zenlink_id_to_currency_id(fake_native_asset);
		let expected_currency: CurrencyId =
			CurrencyId::ZenlinkLPToken(0, 1, 0, 2);
		assert_eq!(currency, Ok(expected_currency));

		// XCM(0) and Stellar USDC LP Token Zenlink index = 0x0000_0201_0100_0600
		let xcm0_stellar_usdc_lp_token_index = 0x0000_0201_0100_0600 as u64;
		let fake_native_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: xcm0_stellar_usdc_lp_token_index,
		};
		let currency: Result<CurrencyId, _> = zenlink_id_to_currency_id(fake_native_asset);
		let expected_currency: CurrencyId =
			CurrencyId::ZenlinkLPToken(0, 1, 1, 2);
		assert_eq!(currency, Ok(expected_currency));

		// Stellar Native and Stellar USDC LP Token Zenlink index = 0x0000_0201_0200_0600
		let stellar_native_stellar_usdc_lp_token_index = 0x0000_0201_0200_0600 as u64;
		let fake_native_asset = ZenlinkAssetId {
			chain_id: 1000u32,
			asset_type: LOCAL,
			asset_index: stellar_native_stellar_usdc_lp_token_index,
		};
		let currency: Result<CurrencyId, _> = zenlink_id_to_currency_id(fake_native_asset);
		let expected_currency: CurrencyId =
			CurrencyId::ZenlinkLPToken(0, 2, 1, 2);
		assert_eq!(currency, Ok(expected_currency));
	}
}
