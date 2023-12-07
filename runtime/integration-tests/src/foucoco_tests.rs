use crate::{
	mock::{rococo_relay_ext, para_ext, ParachainType},
	sibling,
	test_macros::{
		transfer_DEV_token_from_parachain1_to_parachain2_and_back,
	},
	FOUCOCO_ID, SIBLING_ID,
};

use frame_support::assert_ok;
use xcm_emulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, TestExt};
use polkadot_primitives::runtime_api::runtime_decl_for_parachain_host::ParachainHostV4;

decl_test_relay_chain! {
	pub struct RococoRelay {
		Runtime = rococo_runtime::Runtime,
		XcmConfig = rococo_runtime::xcm_config::XcmConfig,
		new_ext = rococo_relay_ext(),
	}
}

decl_test_parachain! {
	pub struct FoucocoParachain {
		Runtime = foucoco_runtime::Runtime,
		RuntimeOrigin = foucoco_runtime::RuntimeOrigin,
		XcmpMessageHandler = foucoco_runtime::XcmpQueue,
		DmpMessageHandler = foucoco_runtime::DmpQueue,
		new_ext = para_ext(ParachainType::Foucoco),
	}
}

decl_test_parachain! {
	pub struct SiblingParachain {
		Runtime = sibling::Runtime,
		RuntimeOrigin = sibling::RuntimeOrigin,
		XcmpMessageHandler = sibling::XcmpQueue,
		DmpMessageHandler = sibling::DmpQueue,
		new_ext = para_ext(ParachainType::Sibling),
	}
}

decl_test_network! {
	pub struct RococoMockNet {
		relay_chain = RococoRelay,
		parachains = vec![
			(2124, FoucocoParachain),
			(1000, SiblingParachain),
		],
	}
}

#[test]
fn transfer_DEV_token_from_moonbeam_foucoco_to_sibling_parachain_and_back() {
	transfer_DEV_token_from_parachain1_to_parachain2_and_back!(
		RococoMockNet,
		foucoco_runtime,
		FoucocoParachain,
		sibling,
		SiblingParachain,
		FOUCOCO_ID,
		SIBLING_ID
	);
}

