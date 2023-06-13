use crate::{polkadot_test_net::*, setup::*};
use frame_support::{
	assert_ok,
	traits::{fungible::Mutate, fungibles::Inspect, Currency},
};
use pendulum_runtime::{Balances, PendulumCurrencyId, RuntimeOrigin, Tokens, XTokens};
use sp_runtime::{traits::AccountIdConversion, MultiAddress};
use xcm::latest::{Junction, Junction::*, Junctions::*, MultiLocation, NetworkId, WeightLimit};
use xcm_emulator::TestExt;


use pendulum_runtime::{RuntimeEvent, System};
use polkadot_core_primitives::{AccountId, Balance};
use polkadot_parachain::primitives::Sibling;
use xcm::v3::Weight;

const DOT_FEE_WHEN_TRANSFER_TO_PARACHAIN: Balance = 3200000000; //The fees that relay chain will charge when transfer DOT to parachain. sovereign account of some parachain will receive transfer_amount - DOT_FEE
const ASSET_ID: u32 = 1984; //Real USDT Asset ID from Statemint
const INCORRECT_ASSET_ID: u32 = 0; //Incorrect asset id that pendulum is not supporting pendulum_runtime xcm_config
pub const UNIT: Balance = 1_000_000_000_000;
pub const TEN_UNITS: Balance = 10_000_000_000_000;
const DOT_FEE_WHEN_TRANSFER_TO_RELAY: u128 = 421434140; //This fee will taken to transfer assets(Polkadot) from sovereign parachain account to destination user account;

#[test]
fn transfer_dot_from_relay_chain_to_pendulum() {
	MockNet::reset();

	let transfer_amount: Balance = units(20);
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

			Box::new(Parachain(2094).into()),
			Box::new(
				Junction::AccountId32 { network: None, id: ALICE }.into_location()
				.into_versioned()
			),
			Box::new((Here, transfer_amount).into()),
			0
		));
	});

	PendulumParachain::execute_with(|| {
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
			orml_tokens_before + transfer_amount - DOT_FEE_WHEN_TRANSFER_TO_PARACHAIN
		);
	});
}

#[test]
fn transfer_dot_from_pendulum_to_relay_chain() {
	MockNet::reset();

	let transfer_dot_amount: Balance = units(10);

	let expected_base_balance = units(100);
	Relay::execute_with(|| {
		let before_bob_free_balance = polkadot_runtime::Balances::free_balance(&BOB.into());
		assert_eq!(before_bob_free_balance, expected_base_balance);
	});

	PendulumParachain::execute_with(|| {
		assert_ok!(pendulum_runtime::XTokens::transfer(
			pendulum_runtime::RuntimeOrigin::signed(BOB.into()),
			pendulum_runtime::PendulumCurrencyId::XCM(0),
			transfer_dot_amount,
			Box::new(
				MultiLocation { parents: 1, interior: X1(AccountId32 { network: None, id: BOB }) }
					.into()
			),
			WeightLimit::Limited(4_000_000_000.into()),
		));
	});

	PendulumParachain::execute_with(|| {
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
		assert_eq!(
			after_bob_free_balance,
			expected_base_balance + transfer_dot_amount - DOT_FEE_WHEN_TRANSFER_TO_RELAY
		);
	});
}

//pendulum_runtime::PendulumCurrencyId::XCM(1) is the representation of USDT from Statemint on Pendulum chain.
//The asset id for USDT on Statemint is 1984. and pendulum support only this asset id to recive it on chain.
//we are going to execute XCM call to sent incorrect Asset Id and expect to see cumulus_pallet_xcmp_queue::Event::Fail event with an error FailedToTransactAsset.
//we what to be sure that the initial USDT balance for BOB is the same after XCM call from statemint when we tried to send wrong ASSET_ID from system parachain.
#[test]
fn statemint_transfer_incorrect_asset_to_pendulum_should_fails() {
	let para_2094: AccountId = Sibling::from(2094).into_account_truncating();

	let extected_base_usdt_balance = 0;
	PendulumParachain::execute_with(|| {
		assert_eq!(
			pendulum_runtime::Tokens::balance(
				pendulum_runtime::PendulumCurrencyId::XCM(1),
				&BOB.into()
			),
			extected_base_usdt_balance
		);
	});

	Statemint::execute_with(|| {
		use statemint_runtime::*;

		let origin = RuntimeOrigin::signed(ALICE.into());
		Balances::make_free_balance_be(&ALICE.into(), TEN_UNITS);
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
			Box::new(Junction::AccountId32 { id: BOB, network: None }.into()),
			Box::new(
				(X2(PalletInstance(50), GeneralIndex(INCORRECT_ASSET_ID as u128)), TEN_UNITS)
					.into()
			),
			0,
			WeightLimit::Unlimited
		));

		assert_eq!(990 * UNIT, Assets::balance(INCORRECT_ASSET_ID, &AccountId::from(ALICE)));
		assert_eq!(0, Assets::balance(INCORRECT_ASSET_ID, &AccountId::from(BOB)));

		assert_eq!(TEN_UNITS, Assets::balance(INCORRECT_ASSET_ID, &para_2094));
		// the DOT balance of sibling parachain sovereign account is not changed
		assert_eq!(UNIT, Balances::free_balance(&para_2094));
	});

	// Rerun the Statemint::execute to actually send the egress message via XCM
	Statemint::execute_with(|| {});

	PendulumParachain::execute_with(|| {
		//println!("all events: {:#?}",System::events());


		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Fail {
				message_hash: _,
				error: xcm::v3::Error::FailedToTransactAsset(..),
				weight: _
			})
		)));
	});

	PendulumParachain::execute_with(|| {
		assert_eq!(
			pendulum_runtime::Tokens::balance(
				pendulum_runtime::PendulumCurrencyId::XCM(1),
				&BOB.into()
			),
			extected_base_usdt_balance
		);
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
		Balances::make_free_balance_be(&ALICE.into(), TEN_UNITS);
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
			Box::new(Junction::AccountId32 { id: BOB, network: None }.into()),
			Box::new((X2(PalletInstance(50), GeneralIndex(ASSET_ID as u128)), TEN_UNITS).into()),
			0,
			WeightLimit::Unlimited
		));

		assert_eq!(990 * UNIT, Assets::balance(ASSET_ID, &AccountId::from(ALICE)));
		assert_eq!(0, Assets::balance(ASSET_ID, &AccountId::from(BOB)));

		assert_eq!(TEN_UNITS, Assets::balance(ASSET_ID, &para_2094));
		// the DOT balance of sibling parachain sovereign account is not changed
		assert_eq!(UNIT, Balances::free_balance(&para_2094));
	});

	// Rerun the Statemint::execute to actually send the egress message via XCM
	Statemint::execute_with(|| {});

	PendulumParachain::execute_with(|| {
		// for i in System::events().iter() {
		// 	println!(" Pendulum_runtime {:?}", i);
		// }

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
			TEN_UNITS
		);
	});
}

#[test]
fn statemint_transfer_asset_to_statemint() {
	//first we need to set up USDT balance on pendulum chain before to start transfer it back.
	statemint_transfer_asset_to_pendulum();

	Statemint::execute_with(|| {});

	PendulumParachain::execute_with(|| {
		assert_eq!(TEN_UNITS, Tokens::balance(PendulumCurrencyId::XCM(1), &AccountId::from(BOB)));
		// ensure sender has enough PEN balance to be charged as fee
		assert_ok!(Balances::mint_into(&AccountId::from(BOB), TEN_UNITS));

		assert_ok!(XTokens::transfer(
			RuntimeOrigin::signed(BOB.into()),
			PendulumCurrencyId::XCM(1),
			UNIT * 1,
			Box::new(
				MultiLocation::new(
					1,
					X2(Parachain(1000), Junction::AccountId32 { network: Some(NetworkId::Polkadot), id: BOB.into() })
				)
				.into()
			),
			WeightLimit::Limited(Weight::from_parts(10_000_000_000, 0)),
		));

		assert_eq!(
			TEN_UNITS - 1 * UNIT, //inital balance - one unit
			Tokens::balance(PendulumCurrencyId::XCM(1), &AccountId::from(BOB))
		);

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. })
		)));
	});

	Statemint::execute_with(|| {
		use statemint_runtime::*;

		// https://github.com/paritytech/cumulus/pull/1278 support using self sufficient asset
		// for paying xcm execution fee on Statemint.
		assert_eq!(990_000_000_000, Assets::balance(ASSET_ID, &AccountId::from(BOB)));
	});
}
