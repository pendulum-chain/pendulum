
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

