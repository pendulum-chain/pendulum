use frame_support::{
	assert_ok,
	traits::{fungible::Mutate, fungibles::Inspect, Currency, GenesisBuild},
};
use pendulum_runtime::{
	Balances, PendulumCurrencyId, Runtime, RuntimeOrigin, System, Tokens, XTokens,
};
use polkadot_core_primitives::{AccountId, Balance, BlockNumber};
use polkadot_parachain::primitives::{Id as ParaId, Sibling};
use polkadot_primitives::v2::{MAX_CODE_SIZE, MAX_POV_SIZE};
use polkadot_runtime_parachains::configuration::HostConfiguration;
use sp_runtime::{traits::AccountIdConversion, MultiAddress};
use xcm::{
	latest::{
		AssetId, Fungibility, Junction, Junction::*, Junctions::*, MultiAsset, MultiLocation,
		NetworkId, WeightLimit,
	},
	v2::{Instruction::WithdrawAsset, Xcm},
	VersionedMultiLocation,
};
const DOT_FEE: Balance = 3200000000;
const ASSET_ID: u32 = 1984; //Real USDT Asset ID from Statemint
const INCORRECT_ASSET_ID: u32 = 0;
pub const UNIT: Balance = 1_000_000_000_000;
pub const TEN: Balance = 10_000_000_000_000;
use xcm_emulator::{
	decl_test_network, decl_test_parachain, decl_test_relay_chain, Junctions, TestExt, Weight,
};
pub fn dot(amount: Balance) -> Balance {
	amount * 10u128.saturating_pow(9)
}
pub const ALICE: [u8; 32] = [4u8; 32];
pub const BOB: [u8; 32] = [5u8; 32];
pub const INITIAL_BALANCE: u128 = 1_000_000_000;

mod setup;
use setup::*;
mod polkadot_test_net;
mod tests;
