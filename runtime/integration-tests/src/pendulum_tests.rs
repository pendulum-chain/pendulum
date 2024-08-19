use crate::{mock::{para_ext, polkadot_relay_ext, ParachainType, USDT_ASSET_ID}, sibling, test_macros::{
	moonbeam_transfers_token_and_handle_automation, parachain1_transfer_asset_to_parachain2,
	parachain1_transfer_asset_to_parachain2_and_back,
	parachain1_transfer_incorrect_asset_to_parachain2_should_fail,
	transfer_10_relay_token_from_parachain_to_relay_chain,
	transfer_20_relay_token_from_relay_chain_to_parachain,
	transfer_native_token_from_parachain1_to_parachain2_and_back,
}, ASSETHUB_ID, PENDULUM_ID, SIBLING_ID, genesis};

use frame_support::assert_ok;
#[allow(unused_imports)]
use pendulum_runtime::definitions::moonbeam::PARA_ID as MOONBEAM_PARA_ID;
use xcm::latest::NetworkId;
use xcm_emulator::{decl_test_networks, decl_test_relay_chains, decl_test_parachains, DefaultMessageProcessor};
use genesis::genesis;
use integration_tests_common::{
	impl_assert_events_helpers_for_parachain, constants::{polkadot, asset_hub_polkadot},
};
use frame_support::traits::OnInitialize;

// Native fee expected for each token according to the `fee_per_second` values defined in the mock
const NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance = 5000000000;
const DOT_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance =
	NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN / 4;
const MOONBEAM_BRZ_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance =
	2 * NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN;
const USDT_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance =
	NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN / 2;

decl_test_relay_chains! {
	#[api_version(5)]
	pub struct Polkadot {
		genesis = polkadot::genesis(),
		on_init = (),
		runtime = polkadot_runtime,
		core = {
			MessageProcessor: DefaultMessageProcessor<Polkadot>,
			SovereignAccountOf: polkadot_runtime::xcm_config::SovereignAccountOf,
		},
		pallets = {
			XcmPallet: polkadot_runtime::XcmPallet,
			Balances: polkadot_runtime::Balances,
			Hrmp: polkadot_runtime::Hrmp,
		}
	},
	// #[api_version(5)]
	// pub struct KusamaRelay {
	// 	genesis = kusama::genesis(),
	// 	on_init = (),
	// 	runtime = kusama_runtime,
	// 	core = {
	// 		MessageProcessor: DefaultMessageProcessor<KusamaRelay>,
	// 		SovereignAccountOf: kusama_runtime::xcm_config::SovereignAccountOf,
	// 	},
	// 	pallets = {
	// 		XcmPallet: kusama_runtime::XcmPallet,
	// 		Balances: kusama_runtime::Balances,
	// 		Hrmp: kusama_runtime::Hrmp,
	// 	}
	// },
}

decl_test_parachains! {
	pub struct AssetHubPolkadot {
		genesis = asset_hub_polkadot::genesis(),
		on_init = {
			asset_hub_polkadot_runtime::AuraExt::on_initialize(1);
		},
		runtime = asset_hub_polkadot_runtime,
		core = {
			XcmpMessageHandler: asset_hub_polkadot_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_polkadot_runtime::DmpQueue,
			LocationToAccountId: asset_hub_polkadot_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: asset_hub_polkadot_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: asset_hub_polkadot_runtime::PolkadotXcm,
			Assets: asset_hub_polkadot_runtime::Assets,
			Balances: asset_hub_polkadot_runtime::Balances,
		}
	},
	pub struct PendulumParachain {
		genesis = genesis(PENDULUM_ID),
		on_init = {
			pendulum_runtime::AuraExt::on_initialize(1);
		},
		runtime = pendulum_runtime,
		core = {
			XcmpMessageHandler: pendulum_runtime::XcmpQueue,
			DmpMessageHandler: pendulum_runtime::DmpQueue,
			LocationToAccountId: pendulum_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: pendulum_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: pendulum_runtime::PolkadotXcm,
			Tokens: pendulum_runtime::Tokens,
			Balances: pendulum_runtime::Balances,
			AssetRegistry: pendulum_runtime::AssetRegistry,
			XTokens: pendulum_runtime::XTokens,
		}
	},
	pub struct SiblingParachain {
		genesis = genesis(SIBLING_ID),
		on_init = {
			sibling::AuraExt::on_initialize(1);
		},
		runtime = sibling,
		core = {
			XcmpMessageHandler: sibling::XcmpQueue,
			DmpMessageHandler: sibling::DmpQueue,
			LocationToAccountId: sibling::LocationToAccountId,
			ParachainInfo: sibling::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: sibling::PolkadotXcm,
			Tokens: sibling::Tokens,
			Balances: sibling::Balances,
			XTokens: sibling::XTokens,
		}
	},
}


// decl_test_parachain! {
// 	pub struct MoonbeamParachain {
// 		Runtime = sibling::Runtime,
// 		XcmpMessageHandler = sibling::XcmpQueue,
// 		DmpMessageHandler = sibling::DmpQueue,
// 		new_ext = para_ext(ParachainType::Moonbeam),
// 	}
// }

decl_test_networks! {
	pub struct PolkadotMockNet {
		relay_chain = Polkadot,
		parachains = vec![
			AssetHubPolkadot,
			PendulumParachain,
			//MoonbeamParachain,
			//SiblingParachain,
		],
		bridge = ()
	},
}

// #[test]
// fn transfer_dot_from_polkadot_to_pendulum() {
// 	transfer_20_relay_token_from_relay_chain_to_parachain!(
// 		PolkadotMockNet,
// 		polkadot_runtime,
// 		Polkadot,
// 		pendulum_runtime,
// 		PendulumParachain,
// 		PENDULUM_ID,
// 		DOT_FEE_WHEN_TRANSFER_TO_PARACHAIN
// 	)
// }

// #[test]
// fn transfer_dot_from_pendulum_to_polkadot() {
// 	transfer_10_relay_token_from_parachain_to_relay_chain!(
// 		PolkadotMockNet,
// 		polkadot_runtime,
// 		Polkadot,
// 		pendulum_runtime,
// 		PendulumParachain
// 	);
// }

// #[test]
// fn assethub_transfer_incorrect_asset_to_pendulum_should_fail() {
// 	parachain1_transfer_incorrect_asset_to_parachain2_should_fail!(
// 		polkadot_asset_hub_runtime,
// 		AssetHubParachain,
// 		pendulum_runtime,
// 		PendulumParachain,
// 		PENDULUM_ID
// 	);
// }
//
// #[test]
// fn assethub_transfer_asset_to_pendulum() {
// 	parachain1_transfer_asset_to_parachain2!(
// 		asset_hub_polkadot_runtime,
// 		AssetHubPolkadot,
// 		USDT_ASSET_ID,
// 		pendulum_runtime,
// 		PendulumParachain,
// 		PENDULUM_ID,
// 		USDT_FEE_WHEN_TRANSFER_TO_PARACHAIN
// 	);
// }
//
#[test]
fn assethub_transfer_asset_to_pendulum_and_back() {
	let network_id = NetworkId::Polkadot;

	parachain1_transfer_asset_to_parachain2_and_back!(
		asset_hub_polkadot_runtime,
		AssetHubPolkadot,
		ASSETHUB_ID,
		USDT_ASSET_ID,
		pendulum_runtime,
		PendulumParachain,
		PENDULUM_ID,
		network_id,
		USDT_FEE_WHEN_TRANSFER_TO_PARACHAIN
	);
}
//
// #[test]
// fn transfer_native_token_from_pendulum_to_sibling_parachain_and_back() {
// 	transfer_native_token_from_parachain1_to_parachain2_and_back!(
// 		PolkadotMockNet,
// 		pendulum_runtime,
// 		PendulumParachain,
// 		sibling,
// 		SiblingParachain,
// 		PENDULUM_ID,
// 		SIBLING_ID,
// 		NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN
// 	);
// }
//
// #[test]
// fn moonbeam_transfers_token_and_handle_automation() {
// 	moonbeam_transfers_token_and_handle_automation!(
// 		PolkadotMockNet,
// 		pendulum_runtime,
// 		PendulumParachain,
// 		sibling,
// 		MoonbeamParachain,
// 		PENDULUM_ID,
// 		MOONBEAM_PARA_ID,
// 		MOONBEAM_BRZ_FEE_WHEN_TRANSFER_TO_PARACHAIN
// 	);
// }
