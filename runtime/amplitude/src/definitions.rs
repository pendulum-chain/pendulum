
pub mod asset_hub {
    use runtime_common::parachain_asset_location;

    pub const PARA_ID: u32 = 1000;
    pub const ASSET_PALLET_INDEX: u8 = 50;

    parachain_asset_location!(USDC, 1337);
    parachain_asset_location!(USDT, 1984);
    parachain_asset_location!(PINK, 23);
}

#[cfg(test)]
mod tests {
	use super::asset_hub;
	use xcm::{
		latest::prelude::{PalletInstance, Parachain},
		prelude::GeneralIndex,
	};

	#[test]
	fn test_PINK() {
		let pink_loc = asset_hub::PINK_location();
		let mut junctions = pink_loc.interior().into_iter();

		assert_eq!(junctions.next(), Some(&Parachain(asset_hub::PARA_ID)));
		assert_eq!(junctions.next(), Some(&PalletInstance(asset_hub::ASSET_PALLET_INDEX)));
		assert_eq!(junctions.next(), Some(&GeneralIndex(asset_hub::PINK_ASSET_ID)));
		assert_eq!(junctions.next(), None);
	}

	#[test]
	fn test_constants() {
		let expected_USDT_value = 1984;
		assert_eq!(asset_hub::USDT_ASSET_ID, expected_USDT_value);
	}
}