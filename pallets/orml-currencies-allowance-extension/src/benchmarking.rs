#![allow(warnings)]
use super::{Pallet as TokenAllowance, *};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;
use sp_std::prelude::*;
use currency::{
	getters::{get_relay_chain_currency_id as get_collateral_currency_id, *}
};

fn get_currency_id<T: crate::Config + currency::Config>() -> CurrencyOf<T> {
	get_collateral_currency_id::<T>()
}

benchmarks! {
	add_allowed_currencies {
        let nativeCurrencyId = <T as  orml_currencies::Config>::GetNativeCurrencyId::get();
		let v: Vec<CurrencyOf<T>> = vec![nativeCurrencyId];
	}: _(RawOrigin::Root, v)
	verify {
        let nativeCurrencyId = <T as  orml_currencies::Config>::GetNativeCurrencyId::get();
		assert_eq!(AllowedCurrencies::<T>::get(nativeCurrencyId), Some(()));
	}
}

impl_benchmark_test_suite!(TokenAllowance, crate::mock::ExtBuilder::build(), crate::mock::Test);
