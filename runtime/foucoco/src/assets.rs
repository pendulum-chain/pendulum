#![allow(non_snake_case)]

pub mod xcm_assets {
	use runtime_common::create_xcm_id;
	create_xcm_id!(RELAY, 0);
	create_xcm_id!(MOONBASE_DEV, 2);
}

/// Locations for native currency and all natively issued tokens
pub mod native_locations {
	use crate::ParachainInfo;
	use frame_support::traits::PalletInfoAccess;
	use xcm::latest::{
		Junction::{PalletInstance, Parachain},
		Junctions::{X1, X2},
		MultiLocation,
	};

	fn balances_pallet_id() -> u8 {
		crate::Balances::index() as u8
	}

	/// location of the native currency from the point of view of Foucoco parachain
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
}

pub mod moonbase_alpha_relay {
	pub mod moonbase {
		use runtime_common::parachain_asset_location;
		use xcm::latest::{
			Junction::{PalletInstance, Parachain},
			Junctions::X2,
		};

		pub const PARA_ID: u32 = 1000;
		pub const BALANCES_PALLET_INDEX: u8 = 110;

		parachain_asset_location!(
			DEV,
			X2(Parachain(PARA_ID), PalletInstance(BALANCES_PALLET_INDEX),)
		);
	}
}
