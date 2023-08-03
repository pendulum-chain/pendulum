use frame_support::assert_ok;
use frame_support::traits::Currency;
use frame_support::traits::fungibles::Inspect;
use polkadot_core_primitives::{AccountId, Balance};
use polkadot_parachain::primitives::Sibling;
use sp_runtime::{traits::AccountIdConversion, MultiAddress};
use xcm::latest::{Junction, Junction::*, Junctions::*, MultiLocation, NetworkId, WeightLimit};

use xcm_emulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, TestExt};
use amplitude_runtime::{System, RuntimeEvent, RuntimeOrigin,Tokens, XTokens};
use crate::AMPLITUDE_ID;
use crate::mock::{ParachainType, para_ext, kusama_relay_ext,TEN_UNITS, UNIT, INCORRECT_ASSET_ID, USDT_ASSET_ID};

use crate::setup::{ALICE, BOB, units};

const KSM_FEE_WHEN_TRANSFER_TO_PARACHAIN: Balance = 3200000000;

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
	pub struct StatemineParachain {
		Runtime = statemine_runtime::Runtime,
		RuntimeOrigin = statemine_runtime::RuntimeOrigin,
		XcmpMessageHandler = statemine_runtime::XcmpQueue,
		DmpMessageHandler = statemine_runtime::DmpQueue,
		new_ext = para_ext(ParachainType::Statemine),
	}
}

decl_test_network! {
	pub struct KusamaMockNet {
		relay_chain = KusamaRelay,
		parachains = vec![
			(1000, StatemineParachain),
			(2124, AmplitudeParachain),
		],
	}
}


#[test]
fn transfer_ksm_from_relay_chain_to_amplitude() {
	KusamaMockNet::reset();

	let transfer_amount: Balance = units(20);
	let mut orml_tokens_before = 0;
	AmplitudeParachain::execute_with(|| {
		orml_tokens_before =
			Tokens::balance(amplitude_runtime::CurrencyId::XCM(0), &ALICE.into());
	});

	KusamaRelay::execute_with(|| {
		assert_ok!(kusama_runtime::XcmPallet::reserve_transfer_assets(
			kusama_runtime::RuntimeOrigin::signed(ALICE.into()),
			Box::new(Parachain(AMPLITUDE_ID).into_versioned()),
			Box::new(
				Junction::AccountId32 { network: None, id: ALICE }
					.into_location()
					.into_versioned()
			),
			Box::new((Here, transfer_amount).into()),
			0
		));
	});

	AmplitudeParachain::execute_with(|| {
		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Tokens(orml_tokens::Event::Deposited { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::DmpQueue(cumulus_pallet_dmp_queue::Event::ExecutedDownward { .. })
		)));
	});

	AmplitudeParachain::execute_with(|| {
		assert_eq!(
			Tokens::balance(amplitude_runtime::CurrencyId::XCM(0), &ALICE.into()),
			orml_tokens_before + transfer_amount - KSM_FEE_WHEN_TRANSFER_TO_PARACHAIN
		);
	});
}


#[test]
fn transfer_ksm_from_amplitude_to_relay_chain() {
	KusamaMockNet::reset();

	let transfer_ksm_amount: Balance = units(10);

	let expected_base_balance = units(100);
	KusamaRelay::execute_with(|| {
		let before_bob_free_balance = kusama_runtime::Balances::free_balance(&BOB.into());
		assert_eq!(before_bob_free_balance, expected_base_balance);
	});

	AmplitudeParachain::execute_with(|| {
		assert_ok!(amplitude_runtime::XTokens::transfer(
			amplitude_runtime::RuntimeOrigin::signed(BOB.into()),
			amplitude_runtime::CurrencyId::XCM(0),
			transfer_ksm_amount,
			Box::new(
				MultiLocation { parents: 1, interior: X1(AccountId32 { network: None, id: BOB }) }
					.into()
			),
			WeightLimit::Unlimited
		));
	});

	AmplitudeParachain::execute_with(|| {
		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Tokens(orml_tokens::Event::Withdrawn { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XTokens(orml_xtokens::Event::TransferredMultiAssets { .. })
		)));
	});

	KusamaRelay::execute_with(|| {
		use kusama_runtime::{RuntimeEvent, System};

		let events = System::events();
		assert_eq!(events.len(), 3);

		let withdrawn_balance = match &events[0].event {
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who: _, amount }) => amount,
			other => panic!("wrong event: {:#?}", other),
		};

		let deposited_balance = match &events[1].event {
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { who: _, amount }) => amount,
			other => panic!("wrong event: {:#?}", other),
		};

		match &events[2].event {
			RuntimeEvent::Ump(polkadot_runtime_parachains::ump::Event::ExecutedUpward(..)) =>
				assert!(true),
			other => panic!("wrong event: {:#?}", other),
		};

		//This fee will taken to transfer assets(Kusama) from sovereign parachain account to destination user account;
		let ksm_fee_when_transferring_to_relay_chain = withdrawn_balance - deposited_balance;

		let after_bob_free_balance = kusama_runtime::Balances::free_balance(&BOB.into());
		assert_eq!(
			after_bob_free_balance,
			expected_base_balance + transfer_ksm_amount - ksm_fee_when_transferring_to_relay_chain
		);
	});
}

//kusama_runtime::CurrencyId::XCM(1) is the representation of USDT from Statemine on Kusama chain.
//The asset id for USDT on Statemine is 1984. and amplitude support only this asset id to recive it on chain.
//we are going to execute XCM call to sent incorrect Asset Id and expect to see cumulus_pallet_xcmp_queue::Event::Fail event with an error FailedToTransactAsset.
//we what to be sure that the initial USDT balance for BOB is the same after XCM call from statemint when we tried to send wrong ASSET_ID from system parachain.
#[test]
fn statemine_transfer_incorrect_asset_to_kusama_should_fail() {
	let amplitude_chain: AccountId = Sibling::from(AMPLITUDE_ID).into_account_truncating();

	let extected_base_usdt_balance = 0;
	AmplitudeParachain::execute_with(|| {
		assert_eq!(
			Tokens::balance(amplitude_runtime::CurrencyId::XCM(1), &BOB.into()),
			extected_base_usdt_balance
		);
	});

	StatemineParachain::execute_with(|| {
		use statemine_runtime::*;

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
		Balances::make_free_balance_be(&amplitude_chain, UNIT);

		assert_ok!(PolkadotXcm::limited_reserve_transfer_assets(
			origin.clone(),
			Box::new(MultiLocation::new(1, X1(Parachain(AMPLITUDE_ID))).into()),
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

		assert_eq!(TEN_UNITS, Assets::balance(INCORRECT_ASSET_ID, &amplitude_chain));
		// the DOT balance of sibling parachain sovereign account is not changed
		assert_eq!(UNIT, Balances::free_balance(&amplitude_chain));
	});

	// Rerun the StatemineParachain::execute to actually send the egress message via XCM
	StatemineParachain::execute_with(|| {});

	AmplitudeParachain::execute_with(|| {
		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Fail {
				message_hash: _,
				error: xcm::v3::Error::FailedToTransactAsset(..),
				weight: _
			})
		)));
	});

	AmplitudeParachain::execute_with(|| {
		assert_eq!(
			Tokens::balance(amplitude_runtime::CurrencyId::XCM(1), &BOB.into()),
			extected_base_usdt_balance
		);
	});
}
