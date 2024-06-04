use crate::{
	mock::{para_ext, polkadot_relay_ext, ParachainType, USDT_ASSET_ID},
	sibling,
	test_macros::{
		moonbeam_transfers_token_and_handle_automation, parachain1_transfer_asset_to_parachain2,
		parachain1_transfer_asset_to_parachain2_and_back,
		parachain1_transfer_incorrect_asset_to_parachain2_should_fail,
		transfer_10_relay_token_from_parachain_to_relay_chain,
		transfer_20_relay_token_from_relay_chain_to_parachain,
		transfer_native_token_from_parachain1_to_parachain2_and_back,
	},
	ASSETHUB_ID, PENDULUM_ID, SIBLING_ID,
};

use frame_support::assert_ok;
use pendulum_runtime::definitions::moonbeam::PARA_ID as MOONBEAM_PARA_ID;
use statemint_runtime as polkadot_asset_hub_runtime;
use xcm::latest::NetworkId;
use xcm_emulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, TestExt};

// Native fee expected for each token according to the `fee_per_second` values defined in the mock
const NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance = 4000000000;
const DOT_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance =
	NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN / 4;
const MOONBEAM_BRZ_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance =
	2 * NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN;
const USDT_FEE_WHEN_TRANSFER_TO_PARACHAIN: polkadot_core_primitives::Balance =
	NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN / 2;

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
	pub struct SiblingParachain {
		Runtime = sibling::Runtime,
		RuntimeOrigin = sibling::RuntimeOrigin,
		XcmpMessageHandler = sibling::XcmpQueue,
		DmpMessageHandler = sibling::DmpQueue,
		new_ext = para_ext(ParachainType::Sibling),
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

decl_test_parachain! {
	pub struct MoonbeamParachain {
		Runtime = sibling::Runtime,
		RuntimeOrigin = sibling::RuntimeOrigin,
		XcmpMessageHandler = sibling::XcmpQueue,
		DmpMessageHandler = sibling::DmpQueue,
		new_ext = para_ext(ParachainType::Moonbeam),
	}
}

decl_test_network! {
	pub struct PolkadotMockNet {
		relay_chain = PolkadotRelay,
		parachains = vec![
			(1000, AssetHubParachain),
			(2094, PendulumParachain),
			(2004, MoonbeamParachain),
			(9999, SiblingParachain),
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
		PENDULUM_ID,
		USDT_FEE_WHEN_TRANSFER_TO_PARACHAIN
	);
}

#[test]
fn assethub_transfer_asset_to_pendulum_and_back() {
	let network_id = NetworkId::Polkadot;

	parachain1_transfer_asset_to_parachain2_and_back!(
		polkadot_asset_hub_runtime,
		AssetHubParachain,
		ASSETHUB_ID,
		USDT_ASSET_ID,
		pendulum_runtime,
		PendulumParachain,
		PENDULUM_ID,
		network_id,
		USDT_FEE_WHEN_TRANSFER_TO_PARACHAIN
	);
}

#[test]
fn transfer_native_token_from_pendulum_to_sibling_parachain_and_back() {
	transfer_native_token_from_parachain1_to_parachain2_and_back!(
		PolkadotMockNet,
		pendulum_runtime,
		PendulumParachain,
		sibling,
		SiblingParachain,
		PENDULUM_ID,
		SIBLING_ID,
		NATIVE_FEE_WHEN_TRANSFER_TO_PARACHAIN
	);
}

#[test]
fn moonbeam_transfers_token_and_handle_automation() {
	moonbeam_transfers_token_and_handle_automation!(
		PolkadotMockNet,
		pendulum_runtime,
		PendulumParachain,
		sibling,
		MoonbeamParachain,
		PENDULUM_ID,
		MOONBEAM_PARA_ID,
		MOONBEAM_BRZ_FEE_WHEN_TRANSFER_TO_PARACHAIN
	);
}
