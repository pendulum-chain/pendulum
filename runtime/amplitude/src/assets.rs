#![allow(non_snake_case)]

pub mod xcm_assets {
	use runtime_common::create_xcm_id;

	create_xcm_id!(RELAY_KSM, 0);
	create_xcm_id!(ASSETHUB_USDT, 1);
}

