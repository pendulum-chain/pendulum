use crate::setup::{dot, ExtBuilderPendulum, ExtStatemintBuilder, ALICE, BOB};
use frame_support::traits::GenesisBuild;
use polkadot_core_primitives::{AccountId, BlockNumber};
use polkadot_parachain::primitives::Id as ParaId;
use polkadot_primitives::v2::{MAX_CODE_SIZE, MAX_POV_SIZE};
use polkadot_runtime_parachains::configuration::HostConfiguration;
use sp_runtime::traits::AccountIdConversion;
use xcm_emulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain, Weight};

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

decl_test_parachain! {
	pub struct Statemint {
		Runtime = statemint_runtime::Runtime,
		RuntimeOrigin = statemint_runtime::RuntimeOrigin,
		XcmpMessageHandler = statemint_runtime::XcmpQueue,
		DmpMessageHandler = statemint_runtime::DmpQueue,
		new_ext = para_ext_statemint(1000),
	}
}

decl_test_network! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(1000, Statemint),
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
			(AccountId::from(BOB), dot(100)),
			(para_account_id(2094), 10 * dot(100000)),
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

pub fn para_ext_pendulum(parachain_id: u32) -> sp_io::TestExternalities {
	ExtBuilderPendulum::default()
		.balances(vec![])
		.parachain_id(parachain_id)
		.build()
}

pub fn para_ext_statemint(parachain_id: u32) -> sp_io::TestExternalities {
	ExtStatemintBuilder::default()
		.balances(vec![])
		.parachain_id(parachain_id)
		.build()
}
