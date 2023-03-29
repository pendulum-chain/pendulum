// mod parachain;
// mod relay_chain;

use polkadot_parachain::primitives::Id as ParaId;
use sp_runtime::traits::AccountIdConversion;
use xcm_simulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain};

pub const ALICE: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([0u8; 32]);
pub const INITIAL_BALANCE: u128 = 1_000_000_000;

decl_test_parachain! {
	pub struct PendulumParachain {
		Runtime = pendulum_runtime::Runtime,
		XcmpMessageHandler = pendulum_runtime::XcmpQueue,
		DmpMessageHandler = pendulum_runtime::DmpQueue,
		new_ext = para_ext(1),
	}
}

decl_test_relay_chain! {
	pub struct Relay {
		Runtime = polkadot_runtime::Runtime,
		XcmConfig = polkadot_runtime::xcm_config::XcmConfig,
		new_ext = relay_ext(),
	}
}

decl_test_network! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(1, PendulumParachain),
			// (2, Statemint),
		],
	}
}

// pub fn para_account_id(id: u32) -> relay_chain::AccountId {
// 	ParaId::from(id).into_account_truncating()
// }

pub fn para_ext(para_id: u32) -> sp_io::TestExternalities {
	use pendulum_runtime::{Runtime, System};

	let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();

	pallet_balances::GenesisConfig::<Runtime> { balances: vec![(ALICE, INITIAL_BALANCE)] }
		.assimilate_storage(&mut t)
		.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		// MsgQueue::set_para_id(para_id.into());
	});
	ext
}

pub fn relay_ext() -> sp_io::TestExternalities {
	// use relay_chain::{Runtime, System};
	use polkadot_runtime::{Runtime, System};

	let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();

	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
            // (ALICE, INITIAL_BALANCE), (para_account_id(1), INITIAL_BALANCE)
            ],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

// pub type RelayChainPalletXcm = pallet_xcm::Pallet<relay_chain::Runtime>;
// pub type ParachainPalletXcm = pallet_xcm::Pallet<parachain::Runtime>;

#[cfg(test)]
mod tests {
    #[test]
	fn dmp() {}
}

// #[cfg(test)]
// mod tests {
// 	use super::*;

// 	use codec::Encode;
// 	use frame_support::assert_ok;
// 	use xcm::latest::prelude::*;
// 	use xcm_simulator::TestExt;

// 	// Helper function for forming buy execution message
// 	fn buy_execution<C>(fees: impl Into<MultiAsset>) -> Instruction<C> {
// 		BuyExecution { fees: fees.into(), weight_limit: Unlimited }
// 	}

// 	#[test]
// 	fn dmp() {
// 		MockNet::reset();

// 		let remark = parachain::RuntimeCall::System(
// 			frame_system::Call::<parachain::Runtime>::remark_with_event { remark: vec![1, 2, 3] },
// 		);
// 		Relay::execute_with(|| {
// 			assert_ok!(RelayChainPalletXcm::send_xcm(
// 				Here,
// 				Parachain(1),
// 				Xcm(vec![Transact {
// 					origin_type: OriginKind::SovereignAccount,
// 					require_weight_at_most: INITIAL_BALANCE as u64,
// 					call: remark.encode().into(),
// 				}]),
// 			));
// 		});

// 		ParaA::execute_with(|| {
// 			use parachain::{RuntimeEvent, System};
// 			assert!(System::events().iter().any(|r| matches!(
// 				r.event,
// 				RuntimeEvent::System(frame_system::Event::Remarked { .. })
// 			)));
// 		});
// 	}

// 	#[test]
// 	fn ump() {
// 		MockNet::reset();

// 		let remark = relay_chain::RuntimeCall::System(
// 			frame_system::Call::<relay_chain::Runtime>::remark_with_event { remark: vec![1, 2, 3] },
// 		);
// 		ParaA::execute_with(|| {
// 			assert_ok!(ParachainPalletXcm::send_xcm(
// 				Here,
// 				Parent,
// 				Xcm(vec![Transact {
// 					origin_type: OriginKind::SovereignAccount,
// 					require_weight_at_most: INITIAL_BALANCE as u64,
// 					call: remark.encode().into(),
// 				}]),
// 			));
// 		});

// 		Relay::execute_with(|| {
// 			use relay_chain::{RuntimeEvent, System};
// 			assert!(System::events().iter().any(|r| matches!(
// 				r.event,
// 				RuntimeEvent::System(frame_system::Event::Remarked { .. })
// 			)));
// 		});
// 	}

// 	#[test]
// 	fn xcmp() {
// 		MockNet::reset();

// 		let remark = parachain::RuntimeCall::System(
// 			frame_system::Call::<parachain::Runtime>::remark_with_event { remark: vec![1, 2, 3] },
// 		);
// 		ParaA::execute_with(|| {
// 			assert_ok!(ParachainPalletXcm::send_xcm(
// 				Here,
// 				(Parent, Parachain(2)),
// 				Xcm(vec![Transact {
// 					origin_type: OriginKind::SovereignAccount,
// 					require_weight_at_most: INITIAL_BALANCE as u64,
// 					call: remark.encode().into(),
// 				}]),
// 			));
// 		});

// 		ParaB::execute_with(|| {
// 			use parachain::{RuntimeEvent, System};
// 			assert!(System::events().iter().any(|r| matches!(
// 				r.event,
// 				RuntimeEvent::System(frame_system::Event::Remarked { .. })
// 			)));
// 		});
// 	}

// 	#[test]
// 	fn reserve_transfer() {
// 		MockNet::reset();

// 		let withdraw_amount = 123;

// 		Relay::execute_with(|| {
// 			assert_ok!(RelayChainPalletXcm::reserve_transfer_assets(
// 				relay_chain::RuntimeOrigin::signed(ALICE),
// 				Box::new(X1(Parachain(1)).into().into()),
// 				Box::new(X1(AccountId32 { network: Any, id: ALICE.into() }).into().into()),
// 				Box::new((Here, withdraw_amount).into()),
// 				0,
// 			));
// 			assert_eq!(
// 				parachain::Balances::free_balance(&para_account_id(1)),
// 				INITIAL_BALANCE + withdraw_amount
// 			);
// 		});

// 		ParaA::execute_with(|| {
// 			// free execution, full amount received
// 			assert_eq!(
// 				pallet_balances::Pallet::<parachain::Runtime>::free_balance(&ALICE),
// 				INITIAL_BALANCE + withdraw_amount
// 			);
// 		});
// 	}

// 	/// Scenario:
// 	/// A parachain transfers funds on the relay chain to another parachain account.
// 	///
// 	/// Asserts that the parachain accounts are updated as expected.
// 	#[test]
// 	fn withdraw_and_deposit() {
// 		MockNet::reset();

// 		let send_amount = 10;

// 		ParaA::execute_with(|| {
// 			let message = Xcm(vec![
// 				WithdrawAsset((Here, send_amount).into()),
// 				buy_execution((Here, send_amount)),
// 				DepositAsset {
// 					assets: All.into(),
// 					max_assets: 1,
// 					beneficiary: Parachain(2).into(),
// 				},
// 			]);
// 			// Send withdraw and deposit
// 			assert_ok!(ParachainPalletXcm::send_xcm(Here, Parent, message.clone()));
// 		});

// 		Relay::execute_with(|| {
// 			assert_eq!(
// 				relay_chain::Balances::free_balance(para_account_id(1)),
// 				INITIAL_BALANCE - send_amount
// 			);
// 			assert_eq!(relay_chain::Balances::free_balance(para_account_id(2)), send_amount);
// 		});
// 	}

// 	/// Scenario:
// 	/// A parachain wants to be notified that a transfer worked correctly.
// 	/// It sends a `QueryHolding` after the deposit to get notified on success.
// 	///
// 	/// Asserts that the balances are updated correctly and the expected XCM is sent.
// 	#[test]
// 	fn query_holding() {
// 		MockNet::reset();

// 		let send_amount = 10;
// 		let query_id_set = 1234;

// 		// Send a message which fully succeeds on the relay chain
// 		ParaA::execute_with(|| {
// 			let message = Xcm(vec![
// 				WithdrawAsset((Here, send_amount).into()),
// 				buy_execution((Here, send_amount)),
// 				DepositAsset {
// 					assets: All.into(),
// 					max_assets: 1,
// 					beneficiary: Parachain(2).into(),
// 				},
// 				QueryHolding {
// 					query_id: query_id_set,
// 					dest: Parachain(1).into(),
// 					assets: All.into(),
// 					max_response_weight: 1_000_000_000,
// 				},
// 			]);
// 			// Send withdraw and deposit with query holding
// 			assert_ok!(ParachainPalletXcm::send_xcm(Here, Parent, message.clone(),));
// 		});

// 		// Check that transfer was executed
// 		Relay::execute_with(|| {
// 			// Withdraw executed
// 			assert_eq!(
// 				relay_chain::Balances::free_balance(para_account_id(1)),
// 				INITIAL_BALANCE - send_amount
// 			);
// 			// Deposit executed
// 			assert_eq!(relay_chain::Balances::free_balance(para_account_id(2)), send_amount);
// 		});

// 		// Check that QueryResponse message was received
// 		ParaA::execute_with(|| {
// 			assert_eq!(
// 				parachain::MsgQueue::received_dmp(),
// 				vec![Xcm(vec![QueryResponse {
// 					query_id: query_id_set,
// 					response: Response::Assets(MultiAssets::new()),
// 					max_weight: 1_000_000_000,
// 				}])],
// 			);
// 		});
// 	}
// }
