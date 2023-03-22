#![allow(warnings)]
use super::{Pallet as TokenAllowance, *};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;
use sp_std::prelude::*;

benchmarks! {
	add_allowed_currencies {
		let native_currency_id = <T as  orml_currencies::Config>::GetNativeCurrencyId::get();
		let added_currencies: Vec<CurrencyOf<T>> = vec![native_currency_id];
	}: _(RawOrigin::Root, added_currencies)
	verify {
		let native_currency_id = <T as  orml_currencies::Config>::GetNativeCurrencyId::get();
		assert_eq!(AllowedCurrencies::<T>::get(native_currency_id), Some(()));
	}

	remove_allowed_currencies {
		let native_currency_id = <T as  orml_currencies::Config>::GetNativeCurrencyId::get();
		AllowedCurrencies::<T>::insert(native_currency_id, ());

		let removed_currencies: Vec<CurrencyOf<T>> = vec![native_currency_id];
	}: _(RawOrigin::Root, removed_currencies)
	verify {
		let native_currency_id = <T as  orml_currencies::Config>::GetNativeCurrencyId::get();
		assert_eq!(AllowedCurrencies::<T>::get(native_currency_id), None);
	}
}

impl_benchmark_test_suite!(TokenAllowance, crate::mock::ExtBuilder::build(), crate::mock::Test);
