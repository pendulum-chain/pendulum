
pub mod asset_hub {
    use runtime_common::parachain_asset_location;

    pub const PARA_ID: u32 = 1000;
    pub const ASSET_PALLET_INDEX: u8 = 50;

    parachain_asset_location!(USDC, 1337);
    parachain_asset_location!(USDT, 1984);
    parachain_asset_location!(PINK, 23);
}
