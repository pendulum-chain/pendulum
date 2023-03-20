use frame_support::{assert_err, assert_ok, error::BadOrigin};

use crate::{mock::*, AllowedCurrencies, CurrencyOf};

#[test]
fn should_add_allowed_currencies() {
	run_test(|| {
		let native_currency_id = <Test as orml_currencies::Config>::GetNativeCurrencyId::get();
		let added_currencies: Vec<CurrencyOf<Test>> = vec![native_currency_id];
		assert_ok!(TokenAllowance::add_allowed_currencies(RuntimeOrigin::root(), added_currencies));
		assert_eq!(AllowedCurrencies::<Test>::get(native_currency_id), Some(()));
	})
}

#[test]
fn should_remove_allowed_currencies() {
	run_test(|| {
		let native_currency_id = <Test as orml_currencies::Config>::GetNativeCurrencyId::get();
		let added_currencies: Vec<CurrencyOf<Test>> = vec![native_currency_id];
		assert_ok!(TokenAllowance::add_allowed_currencies(
			RuntimeOrigin::root(),
			added_currencies.clone()
		));
		assert_eq!(AllowedCurrencies::<Test>::get(native_currency_id), Some(()));

		assert_ok!(TokenAllowance::remove_allowed_currencies(
			RuntimeOrigin::root(),
			added_currencies
		));
		assert_eq!(AllowedCurrencies::<Test>::get(native_currency_id), None);
	})
}

#[test]
fn should_not_add_allowed_currencies_with_not_root_origin() {
	run_test(|| {
		let native_currency_id = <Test as orml_currencies::Config>::GetNativeCurrencyId::get();
		let added_currencies: Vec<CurrencyOf<Test>> = vec![native_currency_id];
		let result =
			TokenAllowance::add_allowed_currencies(RuntimeOrigin::signed(1), added_currencies);
		assert_err!(result, BadOrigin);
	})
}

#[test]
fn should_not_remove_allowed_currencies_with_not_root_origin() {
	run_test(|| {
		let native_currency_id = <Test as orml_currencies::Config>::GetNativeCurrencyId::get();
		let added_currencies: Vec<CurrencyOf<Test>> = vec![native_currency_id];
		let result =
			TokenAllowance::remove_allowed_currencies(RuntimeOrigin::signed(1), added_currencies);
		assert_err!(result, BadOrigin);
	})
}
