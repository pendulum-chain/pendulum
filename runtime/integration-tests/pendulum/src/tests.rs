use crate::{polkadot_test_net::*, setup::*, *};

use sp_runtime::{traits::AccountIdConversion, MultiAddress};

use xcm_emulator::{Junctions, TestExt};

use xcm::{
	latest::{
		AssetId, Fungibility, Junction, Junction::*, Junctions::*, MultiAsset, MultiLocation,
		NetworkId, WeightLimit,
	},
	v2::{Instruction::WithdrawAsset, Xcm},
	VersionedMultiLocation,
};

use pendulum_runtime::{
	Balances, PendulumCurrencyId, Runtime, RuntimeOrigin, System, Tokens, XTokens,
};

use frame_support::{
	assert_ok,
	traits::{fungible::Mutate, fungibles::Inspect, Currency, GenesisBuild},
};

use polkadot_core_primitives::{AccountId, Balance, BlockNumber};
use polkadot_parachain::primitives::{Id as ParaId, Sibling};

const DOT_FEE: Balance = 3200000000;
const ASSET_ID: u32 = 1984; //Real USDT Asset ID from Statemint
const INCORRECT_ASSET_ID: u32 = 0;
const FEE: u128 = 421434140;
pub const UNIT: Balance = 1_000_000_000_000;
pub const TEN: Balance = 10_000_000_000_000;

#[test]
fn transfer_polkadot_from_relay_chain_to_pendulum() {
	MockNet::reset();

	let transfer_amount: Balance = dot(20);
	let mut orml_tokens_before = 0;
	PendulumParachain::execute_with(|| {
		orml_tokens_before = pendulum_runtime::Tokens::balance(
			pendulum_runtime::PendulumCurrencyId::XCM(0),
			&ALICE.into(),
		);
	});

	Relay::execute_with(|| {
		assert_ok!(polkadot_runtime::XcmPallet::reserve_transfer_assets(
			polkadot_runtime::RuntimeOrigin::signed(ALICE.into()),
			Box::new(Parachain(2094).into().into()),
			Box::new(Junction::AccountId32 { network: NetworkId::Any, id: ALICE }.into().into()),
			Box::new((Here, transfer_amount).into()),
			0
		));
	});

	PendulumParachain::execute_with(|| {
		use pendulum_runtime::{RuntimeEvent, System, Tokens};

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Tokens(orml_tokens::Event::Deposited { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::DmpQueue(cumulus_pallet_dmp_queue::Event::ExecutedDownward { .. })
		)));
	});

	PendulumParachain::execute_with(|| {
		assert_eq!(
			pendulum_runtime::Tokens::balance(
				pendulum_runtime::PendulumCurrencyId::XCM(0),
				&ALICE.into()
			),
			orml_tokens_before + transfer_amount - DOT_FEE
		);
	});
}

#[test]
fn transfer_polkadot_from_pendulum_to_relay_chain() {
	MockNet::reset();

	let transfer_dot_amount: Balance = dot(10);

	Relay::execute_with(|| {
		let after_bob_free_balance = polkadot_runtime::Balances::free_balance(&BOB.into());
		assert_eq!(after_bob_free_balance, dot(100));
	});

	PendulumParachain::execute_with(|| {
		assert_ok!(pendulum_runtime::XTokens::transfer(
			pendulum_runtime::RuntimeOrigin::signed(BOB.into()),
			pendulum_runtime::PendulumCurrencyId::XCM(0),
			transfer_dot_amount,
			Box::new(
				MultiLocation::new(
					1,
					Junctions::X1(Junction::AccountId32 { id: BOB, network: NetworkId::Any })
				)
				.into()
			),
			WeightLimit::Limited(4_000_000_000),
		));
	});

	PendulumParachain::execute_with(|| {
		use pendulum_runtime::{RuntimeEvent, System};

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Tokens(orml_tokens::Event::Withdrawn { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XTokens(orml_xtokens::Event::TransferredMultiAssets { .. })
		)));
	});

	Relay::execute_with(|| {
		use polkadot_runtime::{RuntimeEvent, System};

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Ump(polkadot_runtime_parachains::ump::Event::ExecutedUpward { .. })
		)));
	});

	Relay::execute_with(|| {
		let after_bob_free_balance = polkadot_runtime::Balances::free_balance(&BOB.into());
		assert_eq!(after_bob_free_balance, dot(100) + transfer_dot_amount - FEE);
	});
}

#[test]
fn statemint_transfer_incorrect_asset_to_pendulum_fails() {
	let para_2094: AccountId = Sibling::from(2094).into_account_truncating();

	PendulumParachain::execute_with(|| {
		assert_eq!(
			pendulum_runtime::Tokens::balance(
				pendulum_runtime::PendulumCurrencyId::XCM(1),
				&BOB.into()
			),
			0
		);
	});

	Statemint::execute_with(|| {
		use statemint_runtime::*;

		let origin = RuntimeOrigin::signed(ALICE.into());
		Balances::make_free_balance_be(&ALICE.into(), TEN);
		Balances::make_free_balance_be(&BOB.into(), UNIT);

		// If using non root, create custom asset cost 0.1 Dot
		// We're using force_create here to make sure asset is sufficient.
		assert_ok!(Assets::force_create(
			RuntimeOrigin::root(),
			INCORRECT_ASSET_ID.into(),
			MultiAddress::Id(ALICE.into()),
			true,
			UNIT / 100
		));

		assert_ok!(Assets::mint(
			origin.clone(),
			INCORRECT_ASSET_ID.into(),
			MultiAddress::Id(ALICE.into()),
			1000 * UNIT
		));

		// need to have some DOT to be able to receive user assets
		Balances::make_free_balance_be(&para_2094, UNIT);

		assert_ok!(PolkadotXcm::limited_reserve_transfer_assets(
			origin.clone(),
			Box::new(MultiLocation::new(1, X1(Parachain(2094))).into()),
			Box::new(Junction::AccountId32 { id: BOB, network: NetworkId::Any }.into().into()),
			Box::new(
				(X2(PalletInstance(50), GeneralIndex(INCORRECT_ASSET_ID as u128)), TEN).into()
			),
			0,
			WeightLimit::Unlimited
		));

		assert_eq!(990 * UNIT, Assets::balance(INCORRECT_ASSET_ID, &AccountId::from(ALICE)));
		assert_eq!(0, Assets::balance(INCORRECT_ASSET_ID, &AccountId::from(BOB)));

		assert_eq!(TEN, Assets::balance(INCORRECT_ASSET_ID, &para_2094));
		// the DOT balance of sibling parachain sovereign account is not changed
		assert_eq!(UNIT, Balances::free_balance(&para_2094));
	});

	// Rerun the Statemint::execute to actually send the egress message via XCM
	Statemint::execute_with(|| {});

	PendulumParachain::execute_with(|| {
		use pendulum_runtime::{RuntimeEvent, System};

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Fail {
				message_hash: _,
				error: xcm::v2::Error::FailedToTransactAsset(..),
				weight: _
			})
		)));
	});
}

#[test]
fn statemint_transfer_asset_to_pendulum() {
	let para_2094: AccountId = Sibling::from(2094).into_account_truncating();

	PendulumParachain::execute_with(|| {
		assert_eq!(
			pendulum_runtime::Tokens::balance(
				pendulum_runtime::PendulumCurrencyId::XCM(1),
				&BOB.into()
			),
			0
		);
	});

	Statemint::execute_with(|| {
		use statemint_runtime::*;

		let origin = RuntimeOrigin::signed(ALICE.into());
		Balances::make_free_balance_be(&ALICE.into(), TEN);
		Balances::make_free_balance_be(&BOB.into(), UNIT);

		// If using non root, create custom asset cost 0.1 Dot
		// We're using force_create here to make sure asset is sufficient.
		assert_ok!(Assets::force_create(
			RuntimeOrigin::root(),
			ASSET_ID.into(),
			MultiAddress::Id(ALICE.into()),
			true,
			UNIT / 100
		));

		assert_ok!(Assets::mint(
			origin.clone(),
			ASSET_ID.into(),
			MultiAddress::Id(ALICE.into()),
			1000 * UNIT
		));

		// need to have some DOT to be able to receive user assets
		Balances::make_free_balance_be(&para_2094, UNIT);

		assert_ok!(PolkadotXcm::limited_reserve_transfer_assets(
			origin.clone(),
			Box::new(MultiLocation::new(1, X1(Parachain(2094))).into()),
			Box::new(Junction::AccountId32 { id: BOB, network: NetworkId::Any }.into().into()),
			Box::new((X2(PalletInstance(50), GeneralIndex(ASSET_ID as u128)), TEN).into()),
			0,
			WeightLimit::Unlimited
		));

		assert_eq!(990 * UNIT, Assets::balance(ASSET_ID, &AccountId::from(ALICE)));
		assert_eq!(0, Assets::balance(ASSET_ID, &AccountId::from(BOB)));

		assert_eq!(TEN, Assets::balance(ASSET_ID, &para_2094));
		// the DOT balance of sibling parachain sovereign account is not changed
		assert_eq!(UNIT, Balances::free_balance(&para_2094));
	});

	// Rerun the Statemint::execute to actually send the egress message via XCM
	Statemint::execute_with(|| {});

	PendulumParachain::execute_with(|| {
		use pendulum_runtime::{RuntimeEvent, System};
		for i in System::events().iter() {
			println!(" Pendulum_runtime {:?}", i);
		}

		assert!(System::events()
			.iter()
			.any(|r| matches!(r.event, RuntimeEvent::Tokens(orml_tokens::Event::Endowed { .. }))));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Tokens(orml_tokens::Event::Deposited { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Success { .. })
		)));

		assert_eq!(
			pendulum_runtime::Tokens::balance(
				pendulum_runtime::PendulumCurrencyId::XCM(1),
				&BOB.into()
			),
			TEN
		);
	});
}

#[test]
fn statemint_transfer_asset_to_statemint() {
	statemint_transfer_asset_to_pendulum();

	Statemint::execute_with(|| {});

	PendulumParachain::execute_with(|| {
		assert_eq!(TEN, Tokens::balance(PendulumCurrencyId::XCM(1), &AccountId::from(BOB)));
		// ensure sender has enough PEN balance to be charged as fee
		assert_ok!(Balances::mint_into(&AccountId::from(BOB), TEN));

		assert_ok!(XTokens::transfer(
			RuntimeOrigin::signed(BOB.into()),
			PendulumCurrencyId::XCM(1),
			UNIT * 1,
			Box::new(
				MultiLocation::new(
					1,
					X2(
						Parachain(1000),
						Junction::AccountId32 { network: NetworkId::Any, id: BOB.into() }
					)
				)
				.into()
			),
			WeightLimit::Limited(10_000_000_000),
		));

		assert_eq!(
			TEN - 1 * UNIT, //inital balance - one unit
			Tokens::balance(PendulumCurrencyId::XCM(1), &AccountId::from(BOB))
		);

		use pendulum_runtime::{RuntimeEvent, System};
		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. })
		)));

		// assert_eq!(TEN - ksm_fee_amount, Tokens::free_balance(KSM, &AccountId::from(BOB)));
	});

	Statemint::execute_with(|| {
		use statemint_runtime::*;

		// https://github.com/paritytech/cumulus/pull/1278 support using self sufficient asset
		// for paying xcm execution fee on Statemint.
		assert_eq!(990_000_000_000, Assets::balance(ASSET_ID, &AccountId::from(BOB)));
	});
}
