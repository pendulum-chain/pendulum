//! Benchmarks for the token-migration pallet.
//!
//! Run against the Pendulum runtime on reference hardware to generate
//! production weights, e.g.:
//! `cargo run --release --features runtime-benchmarks -- benchmark pallet \
//!    --chain pendulum --pallet token-migration --extrinsic '*' --steps 50 --repeat 20`

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::{EnsureOrigin, Get};
use frame_system::RawOrigin;
use sp_runtime::traits::Bounded;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn migrate() {
		let caller: T::AccountId = whitelisted_caller();
		T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value() / 2u32.into());
		let amount = T::MinimumMigrationAmount::get().saturating_mul(10u32.into());
		let base_address = H160::repeat_byte(0xBE);

		#[extrinsic_call]
		migrate(RawOrigin::Signed(caller.clone()), amount, base_address);

		assert_eq!(TotalMigrated::<T>::get(), amount);
		assert_eq!(NextNonce::<T>::get(), 1);
	}

	#[benchmark]
	fn set_paused() -> Result<(), BenchmarkError> {
		let origin =
			T::PauseOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		set_paused(origin as T::RuntimeOrigin, true);

		assert!(Paused::<T>::get());
		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::ExtBuilder::build(), crate::mock::Test);
}
