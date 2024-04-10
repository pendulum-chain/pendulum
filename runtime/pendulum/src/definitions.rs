
pub mod xcm_assets {
	use runtime_common::create_xcm_id;
	create_xcm_id!(MOONBEAM_BRZ, 4);
}

pub mod asset_hub {
    use runtime_common::parachain_asset_location;
    
    pub const PARA_ID: u32 = 1000;
    pub const ASSET_PALLET_INDEX: u8 = 50;

    parachain_asset_location!(USDT, 1984);

}
pub mod moonbeam {
    use runtime_common::{parachain_asset_location};
    use xcm::latest::{
        Junction::{AccountKey20, PalletInstance, Parachain},
        Junctions::{X3},
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

}

#[cfg(test)]
mod tests {
	use super::{moonbeam};
	use xcm::{
		latest::prelude::{AccountKey20, PalletInstance, Parachain},
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

}