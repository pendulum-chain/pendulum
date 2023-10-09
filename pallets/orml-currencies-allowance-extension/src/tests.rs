use frame_support::{assert_err, assert_ok, error::BadOrigin, traits::Get};
use orml_traits::MultiCurrency;

use crate::{mock::*, AllowedCurrencies, Config, CurrencyOf, Error};

#[test]
fn should_add_allowed_currencies() {
	run_test(|| {
		let max_allowed_currencies: u32 = <Test as Config>::MaxAllowedCurrencies::get();
		let max_allowed_currencies: u64 = max_allowed_currencies as u64;
		let added_currencies = (0..max_allowed_currencies).collect::<Vec<u64>>();

		assert_ok!(TokenAllowance::add_allowed_currencies(RuntimeOrigin::root(), added_currencies));
		for i in 0..max_allowed_currencies {
			assert_eq!(AllowedCurrencies::<Test>::get(i), Some(()));
		}
	})
}

#[test]
fn should_remove_allowed_currencies() {
	run_test(|| {
		let max_allowed_currencies: u32 = <Test as Config>::MaxAllowedCurrencies::get();
		let max_allowed_currencies: u64 = max_allowed_currencies as u64;
		let mut added_currencies = (0..max_allowed_currencies).collect::<Vec<u64>>();

		assert_ok!(TokenAllowance::add_allowed_currencies(
			RuntimeOrigin::root(),
			added_currencies.clone()
		));
		for i in 0..max_allowed_currencies {
			assert_eq!(AllowedCurrencies::<Test>::get(i), Some(()));
		}

		let existing_currency: CurrencyOf<Test> =
			added_currencies.pop().expect("Should have a currency");
		assert_ok!(TokenAllowance::remove_allowed_currencies(
			RuntimeOrigin::root(),
			added_currencies.clone()
		));

		for i in 0..added_currencies.len() as u64 {
			assert_eq!(AllowedCurrencies::<Test>::get(i), None);
		}
		// The existing currency should remain
		assert_eq!(AllowedCurrencies::<Test>::get(existing_currency), Some(()));
	})
}

#[test]
fn should_not_exceed_allowed_currencies() {
	run_test(|| {
		let max_allowed_currencies: u32 = <Test as Config>::MaxAllowedCurrencies::get();
		let max_allowed_currencies: u64 = max_allowed_currencies as u64;
		let too_many_currencies = (0..max_allowed_currencies + 1).collect::<Vec<u64>>();

		// We can't add more than the maximum allowed currencies
		assert_err!(
			TokenAllowance::add_allowed_currencies(
				RuntimeOrigin::root(),
				too_many_currencies.clone()
			),
			Error::<Test>::ExceedsNumberOfAllowedCurrencies
		);

		assert_err!(
			TokenAllowance::remove_allowed_currencies(RuntimeOrigin::root(), too_many_currencies),
			Error::<Test>::ExceedsNumberOfAllowedCurrencies
		);

		// Fill the allowed currencies to the maximum
		let added_currencies = (0..max_allowed_currencies).collect::<Vec<u64>>();
		assert_ok!(TokenAllowance::add_allowed_currencies(
			RuntimeOrigin::root(),
			added_currencies.clone()
		));
		for i in 0..max_allowed_currencies {
			assert_eq!(AllowedCurrencies::<Test>::get(i), Some(()));
		}

		// Try to add a duplicate currency (should not fail because we don't store duplicates)
		let already_added_currency = added_currencies[0];
		assert_ok!(TokenAllowance::add_allowed_currencies(
			RuntimeOrigin::root(),
			vec![already_added_currency]
		));
		assert_eq!(AllowedCurrencies::<Test>::get(already_added_currency), Some(()));

		// Try to add a new distinct currency (should fail since we reached the maximum)
		let illegal_currency: CurrencyOf<Test> = max_allowed_currencies;
		assert_err!(
			TokenAllowance::add_allowed_currencies(RuntimeOrigin::root(), vec![illegal_currency]),
			Error::<Test>::ExceedsNumberOfAllowedCurrencies
		);
	})
}

#[test]
fn should_not_add_allowed_currencies_with_non_root_origin() {
	run_test(|| {
		let native_currency_id = <Test as orml_currencies::Config>::GetNativeCurrencyId::get();
		let added_currencies: Vec<CurrencyOf<Test>> = vec![native_currency_id];
		assert_err!(
			TokenAllowance::add_allowed_currencies(
				RuntimeOrigin::signed(1),
				added_currencies.clone()
			),
			BadOrigin
		);
		assert_err!(
			TokenAllowance::add_allowed_currencies(RuntimeOrigin::none(), added_currencies),
			BadOrigin
		);
	})
}

#[test]
fn should_not_remove_allowed_currencies_with_non_root_origin() {
	run_test(|| {
		let native_currency_id = <Test as orml_currencies::Config>::GetNativeCurrencyId::get();
		let added_currencies: Vec<CurrencyOf<Test>> = vec![native_currency_id];
		assert_err!(
			TokenAllowance::remove_allowed_currencies(
				RuntimeOrigin::signed(1),
				added_currencies.clone()
			),
			BadOrigin
		);
		assert_err!(
			TokenAllowance::remove_allowed_currencies(RuntimeOrigin::none(), added_currencies),
			BadOrigin
		);
	})
}

#[test]
fn should_return_allowance() {
	run_test(|| {
		let currency_id: <Test as orml_tokens::Config>::CurrencyId = 0;
		let owner: <Test as frame_system::Config>::AccountId = 0;
		let delegate: <Test as frame_system::Config>::AccountId = 1;
		let amount: <Test as orml_tokens::Config>::Balance = 1_000_000_000u32 as Balance;

		// Check allowance
		assert_eq!(TokenAllowance::allowance(currency_id, &owner, &delegate), 0);

		// Approve the amount
		assert_ok!(TokenAllowance::approve(
			RuntimeOrigin::signed(owner.clone()),
			currency_id,
			delegate.clone(),
			amount
		));

		// Check allowance again
		assert_eq!(TokenAllowance::allowance(currency_id, &owner, &delegate), amount);
	})
}

#[test]
fn should_approve_transfer() {
	run_test(|| {
		let currency_id: <Test as orml_tokens::Config>::CurrencyId = 0;
		let owner: <Test as frame_system::Config>::AccountId = 0;
		let delegate: <Test as frame_system::Config>::AccountId = 1;
		let amount: <Test as orml_tokens::Config>::Balance = 1_000_000_000u32 as Balance;

		// Will not work yet
		assert_err!(
			TokenAllowance::approve(
				RuntimeOrigin::signed(owner.clone()),
				currency_id,
				delegate.clone(),
				amount
			),
			Error::<Test>::CurrencyNotLive
		);

		// We need to add the currency first
		assert_ok!(TokenAllowance::add_allowed_currencies(
			RuntimeOrigin::root(),
			vec![currency_id]
		));

		// Should work now
		assert_ok!(TokenAllowance::approve(
			RuntimeOrigin::signed(owner.clone()),
			currency_id,
			delegate.clone(),
			amount
		));

		// Check allowance
		assert_eq!(TokenAllowance::allowance(currency_id, &owner, &delegate), amount);
	})
}

#[test]
fn should_transfer_from_for_approved_transfer() {
	run_test(|| {
		let currency_id: <Test as orml_tokens::Config>::CurrencyId = 0;
		let owner: <Test as frame_system::Config>::AccountId = 0;
		let delegate: <Test as frame_system::Config>::AccountId = 1;
		let destination: <Test as frame_system::Config>::AccountId = 2;
		let amount: <Test as orml_tokens::Config>::Balance = 1_000_000_000u32 as Balance;

		// Mint some tokens
		assert_ok!(Tokens::deposit(currency_id, &owner, amount));

		// Check the balances
		assert_eq!(Tokens::free_balance(currency_id, &owner), amount);
		assert_eq!(Tokens::free_balance(currency_id, &delegate), 0);
		assert_eq!(Tokens::free_balance(currency_id, &destination), 0);

		// We need to add the currency first
		assert_ok!(TokenAllowance::add_allowed_currencies(
			RuntimeOrigin::root(),
			vec![currency_id]
		));

		// Approve the amount
		assert_ok!(TokenAllowance::approve(
			RuntimeOrigin::signed(owner.clone()),
			currency_id,
			delegate.clone(),
			amount
		));

		// Transfer all of the approved amount
		assert_ok!(TokenAllowance::transfer_from(
			RuntimeOrigin::signed(delegate.clone()),
			currency_id,
			owner.clone(),
			destination.clone(),
			amount
		));

		// Check the balances
		assert_eq!(Tokens::free_balance(currency_id, &owner), 0);
		assert_eq!(Tokens::free_balance(currency_id, &delegate), 0);
		assert_eq!(Tokens::free_balance(currency_id, &destination), amount);
		// Check that the allowance is now empty since we transferred the whole amount
		assert_eq!(TokenAllowance::allowance(currency_id, &owner, &delegate), 0);

		// Test again but this time only using a partial amount of what was approved
		let partial_amount = amount / 2;
		assert_ok!(Tokens::deposit(currency_id, &owner, amount));
		assert_ok!(TokenAllowance::approve(
			RuntimeOrigin::signed(owner.clone()),
			currency_id,
			delegate.clone(),
			amount
		));
		assert_ok!(TokenAllowance::transfer_from(
			RuntimeOrigin::signed(delegate.clone()),
			currency_id,
			owner.clone(),
			destination.clone(),
			partial_amount
		));

		// Check the balances again
		assert_eq!(Tokens::free_balance(currency_id, &owner), amount - partial_amount);
		assert_eq!(Tokens::free_balance(currency_id, &delegate), 0);
		assert_eq!(Tokens::free_balance(currency_id, &destination), amount + partial_amount);
		// Check that the allowance is now reduced by the partial amount
		assert_eq!(
			TokenAllowance::allowance(currency_id, &owner, &delegate),
			amount - partial_amount
		);
	})
}

#[test]
fn should_not_transfer_from_without_approved_transfer() {
	run_test(|| {
		let currency_id: <Test as orml_tokens::Config>::CurrencyId = 0;
		let owner: <Test as frame_system::Config>::AccountId = 0;
		let delegate: <Test as frame_system::Config>::AccountId = 1;
		let destination: <Test as frame_system::Config>::AccountId = 2;
		let amount: <Test as orml_tokens::Config>::Balance = 1_000_000_000u32 as Balance;

		// Mint some tokens
		assert_ok!(Tokens::deposit(currency_id, &owner, amount));

		// Check the balances
		assert_eq!(Tokens::free_balance(currency_id, &owner), amount);
		assert_eq!(Tokens::free_balance(currency_id, &delegate), 0);
		assert_eq!(Tokens::free_balance(currency_id, &destination), 0);

		// We need to add the currency first
		assert_ok!(TokenAllowance::add_allowed_currencies(
			RuntimeOrigin::root(),
			vec![currency_id]
		));

		// Try to `transfer_from` without having approved the transfer
		assert_err!(
			TokenAllowance::transfer_from(
				RuntimeOrigin::signed(delegate.clone()),
				currency_id,
				owner.clone(),
				destination.clone(),
				amount
			),
			Error::<Test>::Unapproved
		);

		// Approve the amount
		assert_ok!(TokenAllowance::approve(
			RuntimeOrigin::signed(owner.clone()),
			currency_id,
			delegate.clone(),
			amount
		));

		// Try to `transfer_from` for amount larger than what was approved
		assert_err!(
			TokenAllowance::transfer_from(
				RuntimeOrigin::signed(delegate.clone()),
				currency_id,
				owner.clone(),
				destination.clone(),
				amount + 1
			),
			Error::<Test>::Unapproved
		);

		// Check the balances
		assert_eq!(Tokens::free_balance(currency_id, &owner), amount);
		assert_eq!(Tokens::free_balance(currency_id, &delegate), 0);
		assert_eq!(Tokens::free_balance(currency_id, &destination), 0);
	})
}

#[test]
fn should_transfer_from_while_keeping_infinite_allowance() {
	run_test(|| {
		let currency_id: <Test as orml_tokens::Config>::CurrencyId = 0;
		let owner: <Test as frame_system::Config>::AccountId = 0;
		let delegate: <Test as frame_system::Config>::AccountId = 1;
		let destination: <Test as frame_system::Config>::AccountId = 2;
		// We use the max value of u128 as the amount to approve because it represents an infinite allowance
		let allowance_amount: <Test as orml_tokens::Config>::Balance =
			<Test as orml_tokens::Config>::Balance::max_value();

		// Mint some tokens
		assert_ok!(Tokens::deposit(currency_id, &owner, allowance_amount));

		// Check the balances
		assert_eq!(Tokens::free_balance(currency_id, &owner), allowance_amount);
		assert_eq!(Tokens::free_balance(currency_id, &delegate), 0);
		assert_eq!(Tokens::free_balance(currency_id, &destination), 0);

		// We need to add the currency first
		assert_ok!(TokenAllowance::add_allowed_currencies(
			RuntimeOrigin::root(),
			vec![currency_id]
		));

		// Approve infinite spending
		assert_ok!(TokenAllowance::approve(
			RuntimeOrigin::signed(owner.clone()),
			currency_id,
			delegate.clone(),
			allowance_amount,
		));

		// Check the allowance of the delegate
		assert_eq!(TokenAllowance::allowance(currency_id, &owner, &delegate), allowance_amount);

		// Transfer the approved amount once
		assert_ok!(TokenAllowance::transfer_from(
			RuntimeOrigin::signed(delegate.clone()),
			currency_id,
			owner.clone(),
			destination.clone(),
			allowance_amount
		));

		// Check the balances
		assert_eq!(Tokens::free_balance(currency_id, &owner), 0);
		assert_eq!(Tokens::free_balance(currency_id, &delegate), 0);
		assert_eq!(Tokens::free_balance(currency_id, &destination), allowance_amount);

		// Check that the allowance of the delegate is still the same since it should be infinite
		assert_eq!(TokenAllowance::allowance(currency_id, &owner, &delegate), allowance_amount);

		// Move the tokens from `destination` to the `owner` again to avoid overflow but allow for testing the same amount again
		assert_ok!(Tokens::transfer(
			RuntimeOrigin::signed(destination.clone()),
			owner,
			currency_id,
			allowance_amount
		));

		// Transfer the approved amount again
		assert_ok!(TokenAllowance::transfer_from(
			RuntimeOrigin::signed(delegate.clone()),
			currency_id,
			owner.clone(),
			destination.clone(),
			allowance_amount
		));

		// Check the balances again
		assert_eq!(Tokens::free_balance(currency_id, &owner), 0);
		assert_eq!(Tokens::free_balance(currency_id, &delegate), 0);
		assert_eq!(Tokens::free_balance(currency_id, &destination), allowance_amount);
	})
}

#[test]
fn should_not_transfer_from_for_invalid_origin() {
	run_test(|| {
		let currency_id: <Test as orml_tokens::Config>::CurrencyId = 0;
		let owner: <Test as frame_system::Config>::AccountId = 0;
		let destination: <Test as frame_system::Config>::AccountId = 2;
		let amount: <Test as orml_tokens::Config>::Balance = 1_000_000_000u32 as Balance;

		assert_err!(
			TokenAllowance::transfer_from(
				RuntimeOrigin::none(),
				currency_id,
				owner.clone(),
				destination.clone(),
				amount
			),
			BadOrigin
		);
		assert_err!(
			TokenAllowance::transfer_from(
				RuntimeOrigin::root(),
				currency_id,
				owner.clone(),
				destination.clone(),
				amount
			),
			BadOrigin
		);
	})
}

#[test]
fn should_not_transfer_from_for_invalid_currency() {
	run_test(|| {
		let currency_id: <Test as orml_tokens::Config>::CurrencyId = 0;
		let owner: <Test as frame_system::Config>::AccountId = 0;
		let delegate: <Test as frame_system::Config>::AccountId = 1;
		let destination: <Test as frame_system::Config>::AccountId = 2;
		let amount: <Test as orml_tokens::Config>::Balance = 1_000_000_000u32 as Balance;

		assert_err!(
			TokenAllowance::transfer_from(
				RuntimeOrigin::signed(delegate.clone()),
				currency_id,
				owner.clone(),
				destination.clone(),
				amount
			),
			Error::<Test>::CurrencyNotLive
		);
	})
}
