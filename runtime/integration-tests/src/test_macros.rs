use parachain_info;
use xcm_emulator::ParaId;


macro_rules! transfer_20_relay_token_from_relay_chain_to_parachain {
	(
		$mocknet:ident,
		$relay_runtime:ident,
		$relaychain:ident,
		$para_runtime:ident,
		$parachain: ident,
		$parachain_id:ident,
		$tx_fee:ident
	) => {{
		use xcm_emulator::{Network, TestExt, Chain, RelayChain};
		use crate::mock::{units};
		use frame_support::traits::fungibles::Inspect;
		use polkadot_core_primitives::Balance;
		use xcm::latest::{Junction, Junction::Parachain, Junctions::{X1, Here}, MultiLocation, WeightLimit};
		use $para_runtime::CurrencyId;

		use integration_tests_common::constants::accounts;
		let alice_account_id = accounts::init_balances()[0].clone();

		$mocknet::reset();
		let transfer_amount: Balance = units(100);
		let mut orml_tokens_before = 0;

		// get ALICE's balance before the transfer
		$parachain::execute_with(|| {
			orml_tokens_before = $para_runtime::Tokens::balance(CurrencyId::XCM(0), &alice_account_id.clone().into());
		});

		let expected_base_balance = 40960000000000;

		// get ALICE's balance in the relay chain, before the transfer.
		$relaychain::execute_with(|| {
			let before_alice_free_balance = $relay_runtime::Balances::free_balance(&alice_account_id.clone().into());
			assert_eq!(before_alice_free_balance, expected_base_balance);
		});


		println!("executing relay transfer");
		// execute the transfer from relay chain
		$relaychain::execute_with(|| {
			use $relay_runtime::{RuntimeEvent, System};

			assert_ok!($relay_runtime::XcmPallet::reserve_transfer_assets(
				$relay_runtime::RuntimeOrigin::signed(alice_account_id.clone().into()),
				Box::new(Parachain($parachain_id).into()),
				Box::new(
					Junction::AccountId32 { network: None, id: alice_account_id.clone().into() }
						.into_location()
						.into_versioned()
				),
				Box::new((Here, transfer_amount).into()),
				0
			));

			//relaychain::assert_xcm_pallet_sent();

		});

		println!("checking deposit event on receiver chain");
		// a "Deposited" event occurred is proof that the transfer was successful
		$parachain::execute_with(|| {
			use $para_runtime::{RuntimeEvent, System};

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::Tokens(orml_tokens::Event::Deposited { .. })
			)));

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::DmpQueue(cumulus_pallet_dmp_queue::Event::ExecutedDownward { .. })
			)));
		});

		println!("checking balance");
		$parachain::execute_with(|| {
			assert_eq!(
				$para_runtime::Tokens::balance(CurrencyId::XCM(0), &alice_account_id.clone().into()),
				orml_tokens_before + transfer_amount - $tx_fee
			);
		});
	}};
}

macro_rules! transfer_10_relay_token_from_parachain_to_relay_chain {(
		$mocknet:ident,
		$relay_runtime:ident,
		$relaychain:ident,
		$para_runtime:ident,
		$parachain: ident,
		$parachain_id:ident,
		$tx_fee:ident
	) => {{
		use xcm_emulator::{Network, TestExt, Chain};
		use crate::mock::{units};
		use polkadot_core_primitives::Balance;
		use xcm::latest::{Junction::AccountId32, Junctions::X1, MultiLocation, WeightLimit};

		use integration_tests_common::constants::accounts;
		let bob_account_id = accounts::init_balances()[1].clone();

		transfer_20_relay_token_from_relay_chain_to_parachain!(
			$mocknet,
			$relay_runtime,
			$relaychain,
			$para_runtime,
			$parachain,
			$parachain_id,
			$tx_fee
		);

		let transfer_amount: Balance = units(90);
		let expected_base_balance = 40960000000000;

		// get BOB's balance in the relay chain, before the transfer.
		$relaychain::execute_with(|| {
			let before_bob_free_balance = $relay_runtime::Balances::free_balance(&bob_account_id.clone().into());
			assert_eq!(before_bob_free_balance, expected_base_balance);
		});

		// execute th transfer in the parachain.
		$parachain::execute_with(|| {
			use $para_runtime::{System, RuntimeEvent};
			assert_ok!($para_runtime::XTokens::transfer(
				$para_runtime::RuntimeOrigin::signed(bob_account_id.clone().into()),
				$para_runtime::CurrencyId::XCM(0),
				transfer_amount,
				Box::new(
					MultiLocation { parents: 1, interior: X1(AccountId32 { network: None, id: bob_account_id.clone().into() }) }
						.into()
				),
				WeightLimit::Unlimited
			));

			use orml_tokens::Event;

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::Tokens(Event::Withdrawn { .. })
			)));

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::XTokens(orml_xtokens::Event::TransferredMultiAssets { .. })
			)));
		});

		// check events in the Relaychain for proof of transfer
		$relaychain::execute_with(|| {
			use $relay_runtime::{RuntimeEvent, System, Balances};

			let events = System::events();
			for event_record in System::events() {
    			println!("relay {:?}", event_record.event);
			}

			let withdrawn_balance = match &events[0].event {
				RuntimeEvent::Balances(pallet_balances::Event::Withdraw { who: _, amount }) => amount,
				other => panic!("wrong event: {:#?}", other),
			};

			let deposited_balance = match &events[1].event {
				RuntimeEvent::Balances(pallet_balances::Event::Deposit { who: _, amount }) => amount,
				other => panic!("wrong event: {:#?}", other),
			};


			//This fee will taken to transfer assets(Polkadot) from sovereign parachain account to destination user account;
			let fee_when_transferring_to_relay_chain = withdrawn_balance - deposited_balance;

			let after_bob_free_balance = Balances::free_balance(&bob_account_id.into());
			assert_eq!(
				after_bob_free_balance,
				expected_base_balance + transfer_amount - fee_when_transferring_to_relay_chain,
				"Incorrect amount received"
			);
		});

	}};
}

// the CurrencyId::XCM(1) is the representation of USDT from Statemint/Statemine on Pendulum/Amplitude chain.
// The asset id for USDT on Statemint/Statemine is 1984, and our chain support only this asset id.
// we are going to execute XCM call to sent incorrect Asset Id and expect to see cumulus_pallet_xcmp_queue::Event::Fail event with an error FailedToTransactAsset.
// we want to be sure that the initial USDT balance for BOB is the same after XCM call from statemint/statemine when we tried to send wrong ASSET_ID.
macro_rules! parachain1_transfer_incorrect_asset_to_parachain2_should_fail {
	(
		$para1_runtime:ident,
		$parachain1:ident,
		$para2_runtime:ident,
		$parachain2: ident,
		$parachain2_id:ident
	) => {{
		use crate::mock::{INCORRECT_ASSET_ID, TEN_UNITS, UNIT};
		use xcm_emulator::{Network, TestExt, Chain};
		use frame_support::traits::{fungibles::Inspect, Currency};
		use polkadot_core_primitives::AccountId;
		use polkadot_parachain::primitives::Sibling;
		use sp_runtime::{traits::AccountIdConversion, MultiAddress};
		use xcm::latest::{
			Junction,
			Junction::{GeneralIndex, PalletInstance, Parachain},
			Junctions::{X1, X2},
			MultiLocation, WeightLimit,
		};
		use integration_tests_common::constants::accounts;
		use $para2_runtime::CurrencyId;

		let parachain2_account: AccountId = Sibling::from($parachain2_id).into_account_truncating();
		let alice_account_id = accounts::init_balances()[0].clone();
		let bob_account_id = accounts::init_balances()[1].clone();

		let expected_base_usdt_balance = 0;
		// make sure the account does not have any usdt.
		$parachain2::execute_with(|| {
			assert_eq!(
				$para2_runtime::Tokens::balance(CurrencyId::XCM(1), &bob_account_id),
				expected_base_usdt_balance
			);
		});

		$parachain1::execute_with(|| {
			use $para1_runtime::*;

			let origin = RuntimeOrigin::signed(alice_account_id.clone());
			Balances::make_free_balance_be(&alice_account_id, TEN_UNITS);
			Balances::make_free_balance_be(&bob_account_id, UNIT);

			// If using non root, create custom asset cost 0.1 Dot
			// We're using force_create here to make sure asset is sufficient.
			assert_ok!(Assets::force_create(
				RuntimeOrigin::root(),
				INCORRECT_ASSET_ID.into(),
				MultiAddress::Id(alice_account_id.clone()),
				true,
				UNIT / 100
			));

			assert_ok!(Assets::mint(
				origin.clone(),
				INCORRECT_ASSET_ID.into(),
				MultiAddress::Id(alice_account_id.clone()),
				1000 * UNIT
			));

			// need to have some balance to be able to receive user assets
			Balances::make_free_balance_be(&parachain2_account, UNIT);

			assert_ok!(PolkadotXcm::limited_reserve_transfer_assets(
				origin.clone(),
				Box::new(MultiLocation::new(1, X1(Parachain($parachain2_id))).into()),
				Box::new(Junction::AccountId32 { id: bob_account_id.clone().into(), network: None }.into()),
				Box::new(
					(X2(PalletInstance(50), GeneralIndex(INCORRECT_ASSET_ID as u128)), TEN_UNITS)
						.into()
				),
				0,
				WeightLimit::Unlimited
			));

			assert_eq!(990 * UNIT, Assets::balance(INCORRECT_ASSET_ID, &alice_account_id));
			assert_eq!(0, Assets::balance(INCORRECT_ASSET_ID, &bob_account_id));

			assert_eq!(TEN_UNITS, Assets::balance(INCORRECT_ASSET_ID, &parachain2_account));
			// the balance of sibling parachain sovereign account is not changed
			assert_eq!(UNIT, Balances::free_balance(&parachain2_account));
		});

		// Rerun $parachain1 to actually send the egress message via XCM
		$parachain1::execute_with(|| {});

		$parachain2::execute_with(|| {
			use $para2_runtime::{RuntimeEvent, System};
			//since the asset registry trader cannot find the fee per second for the asset,
			//it will return TooExpensive error.
			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Fail {
					message_hash: _,
					error: xcm::v3::Error::TooExpensive,
					weight: _, ..
				})
			)));
		});

		$parachain2::execute_with(|| {
			assert_eq!(
				$para2_runtime::Tokens::balance(CurrencyId::XCM(1), &bob_account_id),
				expected_base_usdt_balance
			);
		});
	}};
}

macro_rules! parachain1_transfer_asset_to_parachain2 {
	(
		$para1_runtime:ident,
		$parachain1:ident,
		$para1_asset_id:ident,
		$para2_runtime:ident,
		$parachain2: ident,
		$parachain2_id:ident,
		$tx_fee:ident
	) => {{
		use xcm_emulator::{Network, TestExt, Chain};
		use crate::mock::{ TEN_UNITS, UNIT};
		use frame_support::traits::{fungibles::Inspect, Currency};
		use polkadot_core_primitives::AccountId;
		use polkadot_parachain::primitives::Sibling;
		use sp_runtime::{traits::AccountIdConversion, MultiAddress};
		use xcm::latest::{
			Junction,
			Junction::{GeneralIndex, PalletInstance, Parachain},
			Junctions::{X1, X2},
			MultiLocation, WeightLimit,
		};
		use $para2_runtime::CurrencyId;
		use integration_tests_common::constants::accounts;

		let alice_account_id = accounts::init_balances()[0].clone();
		let bob_account_id = accounts::init_balances()[1].clone();

		let parachain2_account: AccountId = Sibling::from($parachain2_id).into_account_truncating();

		$parachain2::execute_with(|| {
			assert_eq!($para2_runtime::Tokens::balance(CurrencyId::XCM(1), &alice_account_id.clone().into()), 0);
		});

		$parachain1::execute_with(|| {
			use $para1_runtime::*;

			let origin = RuntimeOrigin::signed(alice_account_id.clone().into());
			Balances::make_free_balance_be(&alice_account_id.clone().into(), TEN_UNITS);
			Balances::make_free_balance_be(&bob_account_id.clone().into(), UNIT);

			// If using non root, create custom asset cost 0.1 Dot
			// We're using force_create here to make sure asset is sufficient.
			assert_ok!(Assets::force_create(
				RuntimeOrigin::root(),
				$para1_asset_id.into(),
				MultiAddress::Id(alice_account_id.clone().into()),
				true,
				UNIT / 100
			));

			assert_ok!(Assets::mint(
				origin.clone(),
				$para1_asset_id.into(),
				MultiAddress::Id(alice_account_id.clone().into()),
				1000 * UNIT
			));

			// need to have some KSM to be able to receive user assets
			Balances::make_free_balance_be(&parachain2_account, UNIT);

			assert_ok!(PolkadotXcm::limited_reserve_transfer_assets(
				origin.clone(),
				Box::new(MultiLocation::new(1, X1(Parachain($parachain2_id))).into()),
				Box::new(Junction::AccountId32 { id: bob_account_id.clone().into(), network: None }.into()),
				Box::new(
					(X2(PalletInstance(50), GeneralIndex($para1_asset_id as u128)), TEN_UNITS)
						.into()
				),
				0,
				WeightLimit::Unlimited
			));

			assert_eq!(990 * UNIT, Assets::balance($para1_asset_id, &alice_account_id.clone()));
			assert_eq!(0, Assets::balance($para1_asset_id, &bob_account_id.clone()));

			assert_eq!(TEN_UNITS, Assets::balance($para1_asset_id, &parachain2_account));
			// the balance of sibling parachain sovereign account is not changed
			assert_eq!(UNIT, Balances::free_balance(&parachain2_account));
		});

		// Rerun the StatemintParachain::execute to actually send the egress message via XCM
		$parachain1::execute_with(|| {});

		$parachain2::execute_with(|| {
			use $para2_runtime::{RuntimeEvent, System};

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::Tokens(orml_tokens::Event::Endowed { .. })
			)));

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::Tokens(orml_tokens::Event::Deposited { .. })
			)));

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Success { .. })
			)));

			assert_eq!(
				$para2_runtime::Tokens::balance($para2_runtime::CurrencyId::XCM(1), &bob_account_id.clone().into()),
				TEN_UNITS - $tx_fee
			);
		});
	}};
}

macro_rules! parachain1_transfer_asset_to_parachain2_and_back {
	(
		$para1_runtime:ident,
		$parachain1:ident,
		$parachain1_id:ident,
		$para1_asset_id:ident,
		$para2_runtime:ident,
		$parachain2: ident,
		$parachain2_id:ident,
		$network_id: ident,
		$tx_fee: ident
	) => {{
		use xcm_emulator::{Network, TestExt, Chain};
		use crate::mock::{TEN_UNITS, UNIT};
		use frame_support::traits::{fungible::Mutate, fungibles::Inspect};
		use polkadot_core_primitives::AccountId;
		use xcm::latest::{
			Junction, Junction::Parachain, Junctions::X2, MultiLocation, WeightLimit,
		};
		use integration_tests_common::constants::accounts;

		let bob_account_id = accounts::init_balances()[1].clone();

		//first we need to set up USDT balance on pendulum chain before to start transfer it back.
		parachain1_transfer_asset_to_parachain2!(
			$para1_runtime,
			$parachain1,
			$para1_asset_id,
			$para2_runtime,
			$parachain2,
			$parachain2_id,
			$tx_fee
		);

		$parachain1::execute_with(|| {});

		let received_amount_after_fee = TEN_UNITS - $tx_fee;
		$parachain2::execute_with(|| {
			use $para2_runtime::{
				Balances, CurrencyId, RuntimeEvent, RuntimeOrigin, System, Tokens, XTokens,
			};

			assert_eq!(
				received_amount_after_fee,
				Tokens::balance(CurrencyId::XCM(1), &bob_account_id.clone())
			);
			// ensure sender has enough balance to be charged as fee
			assert_ok!(Balances::mint_into(&bob_account_id.clone(), TEN_UNITS));

			assert_ok!(XTokens::transfer(
				RuntimeOrigin::signed(bob_account_id.clone().into()),
				CurrencyId::XCM(1),
				UNIT * 1,
				Box::new(
					MultiLocation::new(
						1,
						X2(
							Parachain($parachain1_id),
							Junction::AccountId32 { network: Some($network_id), id: bob_account_id.clone().into() }
						)
					)
					.into()
				),
				WeightLimit::Unlimited
			));

			assert_eq!(
				received_amount_after_fee - 1 * UNIT, //initial balance - one unit
				Tokens::balance(CurrencyId::XCM(1), &bob_account_id.clone())
			);

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. })
			)));
		});

		$parachain1::execute_with(|| {
			use $para1_runtime::*;


			let events = System::events();
			match &events[events.len() - 2] {
				&frame_system::EventRecord {
					phase: frame_system::Phase::Initialization,
					event:
						RuntimeEvent::Assets(pallet_assets::Event::Issued {
							asset_id: $para1_asset_id,
							owner: _,
							amount,
						}),
					topics: _,
				} => {
					// https://github.com/paritytech/cumulus/pull/1278 support using self sufficient asset
					// for paying xcm execution fee.
					// 990_000_000_000 for Statemint
					// 988_423_297_485 for Statemine
					assert_eq!(amount, Assets::balance($para1_asset_id, &bob_account_id.clone()));
				},
				other => panic!("wrong event: {other:?}"),
			}
		});
	}};
}

macro_rules! transfer_native_token_from_parachain1_to_parachain2_and_back {
	(
        $mocknet:ident,
        $parachain1_runtime:ident,
        $parachain1:ident,
        $parachain2_runtime:ident,
        $parachain2:ident,
        $parachain1_id:ident,
        $parachain2_id:ident,
		$tx_fee:ident
    ) => {{
		use crate::mock::{UNIT, NATIVE_INITIAL_BALANCE, units};
		use frame_support::traits::fungibles::Inspect;
		use polkadot_core_primitives::Balance;
		use xcm::latest::{
			Junction, Junction::AccountId32, Junctions::{X2, X1}, MultiLocation, WeightLimit,
		};
		use xcm_emulator::{TestExt, Network};
		use orml_traits::MultiCurrency;
		use $parachain1_runtime::CurrencyId as Parachain1CurrencyId;
		use $parachain2_runtime::CurrencyId as Parachain2CurrencyId;
		use integration_tests_common::constants::accounts;

		$mocknet::reset();

		let alice_account_id = accounts::init_balances()[0].clone();
		let bob_account_id = accounts::init_balances()[1].clone();

		let transfer_amount: Balance = units(10);
		let asset_location_local_pov =  MultiLocation::new(
			0,
			X1(Junction::PalletInstance(10)),
		);
		let asset_location = MultiLocation::new(
			1,
			X2(Junction::Parachain($parachain1_id), Junction::PalletInstance(10)),
		);
		// This is needed in order to have the correct mapping regardless of the XCM sender parachain provided
		// Used for checking BOB's balance
		let para1_native_currency_on_para2 = Parachain2CurrencyId::from($parachain1_id);

		// Get ALICE's balance on parachain1 before the transfer (defined in mock config)
		let native_tokens_before: Balance = units(1000);
		let mut treasury_native_tokens_before: Balance = 0;



		$parachain1::execute_with(|| {
			use $parachain1_runtime::Treasury;
			assert_eq!(
				$parachain1_runtime::Currencies::free_balance(Parachain1CurrencyId::Native, &alice_account_id),
				native_tokens_before
			);
			treasury_native_tokens_before = $parachain1_runtime::Currencies::free_balance(Parachain1CurrencyId::Native, &Treasury::account_id());
		});
		$parachain2::execute_with(|| {
			assert_eq!(
				$parachain2_runtime::Tokens::balance(para1_native_currency_on_para2, &bob_account_id),
				0
			);
		});

		// Execute the transfer from parachain1 to parachain2
		$parachain1::execute_with(|| {
			use $parachain1_runtime::{RuntimeEvent, System, XTokens};

			// Transfer using multilocation
			assert_ok!(XTokens::transfer_multiasset(
				$parachain1_runtime::RuntimeOrigin::signed(alice_account_id.clone()),
				Box::new((asset_location_local_pov.clone(), transfer_amount).into()),
				Box::new(
					MultiLocation {
						parents: 1,
						interior: X2(
							Junction::Parachain($parachain2_id),
							AccountId32 { network: None, id: bob_account_id.clone().into() }
						)
					}
					.into()
				),
				WeightLimit::Unlimited
			));

			// Alternatively, we should be able to use
			// assert_ok!(XTokens::transfer(
			// 	$parachain1_runtime::RuntimeOrigin::signed(ALICE.into()),
			// 	Parachain1CurrencyId::Native,
			// 	transfer_amount,
			// 	Box::new(
			// 		MultiLocation {
			// 			parents: 1,
			// 			interior: X2(
			// 				Junction::Parachain($parachain2_id),
			// 				AccountId32 { network: None, id: BOB }
			// 			)
			// 		}
			// 		.into()
			// 	),
			// 	WeightLimit::Unlimited
			// ));

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::XTokens(orml_xtokens::Event::TransferredMultiAssets { .. })
			)));
		});

		// Verify BOB's balance on parachain2 after receiving
		// Should increase by the transfer amount
		$parachain2::execute_with(|| {
			assert_eq!(
				$parachain2_runtime::Tokens::balance(para1_native_currency_on_para2, &bob_account_id),
				transfer_amount
			);
		});

		// Verify ALICE's balance on parachain1 after transfer
		$parachain1::execute_with(|| {
			assert_eq!(
				$parachain1_runtime::Currencies::free_balance(Parachain1CurrencyId::Native, &alice_account_id),
				native_tokens_before - transfer_amount
			);
		});

		// Send same amount back to ALICE on parachain1
		$parachain2::execute_with(|| {
			use $parachain2_runtime::{RuntimeEvent, System, XTokens};

			// Transfer using the same multilocation
			assert_ok!(XTokens::transfer_multiasset(
				$parachain2_runtime::RuntimeOrigin::signed(bob_account_id.clone()),
				Box::new((asset_location.clone(), transfer_amount).into()),
				Box::new(
					MultiLocation {
						parents: 1,
						interior: X2(
							Junction::Parachain($parachain1_id),
							AccountId32 { network: None, id: alice_account_id.clone().into() }
						)
					}
					.into()
				),
				WeightLimit::Unlimited
			));

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::XTokens(orml_xtokens::Event::TransferredMultiAssets { .. })
			)));
		});

		// Verify BOB's balance on parachain2 after transfer
		// Should become the same amount as initial balance before both transfers
		$parachain2::execute_with(|| {
			assert_eq!(
				$parachain2_runtime::Tokens::balance(para1_native_currency_on_para2, &bob_account_id),
				0
			);
		});

		// Verify ALICE's balance on parachain1 after receiving
		// Should become the same amount as initial balance before both transfers
		$parachain1::execute_with(|| {
			use $parachain1_runtime::{System, Treasury};
			assert_eq!(
				$parachain1_runtime::Currencies::free_balance(Parachain1CurrencyId::Native, &alice_account_id),
				native_tokens_before - $tx_fee,
				"Sender received incorrect amount when transfer back"
			);

			assert_eq!(
				$parachain1_runtime::Currencies::free_balance(Parachain1CurrencyId::Native, &Treasury::account_id()) - treasury_native_tokens_before,
				$tx_fee,
				"Treasury received incorrect fee when transfer back"
			);
		});
	}};
}

// NOTE this test is only relevant to the pendulum runtime configuration
macro_rules! moonbeam_transfers_token_and_handle_automation {
	(
        $mocknet:ident,
        $parachain1_runtime:ident,
        $parachain1:ident,
        $parachain2_runtime:ident,
        $parachain2:ident,
        $parachain1_id:ident,
        $parachain2_id:ident,
		$expected_fee:ident
    ) => {{
		use crate::{mock::units, definitions::xcm_assets};
		use xcm_emulator::{TestExt, Network};
		use integration_tests_common::constants::accounts;

		use polkadot_core_primitives::Balance;
		use xcm::latest::{
			Junction, Junction::{ GeneralKey, PalletInstance}, Junctions::{X3}, MultiLocation, WeightLimit,
		};

		use orml_traits::MultiCurrency;

		use $parachain1_runtime::CurrencyId as Parachain1CurrencyId;
		use $parachain2_runtime::CurrencyId as Parachain2CurrencyId;

		$mocknet::reset();

		let alice_account_id = accounts::init_balances()[0].clone();

		let transfer_amount: Balance = units(10);
		let mut treasury_balance_before: Balance = 0;
		// get the balance of the treasury before sending the message
		$parachain1::execute_with(|| {
			use $parachain1_runtime::{ PendulumTreasuryAccount};
			treasury_balance_before = $parachain1_runtime::Currencies::free_balance(xcm_assets::MOONBEAM_BRZ_id(), &PendulumTreasuryAccount::get());
		});
		// We mock parachain 2 as beeing moonriver in this case.
		// Sending "Token" variant which is equivalent to BRZ mock token Multilocation
		// in the sibling definition
		$parachain2::execute_with(|| {
			use $parachain2_runtime::{XTokens, Tokens,RuntimeOrigin};

			assert_ok!(Tokens::set_balance(RuntimeOrigin::root().into(), alice_account_id.clone(), Parachain2CurrencyId::Token,transfer_amount, 0));

			// We must ensure that the destination Multilocation is of the structure
			// the intercept excepts so it calls automation pallet

			// TODO replace instance 99 with automation pallet index when added
			assert_ok!(XTokens::transfer(
				$parachain2_runtime::RuntimeOrigin::signed(alice_account_id.clone()),
				Parachain2CurrencyId::Token,
				transfer_amount,
				Box::new(
					MultiLocation::new(
						1,
						X3(
							Junction::Parachain($parachain1_id),
							PalletInstance(99),
							GeneralKey {length:32 , data:[1u8;32]}
						)
					)
					.into()
				),
				WeightLimit::Unlimited
			));
		});

		$parachain1::execute_with(|| {
			use $parachain1_runtime::{RuntimeEvent, System, Treasury};
			// given the configuration in pendulum's xcm_config, we expect the callback (in this case a Remark)
			// to be executed and the treasury to be rewarded with the expected fee
			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::System(frame_system::Event::Remarked { .. })
			)));

			// For parachain 1 (Pendulum) BRZ token is located at index 6
			assert_eq!(
				$parachain1_runtime::Currencies::free_balance(Parachain1CurrencyId::XCM(6), &Treasury::account_id()),
				$expected_fee
			);


		});
	}};
}
// macros defined at the bottom of this file to prevent unresolved imports
pub(super) use moonbeam_transfers_token_and_handle_automation;
pub(super) use parachain1_transfer_asset_to_parachain2;
pub(super) use parachain1_transfer_asset_to_parachain2_and_back;
pub(super) use parachain1_transfer_incorrect_asset_to_parachain2_should_fail;
pub(super) use transfer_10_relay_token_from_parachain_to_relay_chain;
pub(super) use transfer_20_relay_token_from_relay_chain_to_parachain;
pub(super) use transfer_native_token_from_parachain1_to_parachain2_and_back;
