use crate::{
	self as orml_tokens_management_extension, default_weights::SubstrateWeight, Config,
	CurrencyIdCheck,
};
use frame_support::{
	parameter_types,
	traits::{ConstU32, Everything},
};
use orml_currencies::BasicCurrencyAdapter;
use orml_traits::parameter_type_with_key;
use sp_core::{ConstU128, H256};
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Tokens: orml_tokens,
		Balances: pallet_balances,
		Currencies: orml_currencies,
		TokensExtension: orml_tokens_management_extension,
	}
);

pub type AccountId = u64;
pub type Balance = u128;
pub type BlockNumber = u64;
pub type Nonce = u64;
pub type Amount = i64;
pub type CurrencyId = u64;

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}
impl frame_system::Config for Test {
	type Block = Block;
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = Nonce;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = TestEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

pub type TestEvent = RuntimeEvent;

parameter_types! {
	pub const MaxLocks: u32 = 50;
	pub const GetNativeCurrencyId: CurrencyId = 0;
}

parameter_type_with_key! {
	pub ExistentialDeposits: |_currency_id: CurrencyId| -> Balance {
		0
	};
}

pub struct CurrencyHooks<T>(sp_std::marker::PhantomData<T>);
impl<T: orml_tokens::Config>
	orml_traits::currency::MutationHooks<T::AccountId, T::CurrencyId, T::Balance> for CurrencyHooks<T>
{
	type OnDust = orml_tokens::BurnDust<T>;
	type OnSlash = ();
	type PreDeposit = ();
	type PostDeposit = ();
	type PreTransfer = ();
	type PostTransfer = ();
	type OnNewTokenAccount = ();
	type OnKilledTokenAccount = ();
}

impl orml_tokens::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type Amount = Amount;
	type CurrencyId = CurrencyId;
	type WeightInfo = ();
	type ExistentialDeposits = ExistentialDeposits;
	type CurrencyHooks = CurrencyHooks<Self>;
	type MaxLocks = MaxLocks;
	type MaxReserves = ConstU32<0>;
	type ReserveIdentifier = ();
	type DustRemovalWhitelist = Everything;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1000;
	pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Test {
	type MaxLocks = MaxLocks;
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Test>;
	type MaxReserves = MaxReserves;
	type ReserveIdentifier = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type MaxHolds = ConstU32<1>;
	type RuntimeHoldReason = RuntimeHoldReason;
}

impl orml_currencies::Config for Test {
	type MultiCurrency = Tokens;
	type NativeCurrency = BasicCurrencyAdapter<Test, Balances, Amount, BlockNumber>;
	type GetNativeCurrencyId = GetNativeCurrencyId;
	type WeightInfo = ();
}

pub struct CurrencyIdCheckerImpl;

impl CurrencyIdCheck for CurrencyIdCheckerImpl {
	type CurrencyId = CurrencyId;

	// We allow currency id 0-9
	fn is_valid_currency_id(currency_id: &Self::CurrencyId) -> bool {
		*currency_id < 10
	}
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = SubstrateWeight<Test>;
	type CurrencyIdChecker = CurrencyIdCheckerImpl;
	type AssetDeposit = ConstU128<DEPOSIT>;
	type DepositCurrency = GetNativeCurrencyId;
}

// ------- Constants and Genesis Config ------ //

pub const USER_0: u64 = 0;
pub const USER_1: u64 = 1;
pub const USER_2: u64 = 2;
pub const USER_3: u64 = 3;

pub const USERS_INITIAL_BALANCE: u128 = 1000000;
pub const DEPOSIT: u128 = 5000;
pub struct ExtBuilder;

impl ExtBuilder {
	pub fn build() -> sp_io::TestExternalities {
		let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

		let native_currency_id = GetNativeCurrencyId::get();

		orml_tokens::GenesisConfig::<Test> {
			balances: vec![
				(USER_0, native_currency_id, USERS_INITIAL_BALANCE),
				(USER_1, native_currency_id, USERS_INITIAL_BALANCE),
				(USER_2, native_currency_id, USERS_INITIAL_BALANCE),
			],
		}
		.assimilate_storage(&mut storage)
		.unwrap();

		pallet_balances::GenesisConfig::<Test> {
			balances: vec![
				(USER_0, USERS_INITIAL_BALANCE),
				(USER_1, USERS_INITIAL_BALANCE),
				(USER_2, USERS_INITIAL_BALANCE),
			],
		}
		.assimilate_storage(&mut storage)
		.unwrap();

		sp_io::TestExternalities::from(storage)
	}
}

pub fn run_test<T>(test: T)
where
	T: FnOnce(),
{
	ExtBuilder::build().execute_with(|| {
		System::set_block_number(1);
		test();
	});
}
