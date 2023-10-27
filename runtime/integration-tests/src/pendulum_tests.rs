use crate::{
	mock::{para_ext, polkadot_relay_ext, ParachainType, USDT_ASSET_ID},
	test_macros::{
		parachain1_transfer_asset_to_parachain2, parachain1_transfer_asset_to_parachain2_and_back,
		parachain1_transfer_incorrect_asset_to_parachain2_should_fail,
		transfer_10_relay_token_from_parachain_to_relay_chain,
		transfer_20_relay_token_from_relay_chain_to_parachain,
		transfer_native_token_from_pendulum_to_assethub,
	},
	PENDULUM_ID, POLKADOT_ASSETHUB_ID,
};

use frame_support::assert_ok;
use statemint_runtime as polkadot_asset_hub_runtime;
use xcm::latest::NetworkId;
use xcm_emulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, TestExt};

const DOT_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance = 3200000000; //The fees that relay chain will charge when transfer DOT to parachain. sovereign account of some parachain will receive transfer_amount - DOT_FEE

decl_test_relay_chain! {
	pub struct PolkadotRelay {
		Runtime = polkadot_runtime::Runtime,
		XcmConfig = polkadot_runtime::xcm_config::XcmConfig,
		new_ext = polkadot_relay_ext(),
	}
}

decl_test_parachain! {
	pub struct PendulumParachain {
		Runtime = pendulum_runtime::Runtime,
		RuntimeOrigin = pendulum_runtime::RuntimeOrigin,
		XcmpMessageHandler = pendulum_runtime::XcmpQueue,
		DmpMessageHandler = pendulum_runtime::DmpQueue,
		new_ext = para_ext(ParachainType::Pendulum),
	}
}

decl_test_parachain! {
	pub struct AssetHubParachain {
		Runtime = polkadot_asset_hub_runtime::Runtime,
		RuntimeOrigin = polkadot_asset_hub_runtime::RuntimeOrigin,
		XcmpMessageHandler = polkadot_asset_hub_runtime::XcmpQueue,
		DmpMessageHandler = polkadot_asset_hub_runtime::DmpQueue,
		new_ext = para_ext(ParachainType::PolkadotAssetHub),
	}
}

decl_test_network! {
	pub struct PolkadotMockNet {
		relay_chain = PolkadotRelay,
		parachains = vec![
			(1000, AssetHubParachain),
			(2094, PendulumParachain),
		],
	}
}

#[test]
fn transfer_dot_from_polkadot_to_pendulum() {
	transfer_20_relay_token_from_relay_chain_to_parachain!(
		PolkadotMockNet,
		polkadot_runtime,
		PolkadotRelay,
		pendulum_runtime,
		PendulumParachain,
		PENDULUM_ID,
		DOT_FEE_WHEN_TRANSFER_TO_PARACHAIN
	)
}

#[test]
fn transfer_dot_from_pendulum_to_polkadot() {
	transfer_10_relay_token_from_parachain_to_relay_chain!(
		PolkadotMockNet,
		polkadot_runtime,
		PolkadotRelay,
		pendulum_runtime,
		PendulumParachain
	);
}

#[test]
fn assethub_transfer_incorrect_asset_to_pendulum_should_fail() {
	parachain1_transfer_incorrect_asset_to_parachain2_should_fail!(
		polkadot_asset_hub_runtime,
		AssetHubParachain,
		pendulum_runtime,
		PendulumParachain,
		PENDULUM_ID
	);
}

#[test]
fn assethub_transfer_asset_to_pendulum() {
	parachain1_transfer_asset_to_parachain2!(
		polkadot_asset_hub_runtime,
		AssetHubParachain,
		USDT_ASSET_ID,
		pendulum_runtime,
		PendulumParachain,
		PENDULUM_ID
	);
}

#[test]
fn assethub_transfer_asset_to_pendulum_and_back() {
	let network_id = NetworkId::Polkadot;

	parachain1_transfer_asset_to_parachain2_and_back!(
		polkadot_asset_hub_runtime,
		AssetHubParachain,
		POLKADOT_ASSETHUB_ID,
		USDT_ASSET_ID,
		pendulum_runtime,
		PendulumParachain,
		PENDULUM_ID,
		network_id
	);
}

#[test]
fn transfer_native_token_to_assethub() {
	transfer_native_token_from_pendulum_to_assethub!(
		PolkadotMockNet,
		pendulum_runtime,
		PendulumParachain,
		polkadot_asset_hub_runtime,
		AssetHubParachain,
		PENDULUM_ID,
		POLKADOT_ASSETHUB_ID
	);
}
