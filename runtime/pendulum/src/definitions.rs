
pub mod asset_hub {
    use runtime_common::parachain_asset_location;
    
    pub const PARA_ID: u32 = 1000;
    pub const ASSET_PALLET_INDEX: u8 = 50;

    parachain_asset_location!(USDC, 1337);
    parachain_asset_location!(USDT, 1984);
    parachain_asset_location!(PINK, 23);
}


pub mod equilibrium {

    use runtime_common::parachain_asset_location;
    pub const PARA_ID: u32 = 2011;
    pub const ASSET_PALLET_INDEX: u8 = 11;

    parachain_asset_location!(EQ, 25_969);
    parachain_asset_location!(EQD, 6_648_164);
}

pub mod moonbeam {
    use runtime_common::parachain_asset_location;
    use xcm::latest::{
        Junction::{AccountKey20, PalletInstance, Parachain},
        Junctions::{X2, X3},
    };

    pub const PARA_ID: u32 = 2004;
    pub const ASSET_PALLET_INDEX: u8 = 110;
    pub const BALANCES_PALLET_INDEX: u8 = 10;

    // The address of the BRZ token on Moonbeam `0x3225edCe8aD30Ae282e62fa32e7418E4b9cf197b` as byte array
    pub const BRZ_ASSET_ACCOUNT_IN_BYTES: [u8; 20] = [
        50, 37, 237, 206, 138, 211, 10, 226, 130, 230, 47, 163, 46, 116, 24, 228, 185, 207, 25, 123
    ];

    parachain_asset_location!(
        BRZ,
        X3(
            Parachain(PARA_ID),
            PalletInstance(ASSET_PALLET_INDEX),
            AccountKey20 { network: None, key: BRZ_ASSET_ACCOUNT_IN_BYTES }
        )
    );

    parachain_asset_location!(
        GLMR,
        X2(Parachain(PARA_ID), PalletInstance(BALANCES_PALLET_INDEX))
    );
}

pub mod polkadex {
    use runtime_common::parachain_asset_location;
    use xcm::latest::{Junction::Parachain, Junctions::X1};

    pub const PARA_ID: u32 = 2040;

    parachain_asset_location!(PDEX, X1(Parachain(PARA_ID)));
}

#[cfg(test)]
mod tests {
	use super::{polkadex, equilibrium, moonbeam};
	use xcm::{
		latest::prelude::{AccountKey20, PalletInstance, Parachain},
		prelude::GeneralIndex,
	};

	#[test]
	fn test_brz() {
		let brz_loc = moonbeam::BRZ_location();
		let mut junctions = brz_loc.interior().into_iter();

		assert_eq!(junctions.next(), Some(&Parachain(moonbeam::PARA_ID)));
		assert_eq!(junctions.next(), Some(&PalletInstance(moonbeam::ASSET_PALLET_INDEX)));
		assert_eq!(
			junctions.next(),
			Some(&AccountKey20 { network: None, key: moonbeam::BRZ_ASSET_ACCOUNT_IN_BYTES })
		);
		assert_eq!(junctions.next(), None);
	}

	#[test]
	fn test_glmr() {
		let glmr_loc = moonbeam::GLMR_location();
		let mut junctions = glmr_loc.interior().into_iter();

		assert_eq!(junctions.next(), Some(&Parachain(moonbeam::PARA_ID)));
		assert_eq!(junctions.next(), Some(&PalletInstance(moonbeam::BALANCES_PALLET_INDEX)));
		assert_eq!(junctions.next(), None);
	}



	#[test]
	fn test_constants() {
		let expected_eq_value = 25_969;
		assert_eq!(equilibrium::EQ_ASSET_ID, expected_eq_value);

		let eq_interior = equilibrium::EQ_location().interior;
		let mut junctions = eq_interior.into_iter();

		assert_eq!(junctions.next(), Some(Parachain(equilibrium::PARA_ID)));
		assert_eq!(junctions.next(), Some(PalletInstance(equilibrium::ASSET_PALLET_INDEX)));
		assert_eq!(junctions.next(), Some(GeneralIndex(equilibrium::EQ_ASSET_ID)));
		assert_eq!(junctions.next(), None);

	}
}