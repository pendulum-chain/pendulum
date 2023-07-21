#![allow(warnings)]
use super::{Pallet as TokenAllowance, *};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;
use sp_std::prelude::*;

benchmarks! {
	add_allowed_currencies {
		let native_currency_id = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		let added_currencies: Vec<CurrencyOf<T>> = vec![native_currency_id];
	}: add_allowed_currencies(RawOrigin::Root, added_currencies)
	verify {
		let native_currency_id = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		assert_eq!(AllowedCurrencies::<T>::get(native_currency_id), Some(()));
	}

	remove_allowed_currencies {
		let native_currency_id = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		AllowedCurrencies::<T>::insert(native_currency_id, ());

		let removed_currencies: Vec<CurrencyOf<T>> = vec![native_currency_id];
	}: remove_allowed_currencies(RawOrigin::Root, removed_currencies)
	verify {
		let native_currency_id = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		assert_eq!(AllowedCurrencies::<T>::get(native_currency_id), None);
	}

	approve {
		//allow currency
		let native_currency_id = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		AllowedCurrencies::<T>::insert(native_currency_id, ());

		//initialize accounts
		let owner: T::AccountId = account("Alice", 0, 0);
		let delegate: T::AccountId = account("Bob", 0, 0);

		//fund account
		let amount =  BalanceOf::<T>::from(1_000_000_000u32);
		<orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::deposit(native_currency_id, &owner, amount);

	}: approve(RawOrigin::Signed(owner), native_currency_id, delegate, amount)
	verify {
		let native_currency_id = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		let owner: T::AccountId = account("Alice", 0, 0);
		let delegate: T::AccountId = account("Bob", 0, 0);
		let amount =  BalanceOf::<T>::from(1_000_000_000u32);

		//check that the allowance was updated
		assert_eq!(TokenAllowance::<T>::allowance(native_currency_id, &owner, &delegate), amount);
	}

	transfer_from {
		//allow currency
		let native_currency_id = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		AllowedCurrencies::<T>::insert(native_currency_id, ());

		//initialize accounts
		let owner: T::AccountId = account("Alice", 0, 0);
		let delegate: T::AccountId = account("Bob", 0, 0);
		let destination: T::AccountId = account("Charlie", 0, 0);

		//fund accounts
		let amount =  BalanceOf::<T>::from(1_000_000_000u32);
		<orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::deposit(native_currency_id, &owner, amount);
		<orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::deposit(native_currency_id, &delegate, amount);

		//approve
		TokenAllowance::<T>::do_approve_transfer(native_currency_id, &owner, &delegate, amount);

	}: transfer_from(RawOrigin::Signed(delegate), native_currency_id, owner, destination, amount)
	verify {
		let native_currency_id = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		let owner: T::AccountId = account("Alice", 0, 0);
		let delegate: T::AccountId = account("Bob", 0, 0);
		let destination: T::AccountId = account("Charlie", 0, 0);
		let amount =  BalanceOf::<T>::from(1_000_000_000u32);

		//check that the allowance was updated
		assert_eq!(TokenAllowance::<T>::allowance(native_currency_id, &owner, &delegate), BalanceOf::<T>::from(0u32));

		//check that the balance was updated
		let destination_balance = <orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::free_balance(native_currency_id, &destination);
		assert_eq!(destination_balance, amount);
	}
}

impl_benchmark_test_suite!(TokenAllowance, crate::mock::ExtBuilder::build(), crate::mock::Test);
