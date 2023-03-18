#![allow(warnings)]
use super::{Pallet as TokenAllowance, *};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;
use sp_std::prelude::*;
// use crate::mock;
// use mock::Test;

// type R = <<Test as orml_currencies::Config>::MultiCurrency as orml_traits::MultiCurrency<
// <Test as frame_system::Config>::AccountId,
// >>::CurrencyId;

fn get_vault_id<T: crate::Config>() -> CurrencyOf<T> {
	todo!()
}

benchmarks! {
	add_allowed_currencies {
		let v: Vec<CurrencyOf<T>> = vec![];
		AllowedCurrencies::<T>::clear(1, None);
	}: _(RawOrigin::Root, v)
	verify {
		assert_eq!(true, true);
		// assert_eq!(AllowedCurrencies::<T>::get(1u64.into()), None);
	}

	// set_max_delay {
	// 	let new_max_delay: T::Moment = 1000u32.into();
	// }: _(RawOrigin::Root, new_max_delay)
	// verify {
	// 	let new_max_delay: T::Moment = 1000u32.into();
	// 	assert_eq!(MaxDelay::<T>::get(), new_max_delay);
	// }
}

impl_benchmark_test_suite!(TokenAllowance, crate::mock::ExtBuilder::build(), crate::mock::Test);
