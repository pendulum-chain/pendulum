#![allow(warnings)]
use super::{Pallet as TokenExtension, *};

use crate::types::{AccountIdOf, BalanceOf, CurrencyOf};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_support::assert_ok;
use frame_system::RawOrigin;
use orml_traits::{
	arithmetic::{One, Zero},
	MultiCurrency,
};
use sp_runtime::traits::Get;
use sp_std::prelude::*;

use sp_runtime::Saturating;
const AMOUNT_MINTED: u32 = 2000000000;
const AMOUNT_BURNED: u32 = 1000000000;

fn get_test_currency<T: Config>() -> CurrencyOf<T> {
	<T as orml_currencies::Config>::GetNativeCurrencyId::get()
}

// mint some tokens to the account
fn set_up_account<T: Config>(account: &AccountIdOf<T>) {
	let token_currency_id = get_test_currency::<T>();
	let deposit_amount = <T as crate::Config>::AssetDeposit::get();
	assert_ok!(<orml_currencies::Pallet<T> as MultiCurrency<AccountIdOf<T>>>::deposit(
		token_currency_id,
		&account,
		deposit_amount.saturating_mul(deposit_amount)
	));
}

benchmarks! {
	create {
		let token_currency_id = get_test_currency::<T>();
		let test_account = account("Tester", 0, 0);
		set_up_account::<T>(&test_account);
		let origin = RawOrigin::Signed(test_account);
	}: _(origin,token_currency_id)
	verify {
		assert!(crate::Pallet::<T>::currency_details(token_currency_id).is_some());
	}

	mint {
		let token_currency_id = get_test_currency::<T>();
		let test_account = account("Tester", 0, 0);
		set_up_account::<T>(&test_account);
		let origin = RawOrigin::Signed(test_account);
		let destination = account::<AccountIdOf<T>>("Receiver", 0, 0);
		assert_ok!(TokenExtension::<T>::create(origin.clone().into(), token_currency_id));

	}: _(origin,token_currency_id, destination.clone(),AMOUNT_MINTED.into())
	verify {
		assert_eq!(<orml_currencies::Pallet<T> as MultiCurrency<AccountIdOf<T>>>::total_balance(token_currency_id, &destination), AMOUNT_MINTED.into());
	}

	burn {
		let token_currency_id = get_test_currency::<T>();
		let test_account = account("Tester", 0, 0);
		set_up_account::<T>(&test_account);
		let origin = RawOrigin::Signed(test_account);
		let destination = account::<AccountIdOf<T>>("Receiver", 0, 0);
		assert_ok!(TokenExtension::<T>::create(origin.clone().into(), token_currency_id));
		assert_ok!(TokenExtension::<T>::mint(origin.clone().into(), token_currency_id, destination.clone(), AMOUNT_MINTED.into()));

	}: _(origin,token_currency_id, destination.clone(),AMOUNT_BURNED.into())
	verify {
		assert_eq!(<orml_currencies::Pallet<T> as MultiCurrency<AccountIdOf<T>>>::total_balance(token_currency_id, &destination), (AMOUNT_MINTED-AMOUNT_BURNED).into());
	}

	transfer_ownership {
		let token_currency_id = get_test_currency::<T>();
		let test_account = account("Tester", 0, 0);
		set_up_account::<T>(&test_account);
		let origin = RawOrigin::Signed(test_account);
		let new_owner = account::<AccountIdOf<T>>("NewOwner", 0, 0);
		set_up_account::<T>(&new_owner);
		assert_ok!(TokenExtension::<T>::create(origin.clone().into(), token_currency_id));

	}: _(origin,token_currency_id, new_owner)
	verify {
		assert!(crate::Pallet::<T>::currency_details(token_currency_id).is_some());
	}

	set_managers {
		let token_currency_id = get_test_currency::<T>();
		let test_account = account("Tester", 0, 0);
		set_up_account::<T>(&test_account);
		let origin = RawOrigin::Signed(test_account);
		let new_issuer = account::<AccountIdOf<T>>("Issuer", 0, 0);
		let new_admin = account::<AccountIdOf<T>>("Admin", 0, 0);
		assert_ok!(TokenExtension::<T>::create(origin.clone().into(), token_currency_id));

	}: _(origin,token_currency_id, new_issuer, new_admin)
	verify {
		assert!(crate::Pallet::<T>::currency_details(token_currency_id).is_some());
	}
}

impl_benchmark_test_suite!(TokenExtension, crate::mock::ExtBuilder::build(), crate::mock::Test);
