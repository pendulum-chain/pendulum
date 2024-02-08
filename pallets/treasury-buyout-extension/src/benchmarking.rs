#![allow(warnings)]
#![cfg(feature = "runtime-benchmarks")]

use super::{Pallet as TreasuryBuyoutExtension, *};
use crate::types::{AccountIdOf, BalanceOf, CurrencyIdOf};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, Vec};
use frame_support::assert_ok;
use frame_system::RawOrigin;
use sp_std::prelude::*;

// Mint some tokens to caller and treasury accounts
fn set_up_accounts<T: Config>(caller_account: &AccountIdOf<T>, treasury_account: &AccountIdOf<T>) {
	let token_currency_id = T::RelayChainCurrencyId::get();
	let native_currency_id = <T as orml_currencies::Config>::GetNativeCurrencyId::get();

	let amount: BalanceOf<T> = 1_000_000_000_000_000u128.try_into().unwrap_or_default();

	assert_ok!(<<T as pallet::Config>::Currency as MultiCurrency::<AccountIdOf<T>>>::deposit(
		token_currency_id,
		&caller_account,
		amount
	));

	assert_ok!(<<T as pallet::Config>::Currency as MultiCurrency::<AccountIdOf<T>>>::deposit(
		native_currency_id,
		&treasury_account,
		amount
	));
}

benchmarks! {
	buyout {
		let token_currency_id = T::RelayChainCurrencyId::get();
		let native_currency_id = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		let caller_account = account("Caller", 0, 0);
		let treasury_account = <T as pallet::Config>::TreasuryAccount::get();
		set_up_accounts::<T>(&caller_account, &treasury_account);
		let origin = RawOrigin::Signed(caller_account.clone());
		let limit: BalanceOf<T> = 100_000_000_000_000u128.try_into().unwrap_or_default();
		// Add token to allowed currencies for buyout
		AllowedCurrencies::<T>::insert(token_currency_id, ());
		BuyoutLimit::<T>::put(limit);
		// Set previous buyout limit to 0
		Buyouts::<T>::insert(caller_account.clone(), (BalanceOf::<T>::default(), 0));

	}: buyout(origin, token_currency_id, Amount::Buyout(100_000_000_000_000u128.try_into().unwrap_or_default()))
	verify{
		assert_eq!(
			<orml_currencies::Pallet<T> as MultiCurrency::<AccountIdOf<T>>>::free_balance(native_currency_id, &caller_account),
			100_000_000_000_000u128.try_into().unwrap_or_default()
		);
	}

	update_buyout_limit {
	}: update_buyout_limit(RawOrigin::Root, Some(100_000_000_000_000u128.try_into().unwrap_or_default()))

	update_allowed_assets {
		let token_currency_id = T::RelayChainCurrencyId::get();
		let max_allowed_currency_updates = T::MaxAllowedCurrencyUpdates::get();

		// Creating a vector of updates with alternating add and remove operations
		let mut updates = Vec::with_capacity(max_allowed_currency_updates.try_into().unwrap_or_default());
		for i in 0..max_allowed_currency_updates {
			if i % 2 == 0 {
				updates.push(BuyoutAssetUpdate::Add(token_currency_id));
			} else {
				updates.push(BuyoutAssetUpdate::Remove(token_currency_id));
			}
		}
	}: update_allowed_assets(RawOrigin::Root, updates)
}

impl_benchmark_test_suite!(
	TreasuryBuyoutExtension,
	crate::mock::ExtBuilder::build(),
	crate::mock::Test
);
