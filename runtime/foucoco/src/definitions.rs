
pub mod moonbase_alpha_relay {
    pub mod moonbase_alpha {
        use runtime_common::parachain_asset_location;
        use xcm::latest::{
            Junction::{PalletInstance, Parachain},
            Junctions::X2,
        };

        pub const PARA_ID: u32 = 1000;
        pub const BALANCES_PALLET_INDEX: u8 = 3;

        parachain_asset_location!(
            DEV,
            X2(Parachain(PARA_ID), PalletInstance(BALANCES_PALLET_INDEX))
        );
    }
}