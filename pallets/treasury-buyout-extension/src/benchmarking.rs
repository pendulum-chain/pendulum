#![allow(warnings)]
#[cfg(feature = "runtime-benchmarks")]
use super::{Pallet as TreasuryBuyoutExtension, *};
use crate::types::{AccountIdOf, BalanceOf, CurrencyIdOf};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_support::assert_ok;
use frame_system::RawOrigin;
use sp_std::prelude::*;
use spacewalk_primitives::CurrencyId;

pub trait Config:
    orml_currencies::Config + orml_tokens::Config + currency::Config + crate::Config
{
}

fn get_test_currency<T: Config>() -> CurrencyIdOf<T> {
    <T as currency::Config>::GetRelayChainCurrencyId::get()
}

// Mint some tokens to the caller and treasury accounts
fn set_up_accounts<T: Config>(caller_account: &AccountIdOf<T>, treasury_account: &AccountIdOf<T>) {
	let token_currency_id = get_test_currency::<T>();
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
		let token_currency_id = get_test_currency::<T>();
		let native_currency_id = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		let caller_account = account("Caller", 0, 0);
		let treasury_account = <T as pallet::Config>::TreasuryAccount::get();
		set_up_accounts::<T>(&caller_account, &treasury_account);
		let origin = RawOrigin::Signed(caller_account.clone());
		let limit: BalanceOf<T> = 100_000_000_000_000u128.try_into().unwrap_or_default();
		BuyoutLimit::<T>::put(limit);
		// Set previous buyout limit to 0
		Buyouts::<T>::insert(caller_account.clone(), (BalanceOf::<T>::default(), 0));

	}: _(origin, token_currency_id, Amount::Buyout(100_000_000_000_000u128.try_into().unwrap_or_default()))
	verify{
		assert_eq!(
			<orml_currencies::Pallet<T> as MultiCurrency::<AccountIdOf<T>>>::free_balance(native_currency_id, &caller_account),
			100_000_000_000_000u128.try_into().unwrap_or_default()
		);
	}

	update_buyout_limit {
	}: _(RawOrigin::Root, Some(100_000_000_000_000u128.try_into().unwrap_or_default()))
}
