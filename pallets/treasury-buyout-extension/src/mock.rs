use crate::{
	self as treasury_buyout_extension, default_weights::SubstrateWeight, Config, PriceGetter,
};
use frame_support::{
	pallet_prelude::GenesisBuild,
	parameter_types,
	traits::{ConstU32, Everything},
};
use orml_currencies::BasicCurrencyAdapter;
use orml_traits::parameter_type_with_key;
use sp_arithmetic::{FixedPointNumber, FixedU128, Permill};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup, One, Zero},
	DispatchError,
};
use sp_std::fmt::Debug;
use spacewalk_primitives::DecimalsLookup;

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
		TreasuryBuyoutExtension: treasury_buyout_extension::{Pallet, Storage, Call, Event<T>},
	}
);

pub type AccountId = u64;
pub type Balance = u128;
pub type BlockNumber = u64;
pub type Index = u64;
pub type Amount = i64;
pub type CurrencyId = u64;

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

pub type TestEvent = RuntimeEvent;

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

parameter_types! {
	pub const MaxLocks: u32 = 50;
	pub const GetNativeCurrencyId: CurrencyId = u64::MAX;
	pub const RelayChainCurrencyId: CurrencyId = 0u64;
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
	pub const SellFee: Permill = Permill::from_percent(1);
	pub const MinAmountToBuyout: Balance = 100 * UNIT;
	// 24 hours in blocks (where average block time is 12 seconds)
	pub const BuyoutPeriod: u32 = 7200;
	// Maximum number of allowed currencies for buyout
	pub const MaxAllowedBuyoutCurrencies: u32 = 20;
}

impl pallet_balances::Config for Test {
	type MaxLocks = MaxLocks;
	type Balance = Balance;
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
	type HoldIdentifier = RuntimeHoldReason;
}

impl orml_currencies::Config for Test {
	type MultiCurrency = Tokens;
	type NativeCurrency = BasicCurrencyAdapter<Test, Balances, Amount, BlockNumber>;
	type GetNativeCurrencyId = GetNativeCurrencyId;
	type WeightInfo = ();
}

pub struct OracleMock;
impl PriceGetter<CurrencyId> for OracleMock {
	fn get_price<FixedNumber>(currency_id: CurrencyId) -> Result<FixedNumber, DispatchError>
	where
		FixedNumber: FixedPointNumber + One + Zero + Debug + TryFrom<FixedU128>,
	{
		// This simulates price fetching error for testing pre_dispatch validation but only for one specific supported asset
		if currency_id == 2u64 {
			return Err(DispatchError::Other("No price"))
		}

		let price: FixedNumber = FixedNumber::one()
			.try_into()
			.map_err(|_| DispatchError::Other("FixedU128 convert"))?;
		Ok(price)
	}
}

pub struct DecimalsLookupImpl;
impl DecimalsLookup for DecimalsLookupImpl {
	type CurrencyId = CurrencyId;

	fn decimals(currency_id: Self::CurrencyId) -> u32 {
		match currency_id {
			0 => 10,
			1 => 6,
			2 | 6 => 18,
			_ => 12,
		}
	}
}

impl Config for Test {
	/// The overarching event type.
	type RuntimeEvent = RuntimeEvent;
	/// Used for currency-related operations
	type Currency = Currencies;
	/// Used for getting the treasury account
	type TreasuryAccount = TreasuryAccount;
	/// Buyout period in blocks
	type BuyoutPeriod = BuyoutPeriod;
	/// Fee from the native asset buyouts
	type SellFee = SellFee;
	/// Used for fetching prices of currencies from oracle
	type PriceGetter = OracleMock;
	/// Used for fetching decimals of assets
	type DecimalsLookup = DecimalsLookupImpl;
	/// Min amount of native token to buyout
	type MinAmountToBuyout = MinAmountToBuyout;
	/// Maximum number of storage updates for allowed currencies in one extrinsic call
	type MaxAllowedBuyoutCurrencies = MaxAllowedBuyoutCurrencies;
	/// Weight information for extrinsics in this pallet.
	type WeightInfo = SubstrateWeight<Test>;
	/// Currency id of relay chain
	#[cfg(feature = "runtime-benchmarks")]
	type RelayChainCurrencyId = RelayChainCurrencyId;
}

// ------- Constants and Genesis Config ------ //

pub const USER: u64 = 0;
pub const TREASURY_ACCOUNT: u64 = TreasuryAccount::get();
// Initial balance of 200 native token
pub const USERS_INITIAL_NATIVE_BALANCE: u128 = 200 * UNIT;
// Initial balance of 200 DOT (DOT has 10 decimals)
pub const USERS_INITIAL_DOT_BALANCE: u128 = 200_0000000000;
// Initial balance of 1000 native token
pub const TREASURY_INITIAL_BALANCE: u128 = 1000 * UNIT;

pub struct ExtBuilder;

impl ExtBuilder {
	pub fn build() -> sp_io::TestExternalities {
		let mut storage = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();

		let dot_currency_id = RelayChainCurrencyId::get();

		orml_tokens::GenesisConfig::<Test> {
			balances: vec![(USER, dot_currency_id, USERS_INITIAL_DOT_BALANCE)],
		}
		.assimilate_storage(&mut storage)
		.unwrap();

		pallet_balances::GenesisConfig::<Test> {
			balances: vec![
				(USER, USERS_INITIAL_NATIVE_BALANCE),
				(TREASURY_ACCOUNT, TREASURY_INITIAL_BALANCE),
			],
		}
		.assimilate_storage(&mut storage)
		.unwrap();

		treasury_buyout_extension::GenesisConfig::<Test> {
			allowed_currencies: vec![dot_currency_id, 1, 2, 6],
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
