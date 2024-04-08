#![allow(non_snake_case)]

pub mod xcm_assets {
	use runtime_common::create_xcm_id;

	create_xcm_id!(RELAY_DOT, 0);
	create_xcm_id!(ASSETHUB_USDT, 1);
	create_xcm_id!(ASSETHUB_USDC, 2);
	create_xcm_id!(EQUILIBRIUM_EQD, 3);
	create_xcm_id!(MOONBEAM_BRZ, 4);
	create_xcm_id!(POLKADEX_PDEX, 5);
	create_xcm_id!(MOONBEAM_GLMR, 6);
	create_xcm_id!(ASSETHUB_PINK, 7);
}
