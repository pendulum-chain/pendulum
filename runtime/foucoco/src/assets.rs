#![allow(non_snake_case)]

pub mod xcm_assets {
	use runtime_common::create_xcm_id;
	create_xcm_id!(RELAY, 0);
	create_xcm_id!(MOONBASE_DEV, 1);
}
