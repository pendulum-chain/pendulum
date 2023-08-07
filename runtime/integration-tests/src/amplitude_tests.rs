use crate::{
	mock::{kusama_relay_ext, para_ext, ParachainType, USDT_ASSET_ID},
	test_macros::{
		parachain1_transfer_asset_to_parachain2, parachain1_transfer_asset_to_parachain2_and_back,
		parachain1_transfer_incorrect_asset_to_parachain2_should_fail,
		transfer_10_relay_token_from_parachain_to_relay_chain,
		transfer_20_relay_token_from_relay_chain_to_parachain,
	},
	AMPLITUDE_ID, KUSAMA_ASSETHUB_ID,
};

use frame_support::assert_ok;
use statemine_runtime as kusama_asset_hub_runtime;
use xcm::latest::NetworkId;
use xcm_emulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, TestExt};

const KSM_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance = 3200000000;

decl_test_relay_chain! {
	pub struct KusamaRelay {
		Runtime = kusama_runtime::Runtime,
		XcmConfig = kusama_runtime::xcm_config::XcmConfig,
		new_ext = kusama_relay_ext(),
	}
}

decl_test_parachain! {
	pub struct AmplitudeParachain {
		Runtime = amplitude_runtime::Runtime,
		RuntimeOrigin = amplitude_runtime::RuntimeOrigin,
		XcmpMessageHandler = amplitude_runtime::XcmpQueue,
		DmpMessageHandler = amplitude_runtime::DmpQueue,
		new_ext = para_ext(ParachainType::Amplitude),
	}
}

decl_test_parachain! {
	pub struct AssetHubParachain {
		Runtime = kusama_asset_hub_runtime::Runtime,
		RuntimeOrigin = kusama_asset_hub_runtime::RuntimeOrigin,
		XcmpMessageHandler = kusama_asset_hub_runtime::XcmpQueue,
		DmpMessageHandler = kusama_asset_hub_runtime::DmpQueue,
		new_ext = para_ext(ParachainType::KusamaAssetHub),
	}
}

decl_test_network! {
	pub struct KusamaMockNet {
		relay_chain = KusamaRelay,
		parachains = vec![
			(1000, AssetHubParachain),
			(2124, AmplitudeParachain),
		],
	}
}

#[test]
fn transfer_ksm_from_kusama_to_amplitude() {
	transfer_20_relay_token_from_relay_chain_to_parachain!(
		KusamaMockNet,
		kusama_runtime,
		KusamaRelay,
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
		KusamaRelay,
		amplitude_runtime,
		AmplitudeParachain
	);
}

#[test]
fn assethub_transfer_incorrect_asset_to_amplitude_should_fail() {
	parachain1_transfer_incorrect_asset_to_parachain2_should_fail!(
		kusama_asset_hub_runtime,
		AssetHubParachain,
		amplitude_runtime,
		AmplitudeParachain,
		AMPLITUDE_ID
	);
}

#[test]
fn assethub_transfer_asset_to_amplitude() {
	parachain1_transfer_asset_to_parachain2!(
		kusama_asset_hub_runtime,
		AssetHubParachain,
		USDT_ASSET_ID,
		amplitude_runtime,
		AmplitudeParachain,
		AMPLITUDE_ID
	);
}

#[test]
fn assethub_transfer_asset_to_amplitude_and_back() {
	let network_id = NetworkId::Kusama;

	parachain1_transfer_asset_to_parachain2_and_back!(
		kusama_asset_hub_runtime,
		AssetHubParachain,
		KUSAMA_ASSETHUB_ID,
		USDT_ASSET_ID,
		amplitude_runtime,
		AmplitudeParachain,
		AMPLITUDE_ID,
		network_id
	);
}
