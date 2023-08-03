use crate::{AMPLITUDE_ID, PENDULUM_ID, STATEMINE_ID, STATEMINT_ID};
use frame_support::traits::GenesisBuild;
use polkadot_core_primitives::{AccountId, Balance};
use sp_io::TestExternalities;
use crate::mock::ParachainType;

pub fn units(amount: Balance) -> Balance {
	amount * 10u128.saturating_pow(9)
}

pub const ALICE: [u8; 32] = [4u8; 32];
pub const BOB: [u8; 32] = [5u8; 32];

pub trait MockNetExt {
	fn universal_reset();
}

pub trait Builder<Currency> {

	fn balances(self, balances: Vec<(AccountId, Currency, Balance)>) -> Self;

	fn build(self) -> sp_io::TestExternalities;
}

pub struct ExtBuilderParachain<Currency> {
	balances: Vec<(AccountId, Currency, Balance)>,
	chain: ParachainType
}

impl <Currency> ExtBuilderParachain<Currency> {
	fn get_parachain_id(&self) -> u32 {
		match self.chain {
			ParachainType::Statemint => STATEMINT_ID,
			ParachainType::Statemine => STATEMINE_ID,
			ParachainType::Pendulum => PENDULUM_ID,
			ParachainType::Amplitude => AMPLITUDE_ID
		}
	}
}

// for Pendulum and Amplitude
impl ExtBuilderParachain<CurrencyId> {
	pub fn pendulum_default() -> Self {
		Self {
			balances: vec![],
			chain: ParachainType::Pendulum,
		}
	}

	pub fn amplitude_default() -> Self {
		Self {
			balances: vec![],
			chain: ParachainType::Amplitude,
		}
	}
}

// for Assethub: Statemint and Statemine
impl ExtBuilderParachain<u128> {
	pub fn statemint_default() -> Self {
		Self {
			balances: vec![],
			chain: ParachainType::Statemint,
		}
	}

	pub fn statemine_default() -> Self {
		Self {
			balances: vec![],
			chain: ParachainType::Statemine,
		}
	}
}

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
pub(super) use create_test_externalities;

macro_rules! build_relaychain {
	($runtime:ty, $system:tt) => {{
		let mut t = frame_system::GenesisConfig::default().build_storage::<$runtime>().unwrap();
		pallet_balances::GenesisConfig::<$runtime> {
			balances: vec![
				(AccountId::from(ALICE), units(100000)),
				(AccountId::from(BOB), units(100)),
				(para_account_id(2094), 10 * units(100000)),
			],
		}
		.assimilate_storage(&mut t)
		.unwrap();
		polkadot_runtime_parachains::configuration::GenesisConfig::<$runtime> {
			config: default_parachains_host_configuration(),
		}
		.assimilate_storage(&mut t)
		.unwrap();

		crate::setup::create_test_externalities!($runtime, $system, t)
	}};
}
pub(super) use build_relaychain;
use pendulum_runtime::CurrencyId;

macro_rules! build_parachain_with_orml {
	($self:ident, $runtime:ty, $system:tt, $balance:literal, $orml_balance:literal) => {{
		let mut t = frame_system::GenesisConfig::default().build_storage::<$runtime>().unwrap();
		pallet_balances::GenesisConfig::<Runtime> {
			balances: vec![
				(AccountId::from(ALICE), $balance),
				(AccountId::from(BOB), $balance),
			],
		}
		.assimilate_storage(&mut t)
		.unwrap();

		orml_tokens::GenesisConfig::<$runtime> {
			balances: vec![(AccountId::from(BOB), CurrencyId::XCM(0), units($orml_balance))],
		}
		.assimilate_storage(&mut t)
		.unwrap();

		build_parachain!($self, $runtime,$system,t)
	}};
}

macro_rules! build_parachain {
	($self:ident, $runtime:ty, $system:tt) => {{
		let mut t = frame_system::GenesisConfig::default()
			.build_storage::<$runtime>()
			.unwrap();

		pallet_balances::GenesisConfig::<$runtime> { balances: vec![] }
			.assimilate_storage(&mut t)
			.unwrap();

		build_parachain!($self, $runtime,$system,t)
	}};

	($self:ident, $runtime:ty, $system:tt, $storage:ident) => {{
		<parachain_info::GenesisConfig as GenesisBuild<$runtime>>::assimilate_storage(
			&parachain_info::GenesisConfig { parachain_id: $self.get_parachain_id().into() },
			&mut $storage,
		)
		.unwrap();

		create_test_externalities!($runtime,$system,$storage)

	}};
}


impl Builder<CurrencyId> for ExtBuilderParachain<CurrencyId> {
	fn balances(mut self, balances: Vec<(AccountId, CurrencyId, Balance)>) -> Self {
		self.balances = balances;
		self
	}

	fn build(self) -> TestExternalities {
		match self.chain {
			ParachainType::Pendulum => {
				use pendulum_runtime::{Runtime,System};
				// initial balance of 1_000_000_000,
				// orml balance of 100
				build_parachain_with_orml!(self,Runtime,System,1_000_000_000,100)
			}
			ParachainType::Amplitude => {
				use amplitude_runtime::{Runtime,System};
				// initial balance of 1_000_000_000,
				// orml balance of 100
				build_parachain_with_orml!(self, Runtime,System,1_000_000_000,100)
			},
			_ => panic!("cannot use this chain to build")
		}
	}
}

impl Builder<u128> for ExtBuilderParachain<u128> {
	fn balances(mut self, balances: Vec<(AccountId, u128, Balance)>) -> Self {
		self.balances = balances;
		self
	}

	fn build(self) -> TestExternalities {
		match self.chain {
			ParachainType::Statemint => {
				use statemint_runtime::{Runtime,System};
				build_parachain!(self,Runtime,System)
			}
			ParachainType::Statemine => {
				use statemine_runtime::{Runtime,System};
				build_parachain!(self,Runtime,System)
			}
			_ => panic!("cannot use this chain to build")
		}
	}
}
