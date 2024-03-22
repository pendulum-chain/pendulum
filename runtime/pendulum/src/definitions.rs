
pub mod asset_hub {
    pub use runtime_common::parachains::asset_hub::*;
}

pub mod equilibrium {
    // TOOD maybe move all common imports to outside each module
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
