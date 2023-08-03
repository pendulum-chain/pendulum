use crate::setup::{units, ALICE, BOB};
use frame_support::traits::GenesisBuild;
use polkadot_core_primitives::{AccountId, Balance, BlockNumber};
use polkadot_parachain::primitives::Id as ParaId;
use polkadot_primitives::v2::{MAX_CODE_SIZE, MAX_POV_SIZE};
use polkadot_runtime_parachains::configuration::HostConfiguration;
use sp_runtime::traits::AccountIdConversion;
use xcm_emulator::Weight;
use crate::setup::{build_relaychain, Builder, ExtBuilderParachain};


pub const UNIT: Balance = 1_000_000_000_000;
pub const TEN_UNITS: Balance = 10_000_000_000_000;
pub const USDT_ASSET_ID: u32 = 1984; //Real USDT Asset ID both from Statemint and Statemine
pub const INCORRECT_ASSET_ID: u32 = 0; //Incorrect asset id that pendulum is not supporting pendulum_runtime xcm_config


pub enum ParachainType {
	Statemint,
	Statemine,
	Pendulum,
	Amplitude
}

pub fn para_account_id(id: u32) -> polkadot_core_primitives::AccountId {
	ParaId::from(id).into_account_truncating()
}

pub fn polkadot_relay_ext() -> sp_io::TestExternalities {
	use polkadot_runtime::{Runtime,System};
	build_relaychain!(Runtime, System)
}

pub fn kusama_relay_ext() -> sp_io::TestExternalities {
	use kusama_runtime::{Runtime,System};
	build_relaychain!(Runtime,System)
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
		ump_service_total_weight: Weight::from_parts(4 * 1_000_000_000, 0),
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


pub fn para_ext(chain:ParachainType) -> sp_io::TestExternalities {
	match chain {
		ParachainType::Statemint => ExtBuilderParachain::statemint_default()
			.balances(vec![]).build(),
		ParachainType::Statemine => ExtBuilderParachain::statemine_default()
			.balances(vec![]).build(),
		ParachainType::Pendulum => ExtBuilderParachain::pendulum_default()
			.balances(vec![]).build(),
		ParachainType::Amplitude => ExtBuilderParachain::amplitude_default()
			.balances(vec![]).build(),
	}
}

