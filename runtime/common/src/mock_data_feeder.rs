use oracle::testing_utils::{DataFeederExtended};
use spin::MutexGuard;
use spacewalk_primitives::UnsignedFixedPoint;
use sp_runtime::DispatchResult;
use spacewalk_primitives::oracle::Key;
use oracle::TimestampedValue;
use oracle::DataFeeder;
use oracle::DataProvider;

pub struct MockDataFeeder<AccountId, Moment>(
	sp_std::marker::PhantomData<AccountId>,
	sp_std::marker::PhantomData<Moment>,
);

impl<AccountId, Moment> DataProvider<Key, TimestampedValue<UnsignedFixedPoint, Moment>>
	for MockDataFeeder<AccountId, Moment>
{
	// We need to implement the DataFeeder trait to the MockDataFeeder but this function is never
	// used
	fn get(_key: &Key) -> Option<TimestampedValue<UnsignedFixedPoint, Moment>> {
		unimplemented!("Not required to implement DataProvider get function")
	}
}

impl<AccountId, Moment: Into<u64>>
	DataFeeder<Key, TimestampedValue<UnsignedFixedPoint, Moment>, AccountId>
	for MockDataFeeder<AccountId, Moment>
{
	fn feed_value(
		_who: AccountId,
		_key: Key,
		_value: TimestampedValue<UnsignedFixedPoint, Moment>,
	) -> DispatchResult {

		Ok(())
	}
}

impl<AccountId, Moment: Into<u64>>
	DataFeederExtended<Key, TimestampedValue<UnsignedFixedPoint, Moment>, AccountId>
	for MockDataFeeder<AccountId, Moment>
{
	fn clear_all_values() -> DispatchResult {
		Ok(())
	}

	fn acquire_lock() -> MutexGuard<'static, ()> {
		unimplemented!("Not required to implement DataProvider get function")
	}
}