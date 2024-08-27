use crate::{mock::{ USDT_ASSET_ID, assets_metadata_for_registry_amplitude},
	sibling,
	test_macros::{
		parachain1_transfer_asset_to_parachain2, parachain1_transfer_asset_to_parachain2_and_back,
		parachain1_transfer_incorrect_asset_to_parachain2_should_fail,
		transfer_10_relay_token_from_parachain_to_relay_chain,
		transfer_20_relay_token_from_relay_chain_to_parachain,
		transfer_native_token_from_parachain1_to_parachain2_and_back,
	},
	AMPLITUDE_ID, ASSETHUB_ID, SIBLING_ID,
};

use frame_support::assert_ok;
use asset_hub_kusama_runtime;
use integration_tests_common::{
	constants::{kusama, asset_hub_kusama},
};

use crate::genesis::{genesis_gen, genesis_sibling};
use xcm::latest::NetworkId;
use xcm_emulator::{decl_test_networks, decl_test_parachains, decl_test_relay_chains, DefaultMessageProcessor};
use frame_support::traits::OnInitialize;


// Native fee expected for each token according to the `fee_per_second` values defined in the mock
const NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance = 4000000000;
const BASE_FEE_WHEN_TRANSFER_NON_NATIVE_ASSET: polkadot_core_primitives::Balance = 5000000000;
const KSM_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance = BASE_FEE_WHEN_TRANSFER_NON_NATIVE_ASSET/ 20;
const USDT_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance =
	BASE_FEE_WHEN_TRANSFER_NON_NATIVE_ASSET / 10;

decl_test_relay_chains! {
	#[api_version(5)]
	pub struct Kusama {
		genesis = kusama::genesis(),
		on_init = (),
		runtime = kusama_runtime,
		core = {
			MessageProcessor: DefaultMessageProcessor<Kusama>,
			SovereignAccountOf: kusama_runtime::xcm_config::SovereignAccountOf,
		},
		pallets = {
			XcmPallet: kusama_runtime::XcmPallet,
			Balances: kusama_runtime::Balances,
			Hrmp: kusama_runtime::Hrmp,
		}
	},
}

decl_test_parachains! {
	pub struct AssetHubKusama {
		genesis = asset_hub_kusama::genesis(),
		on_init = {
			asset_hub_kusama_runtime::AuraExt::on_initialize(1);
		},
		runtime = asset_hub_kusama_runtime,
		core = {
			XcmpMessageHandler: asset_hub_kusama_runtime::XcmpQueue,
			DmpMessageHandler: asset_hub_kusama_runtime::DmpQueue,
			LocationToAccountId: asset_hub_kusama_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: asset_hub_kusama_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: asset_hub_kusama_runtime::PolkadotXcm,
			Assets: asset_hub_kusama_runtime::Assets,
			Balances: asset_hub_kusama_runtime::Balances,
		}
	},
	pub struct AmplitudeParachain {
		genesis = genesis_gen!(amplitude_runtime, AMPLITUDE_ID, assets_metadata_for_registry_amplitude),
		on_init = {
			amplitude_runtime::AuraExt::on_initialize(1);
		},
		runtime = amplitude_runtime,
		core = {
			XcmpMessageHandler: amplitude_runtime::XcmpQueue,
			DmpMessageHandler: amplitude_runtime::DmpQueue,
			LocationToAccountId: amplitude_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: amplitude_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: amplitude_runtime::PolkadotXcm,
			Tokens: amplitude_runtime::Tokens,
			Balances: amplitude_runtime::Balances,
			AssetRegistry: amplitude_runtime::AssetRegistry,
			XTokens: amplitude_runtime::XTokens,
		}
	},
	pub struct SiblingParachainAmplitude {
		genesis = genesis_sibling(SIBLING_ID),
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

decl_test_networks! {
	pub struct KusamaMockNet {
		relay_chain = Kusama,
		parachains = vec![
			AssetHubKusama,
			AmplitudeParachain,
			SiblingParachainAmplitude,
		],
		bridge = ()
	},
}
#[test]
fn transfer_ksm_from_kusama_to_amplitude() {
	transfer_20_relay_token_from_relay_chain_to_parachain!(
		KusamaMockNet,
		kusama_runtime,
		Kusama,
		amplitude_runtime,
		AmplitudeParachain,
		AMPLITUDE_ID,
		KSM_FEE_WHEN_TRANSFER_TO_PARACHAIN
	);
}


#[test]
fn transfer_ksm_from_amplitude_to_kusama() {
	transfer_10_relay_token_from_parachain_to_relay_chain!(
		KusamaMockNet,
		kusama_runtime,
		Kusama,
		amplitude_runtime,
		AmplitudeParachain,
		AMPLITUDE_ID,
		KSM_FEE_WHEN_TRANSFER_TO_PARACHAIN
	);
}
//
#[test]
fn assethub_transfer_incorrect_asset_to_amplitude_should_fail() {
	parachain1_transfer_incorrect_asset_to_parachain2_should_fail!(
		asset_hub_kusama_runtime,
		AssetHubKusama,
		amplitude_runtime,
		AmplitudeParachain,
		AMPLITUDE_ID
	);
}

#[test]
fn assethub_transfer_asset_to_amplitude() {
	parachain1_transfer_asset_to_parachain2!(
		asset_hub_kusama_runtime,
		AssetHubKusama,
		USDT_ASSET_ID,
		amplitude_runtime,
		AmplitudeParachain,
		AMPLITUDE_ID,
		USDT_FEE_WHEN_TRANSFER_TO_PARACHAIN
	);
}

#[test]
fn assethub_transfer_asset_to_amplitude_and_back() {
	let network_id = NetworkId::Kusama;

	parachain1_transfer_asset_to_parachain2_and_back!(
		asset_hub_kusama_runtime,
		AssetHubKusama,
		ASSETHUB_ID,
		USDT_ASSET_ID,
		amplitude_runtime,
		AmplitudeParachain,
		AMPLITUDE_ID,
		network_id,
		USDT_FEE_WHEN_TRANSFER_TO_PARACHAIN
	);
}

#[test]
fn transfer_native_token_from_amplitude_to_sibling_parachain_and_back() {
	transfer_native_token_from_parachain1_to_parachain2_and_back!(
		KusamaMockNet,
		amplitude_runtime,
		AmplitudeParachain,
		sibling,
		SiblingParachainAmplitude,
		AMPLITUDE_ID,
		SIBLING_ID,
		NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN
	);
}
