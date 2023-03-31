// mod parachain;
// mod relay_chain;

use frame_support::traits::GenesisBuild;
use pendulum_runtime::{PendulumCurrencyId, Runtime, System};
use polkadot_core_primitives::{AccountId, Balance, BlockNumber};
use polkadot_parachain::primitives::Id as ParaId;
use polkadot_primitives::v2::{MAX_CODE_SIZE, MAX_POV_SIZE};
use polkadot_runtime_parachains::configuration::HostConfiguration;
use sp_runtime::traits::AccountIdConversion;
use xcm_simulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, Weight};

// pub const ALICE: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([0u8; 32]);
pub const ALICE: [u8; 32] = [4u8; 32];
pub const INITIAL_BALANCE: u128 = 1_000_000_000;

decl_test_parachain! {
	pub struct PendulumParachain {
		Runtime = pendulum_runtime::Runtime,
		XcmpMessageHandler = pendulum_runtime::XcmpQueue,
		DmpMessageHandler = pendulum_runtime::DmpQueue,
		new_ext = para_ext_pendulum(1234),
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
			(1234, PendulumParachain),
			// (2, Statemint),
		],
	}
}

pub fn para_account_id(id: u32) -> polkadot_core_primitives::AccountId {
	ParaId::from(id).into_account_truncating()
}

pub fn relay_ext() -> sp_io::TestExternalities {
	use polkadot_runtime::{Runtime, System};
	let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();
	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![(AccountId::from(ALICE), dot(2002))],
	}
	.assimilate_storage(&mut t)
	.unwrap();
	polkadot_runtime_parachains::configuration::GenesisConfig::<Runtime> {
		config: default_parachains_host_configuration(),
	}
	.assimilate_storage(&mut t)
	.unwrap();
	<pallet_xcm::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
		&pallet_xcm::GenesisConfig { safe_xcm_version: Some(2) },
		&mut t,
	)
	.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub struct ExtBuilderPendulum {
	balances: Vec<(AccountId, PendulumCurrencyId, Balance)>,
	parachain_id: u32,
}
impl Default for ExtBuilderPendulum {
	fn default() -> Self {
		Self { balances: vec![], parachain_id: 1234 }
	}
}

pub fn para_ext_pendulum(parachain_id: u32) -> sp_io::TestExternalities {
	ExtBuilderPendulum::default()
		.balances(vec![])
		.parachain_id(parachain_id)
		.build()
}

impl ExtBuilderPendulum {
	pub fn balances(mut self, balances: Vec<(AccountId, PendulumCurrencyId, Balance)>) -> Self {
		self.balances = balances;
		self
	}
	pub fn parachain_id(mut self, parachain_id: u32) -> Self {
		self.parachain_id = parachain_id;
		self
	}
	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();
		// let native_currency_id = Pendulum_runtime::Native::get();
		pallet_balances::GenesisConfig::<Runtime> {
			balances: vec![(AccountId::from(ALICE), INITIAL_BALANCE)],
		}
		.assimilate_storage(&mut t)
		.unwrap();
		<parachain_info::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
			&parachain_info::GenesisConfig { parachain_id: self.parachain_id.into() },
			&mut t,
		)
		.unwrap();
		<pallet_xcm::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
			&pallet_xcm::GenesisConfig { safe_xcm_version: Some(2) },
			&mut t,
		)
		.unwrap();
		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}

fn default_parachains_host_configuration() -> HostConfiguration<BlockNumber> {
	HostConfiguration {
		minimum_validation_upgrade_delay: 5,
		validation_upgrade_cooldown: 5u32,
		validation_upgrade_delay: 5,
		code_retention_period: 1200,
		max_code_size: MAX_CODE_SIZE,
		max_pov_size: MAX_POV_SIZE,
		max_head_data_size: 32 * 1024,
		group_rotation_frequency: 20,
		chain_availability_period: 4,
		thread_availability_period: 4,
		max_upward_queue_count: 8,
		max_upward_queue_size: 1024 * 1024,
		max_downward_message_size: 1024,
		ump_service_total_weight: Weight::from_ref_time(4 * 1_000_000_000),
		max_upward_message_size: 50 * 1024,
		max_upward_message_num_per_candidate: 5,
		hrmp_sender_deposit: 0,
		hrmp_recipient_deposit: 0,
		hrmp_channel_max_capacity: 8,
		hrmp_channel_max_total_size: 8 * 1024,
		hrmp_max_parachain_inbound_channels: 4,
		hrmp_max_parathread_inbound_channels: 4,
		hrmp_channel_max_message_size: 1024 * 1024,
		hrmp_max_parachain_outbound_channels: 4,
		hrmp_max_parathread_outbound_channels: 4,
		hrmp_max_message_num_per_candidate: 5,
		dispute_period: 6,
		no_show_slots: 2,
		n_delay_tranches: 25,
		needed_approvals: 2,
		relay_vrf_modulo_samples: 2,
		zeroth_delay_tranche_width: 0,
		..Default::default()
	}
}

pub fn dot(amount: Balance) -> Balance {
	amount * one(18)
}

pub fn one(decimals: u32) -> Balance {
	10u128.saturating_pow(decimals.into())
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
