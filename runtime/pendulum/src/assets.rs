#![allow(non_snake_case)]

pub mod xcm_assets {
	use runtime_common::create_xcm_id;

	create_xcm_id!(RELAY_DOT, 0);
	create_xcm_id!(ASSETHUB_USDT, 1);
	create_xcm_id!(ASSETHUB_USDC, 2);
	create_xcm_id!(EQUILIBRIUM_EQD, 3);
	create_xcm_id!(MOONBEAM_BRZ, 4);
	create_xcm_id!(POLKADEX_PDEX, 5);
	create_xcm_id!(MOONBEAM_GLMR, 6);
}

/// Locations for native currency and all natively issued tokens
pub mod native_locations {
	use crate::ParachainInfo;
	use frame_support::traits::PalletInfoAccess;
	use xcm::latest::{
		Junction::{GeneralIndex, PalletInstance, Parachain},
		Junctions::{X1, X2, X3, X4},
		MultiLocation,
	};

	const TOKEN_IN_CURRENCY_ID: u128 = 4;

	fn tokens_pallet_id() -> u8 {
		crate::Tokens::index() as u8
	}

	fn balances_pallet_id() -> u8 {
		crate::Balances::index() as u8
	}

	/// location of the native currency from the point of view of Pendulum parachain
	pub fn native_location_local_pov() -> MultiLocation {
		MultiLocation { parents: 0, interior: X1(PalletInstance(balances_pallet_id())) }
	}

	/// location of the native currency from the point of view of other parachains(external)
	pub fn native_location_external_pov() -> MultiLocation {
		MultiLocation {
			parents: 1,
			interior: X2(
				Parachain(ParachainInfo::parachain_id().into()),
				PalletInstance(balances_pallet_id()),
			),
		}
	}

	/// EURC location from the point of view of Pendulum parachain
	pub fn EURC_location_local_pov() -> MultiLocation {
		MultiLocation {
			parents: 0,
			interior: X3(
				PalletInstance(tokens_pallet_id()),
				GeneralIndex(TOKEN_IN_CURRENCY_ID), // index of the Token variant of CurrencyId enum
				GeneralIndex(super::tokens::EURC_TOKEN_INDEX as u128),
			),
		}
	}

	/// EURC location from the point of view of other parachains(external)
	pub fn EURC_location_external_pov() -> MultiLocation {
		MultiLocation {
			parents: 1,
			interior: X4(
				Parachain(ParachainInfo::parachain_id().into()),
				PalletInstance(tokens_pallet_id()),
				GeneralIndex(TOKEN_IN_CURRENCY_ID),
				GeneralIndex(super::tokens::EURC_TOKEN_INDEX as u128),
			),
		}
	}
}

/// Tokens issued by Pendulum
pub mod tokens {
	use spacewalk_primitives::CurrencyId;

	// The index of EURC in the token variant of CurrencyId
	pub const EURC_TOKEN_INDEX: u64 = 0;

	pub const EURC_ID: CurrencyId = CurrencyId::Token(EURC_TOKEN_INDEX);
}
