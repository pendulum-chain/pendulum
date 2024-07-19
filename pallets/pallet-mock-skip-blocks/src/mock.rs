#[cfg(feature = "instant-seal")]
use crate::{self as pallet_mock_skip_blocks, Config};
#[cfg(feature = "instant-seal")]
use frame_support::{parameter_types, traits::Everything};
#[cfg(feature = "instant-seal")]
use sp_core::H256;
#[cfg(feature = "instant-seal")]
use sp_runtime::{
	testing::Header,
	traits::{BlakeTwo256, IdentityLookup},
};

#[cfg(feature = "instant-seal")]
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
#[cfg(feature = "instant-seal")]
type Block = frame_system::mocking::MockBlock<Test>;

// Configure a mock runtime to test the pallet.
#[cfg(feature = "instant-seal")]
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Storage, Config, Event<T>},
		MockSkipBlocks: pallet_mock_skip_blocks::{Pallet, Storage, Call, Event<T>},
	}
);

#[cfg(feature = "instant-seal")]
pub type AccountId = u64;
#[cfg(feature = "instant-seal")]
pub type BlockNumber = u64;
#[cfg(feature = "instant-seal")]
pub type Index = u64;

#[cfg(feature = "instant-seal")]
parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

#[cfg(feature = "instant-seal")]
pub type TestEvent = RuntimeEvent;

#[cfg(feature = "instant-seal")]
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
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

#[cfg(feature = "instant-seal")]
impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
}

#[cfg(feature = "instant-seal")]
pub struct ExtBuilder;

#[cfg(feature = "instant-seal")]
impl ExtBuilder {
	pub fn build() -> sp_io::TestExternalities {
		let storage = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
		sp_io::TestExternalities::from(storage)
	}
}

#[cfg(feature = "instant-seal")]
pub fn run_test<T>(test: T)
where
	T: FnOnce(),
{
	ExtBuilder::build().execute_with(|| {
		System::set_block_number(1);
		test();
	});
}
