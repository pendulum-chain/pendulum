
pub mod asset_hub {
    use runtime_common::parachain_asset_location;

    pub const PARA_ID: u32 = 1000;
    pub const ASSET_PALLET_INDEX: u8 = 50;

    parachain_asset_location!(USDC, 1337);
    parachain_asset_location!(USDT, 1984);
}

#[cfg(test)]
mod tests {
	use super::asset_hub;
	use xcm::{
		latest::prelude::{PalletInstance, Parachain},
		prelude::GeneralIndex,
	};

	#[test]
	fn test_constants() {
		let expected_usdt_value = 1984;
		assert_eq!(asset_hub::USDT_ASSET_ID, expected_usdt_value);
	}
}