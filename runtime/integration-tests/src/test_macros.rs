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
		use crate::mock::{units, ALICE};
		use frame_support::traits::fungibles::Inspect;
		use polkadot_core_primitives::Balance;
		use xcm::latest::{Junction, Junction::Parachain, Junctions::Here};
		use $para_runtime::CurrencyId;

		$mocknet::reset();
		let transfer_amount: Balance = units(20);
		let mut orml_tokens_before = 0;

		// get ALICE's balance before the transfer
		$parachain::execute_with(|| {
			orml_tokens_before = $para_runtime::Tokens::balance(CurrencyId::XCM(0), &ALICE.into());
		});

		// execute the transfer from relay chain
		$relaychain::execute_with(|| {
			assert_ok!($relay_runtime::XcmPallet::reserve_transfer_assets(
				$relay_runtime::RuntimeOrigin::signed(ALICE.into()),
				Box::new(Parachain($parachain_id).into_versioned()),
				Box::new(
					Junction::AccountId32 { network: None, id: ALICE }
						.into_location()
						.into_versioned()
				),
				Box::new((Here, transfer_amount).into()),
				0
			));
		});

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

		$parachain::execute_with(|| {
			assert_eq!(
				$para_runtime::Tokens::balance(CurrencyId::XCM(0), &ALICE.into()),
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
		$parachain: ident
	) => {{
		use crate::mock::{BOB, units};
		use polkadot_core_primitives::Balance;
		use xcm::latest::{Junction::AccountId32, Junctions::X1, MultiLocation, WeightLimit};
		use xcm_emulator::TestExt;

		$mocknet::reset();
		let transfer_amount: Balance = units(10);
		let expected_base_balance = units(100);

		// get BOB's balance in the relay chain, before the transfer.
		$relaychain::execute_with(|| {
			let before_bob_free_balance = $relay_runtime::Balances::free_balance(&BOB.into());
			assert_eq!(before_bob_free_balance, expected_base_balance);
		});

		// execute th transfer in the parachain.
		$parachain::execute_with(|| {
			assert_ok!($para_runtime::XTokens::transfer(
				$para_runtime::RuntimeOrigin::signed(BOB.into()),
				$para_runtime::CurrencyId::XCM(0),
				transfer_amount,
				Box::new(
					MultiLocation { parents: 1, interior: X1(AccountId32 { network: None, id: BOB }) }
						.into()
				),
				WeightLimit::Unlimited
			));
		});

		// check events in Parachain for proof of transfer
		$parachain::execute_with(|| {
			use $para_runtime::{System, RuntimeEvent};

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::Tokens(orml_tokens::Event::Withdrawn { .. })
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

			//This fee will taken to transfer assets(Polkadot) from sovereign parachain account to destination user account;
			let fee_when_transferring_to_relay_chain = withdrawn_balance - deposited_balance;

			let after_bob_free_balance = Balances::free_balance(&BOB.into());
			assert_eq!(
				after_bob_free_balance,
				expected_base_balance + transfer_amount - fee_when_transferring_to_relay_chain
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
		use crate::mock::{ALICE, BOB, INCORRECT_ASSET_ID, TEN_UNITS, UNIT};
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

		let parachain2_account: AccountId = Sibling::from($parachain2_id).into_account_truncating();

		let expected_base_usdt_balance = 0;
		// make sure the account does not have any usdt.
		$parachain2::execute_with(|| {
			assert_eq!(
				$para2_runtime::Tokens::balance(CurrencyId::XCM(1), &BOB.into()),
				expected_base_usdt_balance
			);
		});

		$parachain1::execute_with(|| {
			use $para1_runtime::*;

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

			// need to have some balance to be able to receive user assets
			Balances::make_free_balance_be(&parachain2_account, UNIT);

			assert_ok!(PolkadotXcm::limited_reserve_transfer_assets(
				origin.clone(),
				Box::new(MultiLocation::new(1, X1(Parachain($parachain2_id))).into()),
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

			assert_eq!(TEN_UNITS, Assets::balance(INCORRECT_ASSET_ID, &parachain2_account));
			// the balance of sibling parachain sovereign account is not changed
			assert_eq!(UNIT, Balances::free_balance(&parachain2_account));
		});

		// Rerun $parachain1 to actually send the egress message via XCM
		$parachain1::execute_with(|| {});

		$parachain2::execute_with(|| {
			use $para2_runtime::{RuntimeEvent, System};
			//most likely this is not emitid because buy execution fails
			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Fail {
					message_hash: _,
					error: xcm::v3::Error::AssetNotFound,
					weight: _
				})
			)));
		});

		$parachain2::execute_with(|| {
			assert_eq!(
				$para2_runtime::Tokens::balance(CurrencyId::XCM(1), &BOB.into()),
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
		use crate::mock::{ALICE, BOB, TEN_UNITS, UNIT};
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

		let parachain2_account: AccountId = Sibling::from($parachain2_id).into_account_truncating();

		$parachain2::execute_with(|| {
			assert_eq!($para2_runtime::Tokens::balance(CurrencyId::XCM(1), &BOB.into()), 0);
		});

		$parachain1::execute_with(|| {
			use $para1_runtime::*;

			let origin = RuntimeOrigin::signed(ALICE.into());
			Balances::make_free_balance_be(&ALICE.into(), TEN_UNITS);
			Balances::make_free_balance_be(&BOB.into(), UNIT);

			// If using non root, create custom asset cost 0.1 Dot
			// We're using force_create here to make sure asset is sufficient.
			assert_ok!(Assets::force_create(
				RuntimeOrigin::root(),
				$para1_asset_id.into(),
				MultiAddress::Id(ALICE.into()),
				true,
				UNIT / 100
			));

			assert_ok!(Assets::mint(
				origin.clone(),
				$para1_asset_id.into(),
				MultiAddress::Id(ALICE.into()),
				1000 * UNIT
			));

			// need to have some KSM to be able to receive user assets
			Balances::make_free_balance_be(&parachain2_account, UNIT);

			assert_ok!(PolkadotXcm::limited_reserve_transfer_assets(
				origin.clone(),
				Box::new(MultiLocation::new(1, X1(Parachain($parachain2_id))).into()),
				Box::new(Junction::AccountId32 { id: BOB, network: None }.into()),
				Box::new(
					(X2(PalletInstance(50), GeneralIndex($para1_asset_id as u128)), TEN_UNITS)
						.into()
				),
				0,
				WeightLimit::Unlimited
			));

			assert_eq!(990 * UNIT, Assets::balance($para1_asset_id, &AccountId::from(ALICE)));
			assert_eq!(0, Assets::balance($para1_asset_id, &AccountId::from(BOB)));

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
				$para2_runtime::Tokens::balance($para2_runtime::CurrencyId::XCM(1), &BOB.into()),
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
		use crate::mock::{BOB, TEN_UNITS, UNIT};
		use frame_support::traits::{fungible::Mutate, fungibles::Inspect};
		use polkadot_core_primitives::AccountId;
		use xcm::latest::{
			Junction, Junction::Parachain, Junctions::X2, MultiLocation, WeightLimit,
		};

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
				Tokens::balance(CurrencyId::XCM(1), &AccountId::from(BOB))
			);
			// ensure sender has enough balance to be charged as fee
			assert_ok!(Balances::mint_into(&AccountId::from(BOB), TEN_UNITS));

			assert_ok!(XTokens::transfer(
				RuntimeOrigin::signed(BOB.into()),
				CurrencyId::XCM(1),
				UNIT * 1,
				Box::new(
					MultiLocation::new(
						1,
						X2(
							Parachain($parachain1_id),
							Junction::AccountId32 { network: Some($network_id), id: BOB.into() }
						)
					)
					.into()
				),
				WeightLimit::Unlimited
			));

			assert_eq!(
				received_amount_after_fee - 1 * UNIT, //initial balance - one unit
				Tokens::balance(CurrencyId::XCM(1), &AccountId::from(BOB))
			);

			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. })
			)));

			for i in System::events().iter() {
				println!("{}: {:?}\n", stringify!($para2_runtime), i);
			}
		});

		$parachain1::execute_with(|| {
			use $para1_runtime::*;

			for i in System::events().iter() {
				println!("{}: {:?}\n", stringify!($para1_runtime), i);
			}

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
					assert_eq!(amount, Assets::balance($para1_asset_id, &AccountId::from(BOB)));
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
		use crate::mock::{ALICE, BOB, UNIT, NATIVE_INITIAL_BALANCE};
		use frame_support::traits::fungibles::Inspect;
		use polkadot_core_primitives::Balance;
		use xcm::latest::{
			Junction, Junction::AccountId32, Junctions::X2, MultiLocation, WeightLimit,
		};
		use orml_traits::MultiCurrency;
		use $parachain1_runtime::CurrencyId as Parachain1CurrencyId;
		use $parachain2_runtime::CurrencyId as Parachain2CurrencyId;

		$mocknet::reset();

		let transfer_amount: Balance = UNIT;
		let asset_location = MultiLocation::new(
			1,
			X2(Junction::Parachain($parachain1_id), Junction::PalletInstance(10)),
		);
		// This is needed in order to have the correct mapping regardless of the XCM sender parachain provided
		// Used for checking BOB's balance
		let para1_native_currency_on_para2 = Parachain2CurrencyId::from($parachain1_id);

		// Get ALICE's balance on parachain1 before the transfer (defined in mock config)
		let native_tokens_before: Balance = NATIVE_INITIAL_BALANCE;

		$parachain1::execute_with(|| {
			assert_eq!(
				$parachain1_runtime::Currencies::free_balance(Parachain1CurrencyId::Native, &ALICE.into()),
				native_tokens_before
			);
		});
		$parachain2::execute_with(|| {
			assert_eq!(
				$parachain2_runtime::Tokens::balance(para1_native_currency_on_para2, &BOB.into()),
				0
			);
		});

		// Execute the transfer from parachain1 to parachain2
		$parachain1::execute_with(|| {
			use $parachain1_runtime::{RuntimeEvent, System, XTokens};

			// Transfer using multilocation
			assert_ok!(XTokens::transfer_multiasset(
				$parachain1_runtime::RuntimeOrigin::signed(ALICE.into()),
				Box::new((asset_location.clone(), transfer_amount).into()),
				Box::new(
					MultiLocation {
						parents: 1,
						interior: X2(
							Junction::Parachain($parachain2_id),
							AccountId32 { network: None, id: BOB }
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

		// Verify BOB's balance on parachain2 after receiving
		// Should increase by the transfer amount
		$parachain2::execute_with(|| {
			use $parachain2_runtime::{RuntimeEvent, System, XTokens};
			for i in System::events().iter() {
				println!("para 2 events {}: {:?}\n", stringify!($para2_runtime), i);
			}
			assert_eq!(
				$parachain2_runtime::Tokens::balance(para1_native_currency_on_para2, &BOB.into()),
				transfer_amount
			);
		});

		// Verify ALICE's balance on parachain1 after transfer
		$parachain1::execute_with(|| {
			assert_eq!(
				$parachain1_runtime::Currencies::free_balance(Parachain1CurrencyId::Native, &ALICE.into()),
				native_tokens_before - transfer_amount
			);
		});

		// Send same amount back to ALICE on parachain1
		$parachain2::execute_with(|| {
			use $parachain2_runtime::{RuntimeEvent, System, XTokens};

			// Transfer using the same multilocation
			assert_ok!(XTokens::transfer_multiasset(
				$parachain2_runtime::RuntimeOrigin::signed(BOB.into()),
				Box::new((asset_location.clone(), transfer_amount).into()),
				Box::new(
					MultiLocation {
						parents: 1,
						interior: X2(
							Junction::Parachain($parachain1_id),
							AccountId32 { network: None, id: ALICE }
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
				$parachain2_runtime::Tokens::balance(para1_native_currency_on_para2, &BOB.into()),
				0
			);
		});

		// Verify ALICE's balance on parachain1 after receiving
		// Should become the same amount as initial balance before both transfers
		$parachain1::execute_with(|| {
			use $parachain1_runtime::{System, Treasury};
			for i in System::events().iter() {
				println!("para 1 events {}: {:?}\n", stringify!($para2_runtime), i);
			}
			assert_eq!(
				$parachain1_runtime::Currencies::free_balance(Parachain1CurrencyId::Native, &ALICE.into()),
				native_tokens_before - $tx_fee
			);

			assert_eq!(
				$parachain1_runtime::Currencies::free_balance(Parachain1CurrencyId::Native, &Treasury::account_id()),
				$tx_fee
			);
		});
	}};
}

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
		use crate::mock::{units, ALICE};
		use polkadot_core_primitives::Balance;
		use xcm::latest::{
			Junction, Junction::{ GeneralKey, PalletInstance}, Junctions::{X1,X2, X3}, MultiLocation, WeightLimit,
		};
		use pendulum_runtime::assets::xcm_assets;
		use orml_traits::MultiCurrency;

		use $parachain2_runtime::CurrencyId as Parachain2CurrencyId;

		$mocknet::reset();

		let transfer_amount: Balance = units(100);
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
			use $parachain2_runtime::{XTokens, Tokens,RuntimeOrigin, System};

			assert_ok!(Tokens::set_balance(RuntimeOrigin::root().into(), ALICE.clone().into(), Parachain2CurrencyId::Token,transfer_amount, 0));

			// We must ensure that the destination Multilocation is of the structure
			// the intercept excepts so it calls automation pallet

			// TODO replace instance 99 with automation pallet index when added
			assert_ok!(XTokens::transfer(
				$parachain2_runtime::RuntimeOrigin::signed(ALICE.into()),
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
			use $parachain1_runtime::{RuntimeEvent, System, Tokens, Treasury};
			// given the configuration in pendulum's xcm_config, we expect the callback (in this case a Remark)
			// to be executed
			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::System(frame_system::Event::Remarked { .. })
			)));

			assert_eq!(
				$parachain1_runtime::Currencies::free_balance(xcm_assets::MOONBEAM_BRZ_id(), &Treasury::account_id()),
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
