use crate::{
	self as treasury_buyout_extension, Config, PriceGetter, CurrencyIdChecker,
};
use frame_support::{
	pallet_prelude::GenesisBuild,
	parameter_types,
	traits::{ConstU32, Everything, UnixTime},
};
use orml_currencies::BasicCurrencyAdapter;
use orml_traits::parameter_type_with_key;
use sp_std::fmt::Debug;
use sp_arithmetic::{FixedU128, FixedPointNumber, Permill};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup, One, Zero}, DispatchError,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

pub const UNIT: Balance = 1_000_000_000_000; 

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Storage, Config, Event<T>},
		Tokens: orml_tokens::{Pallet, Storage, Config<T>, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Event<T>},
		Currencies: orml_currencies::{Pallet, Call},
	}
);

pub type AccountId = u64;
pub type Balance = u128;
pub type BlockNumber = u64;
pub type Index = u64;
pub type Amount = i64;
pub type CurrencyId = spacewalk_primitives::CurrencyId;

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}
impl frame_system::Config for Test {
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Index = Index;
	type BlockNumber = BlockNumber;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
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
	pub const GetNativeCurrencyId: CurrencyId = spacewalk_primitives::CurrencyId::Native;
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
    pub const TreasuryAccount: AccountId = u64::MAX;
    pub const SellFee: Permill = Permill::from_percent(10);
    pub const MinAmountToBuyout: Balance = 100 * UNIT;

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
}

impl orml_currencies::Config for Test {
	type MultiCurrency = Tokens;
	type NativeCurrency = BasicCurrencyAdapter<Test, Balances, Amount, BlockNumber>;
	type GetNativeCurrencyId = GetNativeCurrencyId;
	type WeightInfo = ();
}

pub struct TimeMock;

impl UnixTime for TimeMock {
    fn now() -> core::time::Duration {
        //core::time::Duration::from_millis(CURRENT_TIME.with(|v| *v.borrow()))
        core::time::Duration::from_millis(1598006981634)
    }
}

pub struct CurrencyIdCheckerImpl;

impl CurrencyIdChecker<CurrencyId> for CurrencyIdCheckerImpl {
	// We allow only XCM assets
	fn is_allowed_currency_id(currency_id: &CurrencyId) -> bool {
		matches!(currency_id, spacewalk_primitives::CurrencyId::XCM(_))
	}
}

pub struct OracleMock;

// Maybe put it in text ext?
impl PriceGetter<CurrencyId> for OracleMock {
	fn get_price<FixedNumber>(currency_id: CurrencyId) -> Result<FixedNumber, DispatchError>
    where
        FixedNumber: FixedPointNumber + One + Zero + Debug  + TryFrom<FixedU128>,
    {
		//TODO: get price from oracle
		let price: FixedNumber = FixedNumber::one().try_into().map_err(|_| DispatchError::Other("FixedU128 convert"))?;
		
		Ok(price)
    }
}

impl Config for Test {
    /// The overarching event type.
    type RuntimeEvent = RuntimeEvent;
    /// Used for currency-related operations
    type Currency = Currencies; 
    /// Used for getting the treasury account
    type TreasuryAccount = TreasuryAccount;
    /// Timestamp provider
    type UnixTime = TimeMock;
    /// Fee from the native asset buyouts
    type SellFee =  SellFee;
	/// Type that allows for checking if currency type is ownable by users
	type CurrencyIdChecker = CurrencyIdCheckerImpl;
    /// Used for fetching prices of currencies from oracle
    type PriceGetter = OracleMock;
    /// Min amount of native token to buyout
    type MinAmountToBuyout = MinAmountToBuyout;
	/// Weight information for extrinsics in this pallet.
	type WeightInfo = ();

}

// ------- Constants and Genesis Config ------ //

pub const USER: u64 = 0;

pub const USERS_INITIAL_BALANCE: u128 = 1000000;
pub const DEPOSIT: u128 = 5000;
pub struct ExtBuilder;

impl ExtBuilder {
	pub fn build() -> sp_io::TestExternalities {
		let mut storage = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();

		let native_currency_id = GetNativeCurrencyId::get();
		//let dot_currency_id = 

		orml_tokens::GenesisConfig::<Test> {
			balances: vec![
				(USER, native_currency_id, USERS_INITIAL_BALANCE),
			],
		}
		.assimilate_storage(&mut storage)
		.unwrap();

		pallet_balances::GenesisConfig::<Test> {
			balances: vec![
				(USER, USERS_INITIAL_BALANCE),
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
