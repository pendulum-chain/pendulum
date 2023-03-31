use frame_support::{
	assert_ok,
	traits::{fungibles::Inspect, GenesisBuild},
};
use pendulum_runtime::{Balances, PendulumCurrencyId, Runtime, System, Tokens};
use polkadot_core_primitives::{AccountId, Balance, BlockNumber};
use polkadot_parachain::primitives::Id as ParaId;
use polkadot_primitives::v2::{MAX_CODE_SIZE, MAX_POV_SIZE};
use polkadot_runtime_parachains::configuration::HostConfiguration;
use sp_runtime::traits::AccountIdConversion;
use xcm::{
	latest::{
		AssetId, Fungibility, Junction, Junction::*, Junctions::*, MultiAsset, MultiLocation,
		NetworkId, WeightLimit,
	},
	v2::{Instruction::WithdrawAsset, Xcm},
	VersionedMultiLocation,
};
const DOT_FEE: Balance = 3200000000;
use xcm_emulator::{
	decl_test_network, decl_test_parachain, decl_test_relay_chain, Junctions, TestExt, Weight,
};

pub const ALICE: [u8; 32] = [4u8; 32];
pub const BOB: [u8; 32] = [5u8; 32];
pub const INITIAL_BALANCE: u128 = 1_000_000_000;

decl_test_relay_chain! {
	pub struct Relay {
		Runtime = polkadot_runtime::Runtime,
		XcmConfig = polkadot_runtime::xcm_config::XcmConfig,
		new_ext = relay_ext(),
	}
}

decl_test_parachain! {
	pub struct PendulumParachain {
		Runtime = pendulum_runtime::Runtime,
		RuntimeOrigin = pendulum_runtime::RuntimeOrigin,
		XcmpMessageHandler = pendulum_runtime::XcmpQueue,
		DmpMessageHandler = pendulum_runtime::DmpQueue,
		new_ext = para_ext_pendulum(2094),
	}
}

decl_test_network! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(2094, PendulumParachain),
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
		balances: vec![
			(AccountId::from(ALICE), dot(100000)),
			(ParaId::from(2094).into_account_truncating(), 10 * dot(100000)),
		],
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
		Self { balances: vec![], parachain_id: 2094 }
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
	amount * one(9)
}

pub fn one(decimals: u32) -> Balance {
	10u128.saturating_pow(decimals.into())
}

#[test]
fn transfer_polkadot_from_relay_chain_to_pendulum() {
	MockNet::reset();

	let transfer_amount: Balance = dot(20);
	println!("transfer DOT amount : {} ", transfer_amount);
	let mut balance_before = 0;
	let mut orml_tokens_before = 0;
	PendulumParachain::execute_with(|| {
		balance_before = Balances::free_balance(&ALICE.into());
		println!("Alice balance_before {}", balance_before);
		let orml_tokens_before = pendulum_runtime::Tokens::balance(
			pendulum_runtime::PendulumCurrencyId::XCM(0),
			&ALICE.into(),
		);
		println!("Alice orml tokens DOT before {}", orml_tokens_before);
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

	Relay::execute_with(|| {
		use polkadot_runtime::{RuntimeEvent, System};
		for i in System::events().iter() {
			println!("polkadot_runtime {:?}", i);
		}
	});

	println!("____________________________________________________");

	PendulumParachain::execute_with(|| {
		use pendulum_runtime::{RuntimeEvent, System, Tokens};
		for i in System::events().iter() {
			println!(" Pendulum_runtime {:?}", i);
		}

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

	return
	//TODO move to another test.
	//TODO rewrite the test with correct assert statements
	Relay::execute_with(|| {
		let before_bob_free_balance = polkadot_runtime::Balances::free_balance(&BOB.into());
		println!("BOB DOT BEFORE balance on relay chain {} ", before_bob_free_balance);
		assert_eq!(before_bob_free_balance, 0);
	});

	PendulumParachain::execute_with(|| {
		assert_ok!(pendulum_runtime::XTokens::transfer(
			pendulum_runtime::RuntimeOrigin::signed(ALICE.into()),
			pendulum_runtime::PendulumCurrencyId::XCM(0),
			dot(1),
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
	//h horozin rmp  hrmp
	// ump dmp
	Relay::execute_with(|| {
		let after_bob_free_balance = polkadot_runtime::Balances::free_balance(&BOB.into());
		println!("BOB DOT AFTER balance on relay chain {} ", after_bob_free_balance);
		assert_eq!(after_bob_free_balance, 999988476752);
	});
}
