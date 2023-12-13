use crate::{sibling, AMPLITUDE_ID, ASSETHUB_ID, PENDULUM_ID, SIBLING_ID};
use frame_support::traits::GenesisBuild;
use pendulum_runtime::CurrencyId as PendulumCurrencyId;
use polkadot_core_primitives::{AccountId, Balance, BlockNumber};
use polkadot_parachain::primitives::Id as ParaId;
use polkadot_primitives::v2::{MAX_CODE_SIZE, MAX_POV_SIZE};
use polkadot_runtime_parachains::configuration::HostConfiguration;
use sibling::CurrencyId as SiblingCurrencyId;
use sp_io::TestExternalities;
use sp_runtime::traits::AccountIdConversion;
use xcm_emulator::Weight;

use statemine_runtime as kusama_asset_hub_runtime;
use statemint_runtime as polkadot_asset_hub_runtime;

pub const ALICE: [u8; 32] = [4u8; 32];
pub const BOB: [u8; 32] = [5u8; 32];

pub const UNIT: Balance = 1_000_000_000_000;
pub const TEN_UNITS: Balance = 10_000_000_000_000;

pub const USDT_ASSET_ID: u32 = 1984; //Real USDT Asset ID of both Polkadot's and Kusama's Asset Hub
pub const INCORRECT_ASSET_ID: u32 = 0; //asset id that pendulum/amplitude does NOT SUPPORT

pub const NATIVE_INITIAL_BALANCE: Balance = TEN_UNITS;
pub const ORML_INITIAL_BALANCE: Balance = TEN_UNITS;

macro_rules! create_test_externalities {
	($runtime:ty, $system:ident, $storage:ident) => {{
		<pallet_xcm::GenesisConfig as GenesisBuild<$runtime>>::assimilate_storage(
			&pallet_xcm::GenesisConfig { safe_xcm_version: Some(2) },
			&mut $storage,
		)
		.unwrap();
		let mut ext = sp_io::TestExternalities::new($storage);
		ext.execute_with(|| $system::set_block_number(1));
		ext
	}};
}

macro_rules! build_relaychain {
	($runtime:ty, $system:tt, $para_account_id: ident) => {{
		let mut t = frame_system::GenesisConfig::default().build_storage::<$runtime>().unwrap();
		pallet_balances::GenesisConfig::<$runtime> {
			balances: vec![
				(AccountId::from(ALICE), units(100000)),
				(AccountId::from(BOB), units(100)),
				(para_account_id($para_account_id), 10 * units(100000)),
			],
		}
		.assimilate_storage(&mut t)
		.unwrap();
		polkadot_runtime_parachains::configuration::GenesisConfig::<$runtime> {
			config: default_parachains_host_configuration(),
		}
		.assimilate_storage(&mut t)
		.unwrap();

		create_test_externalities!($runtime, $system, t)
	}};
}

macro_rules! build_parachain_with_orml {
	($self:ident, $runtime:ty, $system:tt, $balance:tt, $orml_balance:tt, $currency_id_type:ty) => {{
		let mut t = frame_system::GenesisConfig::default().build_storage::<$runtime>().unwrap();
		pallet_balances::GenesisConfig::<$runtime> {
			balances: vec![(AccountId::from(ALICE), $balance), (AccountId::from(BOB), $balance)],
		}
		.assimilate_storage(&mut t)
		.unwrap();

		type CurrencyId = $currency_id_type;
		orml_tokens::GenesisConfig::<$runtime> {
			balances: vec![
				(AccountId::from(BOB), CurrencyId::XCM(0), units($orml_balance)),
				(AccountId::from(ALICE), CurrencyId::XCM(0), units($orml_balance)),
			],
		}
		.assimilate_storage(&mut t)
		.unwrap();

		build_parachain!($self, $runtime, $system, t)
	}};
}

macro_rules! build_parachain {
	($self:ident, $runtime:ty, $system:tt) => {{
		let mut t = frame_system::GenesisConfig::default().build_storage::<$runtime>().unwrap();

		pallet_balances::GenesisConfig::<$runtime> { balances: vec![] }
			.assimilate_storage(&mut t)
			.unwrap();

		build_parachain!($self, $runtime, $system, t)
	}};

	($self:ident, $runtime:ty, $system:tt, $storage:ident) => {{
		<parachain_info::GenesisConfig as GenesisBuild<$runtime>>::assimilate_storage(
			&parachain_info::GenesisConfig { parachain_id: $self.get_parachain_id().into() },
			&mut $storage,
		)
		.unwrap();

		create_test_externalities!($runtime, $system, $storage)
	}};
}

pub trait Builder<Currency> {
	fn balances(self, balances: Vec<(AccountId, Currency, Balance)>) -> Self;
	fn build(self) -> TestExternalities;
}

pub enum ParachainType {
	PolkadotAssetHub,
	KusamaAssetHub,
	Pendulum,
	Amplitude,
	Sibling,
}

pub struct ExtBuilderParachain<Currency> {
	balances: Vec<(AccountId, Currency, Balance)>,
	chain: ParachainType,
}

pub fn units(amount: Balance) -> Balance {
	amount * 10u128.saturating_pow(9)
}

pub fn para_account_id(id: u32) -> polkadot_core_primitives::AccountId {
	ParaId::from(id).into_account_truncating()
}

pub fn polkadot_relay_ext() -> sp_io::TestExternalities {
	use polkadot_runtime::{Runtime, System};
	build_relaychain!(Runtime, System, PENDULUM_ID)
}

pub fn kusama_relay_ext() -> sp_io::TestExternalities {
	use kusama_runtime::{Runtime, System};
	build_relaychain!(Runtime, System, AMPLITUDE_ID)
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

pub fn para_ext(chain: ParachainType) -> sp_io::TestExternalities {
	match chain {
		ParachainType::PolkadotAssetHub =>
			ExtBuilderParachain::polkadot_asset_hub_default().balances(vec![]).build(),
		ParachainType::KusamaAssetHub =>
			ExtBuilderParachain::kusama_asset_hub_default().balances(vec![]).build(),
		ParachainType::Pendulum => ExtBuilderParachain::pendulum_default().balances(vec![]).build(),
		ParachainType::Amplitude =>
			ExtBuilderParachain::amplitude_default().balances(vec![]).build(),
		ParachainType::Sibling => ExtBuilderParachain::sibling_default().balances(vec![]).build(),
	}
}

impl<Currency> ExtBuilderParachain<Currency> {
	fn get_parachain_id(&self) -> u32 {
		match self.chain {
			ParachainType::PolkadotAssetHub => ASSETHUB_ID,
			ParachainType::KusamaAssetHub => ASSETHUB_ID,
			ParachainType::Pendulum => PENDULUM_ID,
			ParachainType::Sibling => SIBLING_ID,
			ParachainType::Amplitude => AMPLITUDE_ID,
		}
	}
}

// ------------------- for Pendulum and Amplitude -------------------
impl ExtBuilderParachain<PendulumCurrencyId> {
	pub fn pendulum_default() -> Self {
		Self { balances: vec![], chain: ParachainType::Pendulum }
	}

	pub fn amplitude_default() -> Self {
		Self { balances: vec![], chain: ParachainType::Amplitude }
	}
}

impl Builder<PendulumCurrencyId> for ExtBuilderParachain<PendulumCurrencyId> {
	fn balances(mut self, balances: Vec<(AccountId, PendulumCurrencyId, Balance)>) -> Self {
		self.balances = balances;
		self
	}

	fn build(self) -> TestExternalities {
		match self.chain {
			ParachainType::Pendulum => {
				use pendulum_runtime::{Runtime, System};
				build_parachain_with_orml!(
					self,
					Runtime,
					System,
					NATIVE_INITIAL_BALANCE,
					ORML_INITIAL_BALANCE,
					PendulumCurrencyId
				)
			},
			ParachainType::Amplitude => {
				use amplitude_runtime::{Runtime, System};
				build_parachain_with_orml!(
					self,
					Runtime,
					System,
					NATIVE_INITIAL_BALANCE,
					ORML_INITIAL_BALANCE,
					PendulumCurrencyId
				)
			},
			_ => panic!("cannot use this chain to build"),
		}
	}
}

// ------------------- for Sibling -------------------
impl ExtBuilderParachain<SiblingCurrencyId> {
	pub fn sibling_default() -> Self {
		Self { balances: vec![], chain: ParachainType::Sibling }
	}
}

impl Builder<SiblingCurrencyId> for ExtBuilderParachain<SiblingCurrencyId> {
	fn balances(mut self, balances: Vec<(AccountId, SiblingCurrencyId, Balance)>) -> Self {
		self.balances = balances;
		self
	}

	fn build(self) -> TestExternalities {
		match self.chain {
			ParachainType::Sibling => {
				use sibling::{Runtime, System};
				build_parachain_with_orml!(
					self,
					Runtime,
					System,
					NATIVE_INITIAL_BALANCE,
					ORML_INITIAL_BALANCE,
					SiblingCurrencyId
				)
			},
			_ => panic!("cannot use this chain to build"),
		}
	}
}

// ------------------- for Statemint and Statemine -------------------
impl ExtBuilderParachain<u128> {
	pub fn polkadot_asset_hub_default() -> Self {
		Self { balances: vec![], chain: ParachainType::PolkadotAssetHub }
	}

	pub fn kusama_asset_hub_default() -> Self {
		Self { balances: vec![], chain: ParachainType::KusamaAssetHub }
	}
}

impl Builder<u128> for ExtBuilderParachain<u128> {
	fn balances(mut self, balances: Vec<(AccountId, u128, Balance)>) -> Self {
		self.balances = balances;
		self
	}

	fn build(self) -> TestExternalities {
		match self.chain {
			ParachainType::PolkadotAssetHub => {
				use polkadot_asset_hub_runtime::{Runtime, System};
				build_parachain!(self, Runtime, System)
			},
			ParachainType::KusamaAssetHub => {
				use kusama_asset_hub_runtime::{Runtime, System};
				build_parachain!(self, Runtime, System)
			},
			_ => panic!("cannot use this chain to build"),
		}
	}
}
