#![cfg(test)]
use crate::{
	mock::*,
	types::{Amount, CurrencyIdOf},
	BuyoutLimit, Buyouts, Config, Error, PriceGetter, ValidityError,
};
use frame_support::{assert_err, assert_noop, assert_ok};
use orml_traits::MultiCurrency;
use sp_arithmetic::{traits::One, FixedU128};
use sp_runtime::{
	traits::BadOrigin,
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	SaturatedConversion,
};

fn get_free_balance(currency_id: CurrencyIdOf<Test>, account: &AccountId) -> Balance {
	<orml_currencies::Pallet<Test> as MultiCurrency<AccountId>>::free_balance(currency_id, account)
}

fn run_to_block(new_block: <Test as frame_system::Config>::BlockNumber) {
	frame_system::Pallet::<Test>::set_block_number(new_block);
}

#[test]
fn buyout_using_dot_given_exchange_amount_in_dot_succeeds() {
	run_test(|| {
		let user = USER;
		let initial_user_dot_balance = get_free_balance(0u64, &user);
		let initial_treasury_dot_balance = get_free_balance(0u64, &TreasuryAccount::get());

		let initial_user_native_balance = get_free_balance(GetNativeCurrencyId::get(), &user);
		let initial_treasury_native_balance =
			get_free_balance(GetNativeCurrencyId::get(), &TreasuryAccount::get());

		assert_eq!(initial_user_native_balance, USERS_INITIAL_BALANCE);
		assert_eq!(initial_treasury_native_balance, TREASURY_INITIAL_BALANCE);

		let exchange_amount = 100 * UNIT;
		assert_ok!(crate::Pallet::<Test>::buyout(
			RuntimeOrigin::signed(user),
			0u64,
			Amount::Exchange(exchange_amount),
		));

		// Fetch prices from Oracle mock
		let basic_asset_price = <OracleMock as PriceGetter<CurrencyIdOf<Test>>>::get_price::<
			FixedU128,
		>(GetNativeCurrencyId::get())
		.expect("This is mocked so it should not fail");
		let exchange_asset_price =
			<OracleMock as PriceGetter<CurrencyIdOf<Test>>>::get_price::<FixedU128>(0u64)
				.expect("This is mocked so it should not fail");

		// Add fee to basic asset price
		let basic_asset_price_with_fee =
			basic_asset_price * (FixedU128::from(SellFee::get()) + FixedU128::one());

		// Calculate Native buyout amount
		let buyout_amount = crate::Pallet::<Test>::multiply_by_rational(
			exchange_amount,
			exchange_asset_price.into_inner(),
			basic_asset_price_with_fee.into_inner(),
		)
		.expect("This is mocked so it should not fail");

		let final_user_dot_balance = get_free_balance(0u64, &user);
		let final_user_native_balance = get_free_balance(GetNativeCurrencyId::get(), &user);

		let final_treasury_dot_balance = get_free_balance(0u64, &TreasuryAccount::get());
		let final_treasury_native_balance =
			get_free_balance(GetNativeCurrencyId::get(), &TreasuryAccount::get());

		assert_eq!(final_user_dot_balance, initial_user_dot_balance - exchange_amount);
		assert_eq!(final_treasury_dot_balance, initial_treasury_dot_balance + exchange_amount);

		assert_eq!(final_user_native_balance, initial_user_native_balance + buyout_amount);
		assert_eq!(final_treasury_native_balance, initial_treasury_native_balance - buyout_amount);

		// Verify Buyout event was emitted
		assert!(System::events().iter().any(|record| matches!(
			record.event,
			TestEvent::TreasuryBuyoutExtension(crate::Event::Buyout { who, buyout_amount: amount, .. })
			if who == user && amount == buyout_amount
		)));
	});
}

#[test]
fn buyout_using_dot_given_buyout_amount_in_native_succeeds() {
	run_test(|| {
		let user = USER;
		let initial_user_dot_balance = get_free_balance(0u64, &user);
		let initial_treasury_dot_balance = get_free_balance(0u64, &TreasuryAccount::get());

		let initial_user_native_balance = get_free_balance(GetNativeCurrencyId::get(), &user);
		let initial_treasury_native_balance =
			get_free_balance(GetNativeCurrencyId::get(), &TreasuryAccount::get());

		assert_eq!(initial_user_native_balance, USERS_INITIAL_BALANCE);
		assert_eq!(initial_treasury_native_balance, TREASURY_INITIAL_BALANCE);

		let buyout_amount = 100 * UNIT;
		assert_ok!(crate::Pallet::<Test>::buyout(
			RuntimeOrigin::signed(user),
			0u64,
			Amount::Buyout(buyout_amount),
		));

		// Fetch prices from Oracle mock
		let basic_asset_price = <OracleMock as PriceGetter<CurrencyIdOf<Test>>>::get_price::<
			FixedU128,
		>(GetNativeCurrencyId::get())
		.expect("This is mocked so it should not fail");
		let exchange_asset_price =
			<OracleMock as PriceGetter<CurrencyIdOf<Test>>>::get_price::<FixedU128>(0u64)
				.expect("This is mocked so it should not fail");

		// Add fee to basic asset price
		let basic_asset_price_with_fee =
			basic_asset_price * (FixedU128::from(SellFee::get()) + FixedU128::one());

		// Calculate DOT exchange amount
		let exchange_amount = crate::Pallet::<Test>::multiply_by_rational(
			buyout_amount,
			basic_asset_price_with_fee.into_inner(),
			exchange_asset_price.into_inner(),
		)
		.expect("This is mocked so it should not fail");

		let final_user_dot_balance = get_free_balance(0u64, &user);
		let final_user_native_balance = get_free_balance(GetNativeCurrencyId::get(), &user);

		let final_treasury_dot_balance = get_free_balance(0u64, &TreasuryAccount::get());
		let final_treasury_native_balance =
			get_free_balance(GetNativeCurrencyId::get(), &TreasuryAccount::get());

		assert_eq!(final_user_dot_balance, initial_user_dot_balance - exchange_amount);
		assert_eq!(final_treasury_dot_balance, initial_treasury_dot_balance + exchange_amount);

		assert_eq!(final_user_native_balance, initial_user_native_balance + buyout_amount);
		assert_eq!(final_treasury_native_balance, initial_treasury_native_balance - buyout_amount);

		// Verify Buyout event was emitted
		assert!(System::events().iter().any(|record| matches!(
			record.event,
			TestEvent::TreasuryBuyoutExtension(crate::Event::Buyout { who, buyout_amount: amount, .. })
			if who == user && amount == buyout_amount
		)));
	});
}

#[test]
fn root_update_buyout_amount_limit_succeeds() {
	run_test(|| {
		let buyout_amount_limit = 200 * UNIT;
		assert_ok!(crate::Pallet::<Test>::update_buyout_limit(
			RuntimeOrigin::root(),
			Some(buyout_amount_limit.into()),
		));

		assert_eq!(BuyoutLimit::<Test>::get(), buyout_amount_limit.into());

		let buyout_amount_limit = None;
		assert_ok!(crate::Pallet::<Test>::update_buyout_limit(
			RuntimeOrigin::root(),
			buyout_amount_limit,
		));

		assert_eq!(BuyoutLimit::<Test>::get(), buyout_amount_limit);
	});
}

#[test]
fn user_update_buyout_amount_limit_fails() {
	run_test(|| {
		let user = USER;

		let buyout_amount_limit = 200 * UNIT;
		assert_noop!(
			crate::Pallet::<Test>::update_buyout_limit(
				RuntimeOrigin::signed(user),
				Some(buyout_amount_limit.into()),
			),
			BadOrigin
		);
	});
}

#[test]
fn attempt_buyout_with_wrong_currency_fails() {
	run_test(|| {
		let user = USER;
		let native_currency_id = GetNativeCurrencyId::get();
		let initial_user_native_balance = get_free_balance(native_currency_id, &user);
		let initial_treasury_native_balance =
			get_free_balance(native_currency_id, &TreasuryAccount::get());

		assert_eq!(initial_user_native_balance, USERS_INITIAL_BALANCE);
		assert_eq!(initial_treasury_native_balance, TREASURY_INITIAL_BALANCE);

		let buyout_amount = 100 * UNIT;
		assert_noop!(
			crate::Pallet::<Test>::buyout(
				RuntimeOrigin::signed(user),
				native_currency_id,
				Amount::Buyout(buyout_amount),
			),
			Error::<Test>::WrongAssetToBuyout
		);

		assert_eq!(initial_user_native_balance, USERS_INITIAL_BALANCE);
		assert_eq!(initial_treasury_native_balance, TREASURY_INITIAL_BALANCE);

		let exchange_amount = 100 * UNIT;
		assert_noop!(
			crate::Pallet::<Test>::buyout(
				RuntimeOrigin::signed(user),
				native_currency_id,
				Amount::Exchange(exchange_amount),
			),
			Error::<Test>::WrongAssetToBuyout
		);

		assert_eq!(initial_user_native_balance, USERS_INITIAL_BALANCE);
		assert_eq!(initial_treasury_native_balance, TREASURY_INITIAL_BALANCE);
	});
}

#[test]
fn buyout_with_previous_existing_buyouts_succeeds() {
	run_test(|| {
		let user = USER;
		let dot_currency_id = RelayChainCurrencyId::get();
		let exchange_amount = 100 * UNIT;

		// With buyout limit and buyouts of previous periods
		BuyoutLimit::<Test>::put(200 * UNIT);
		Buyouts::<Test>::insert(user, (100 * UNIT, 0));

		assert_ok!(crate::Pallet::<Test>::buyout(
			RuntimeOrigin::signed(user),
			dot_currency_id,
			Amount::Exchange(exchange_amount),
		));
	});
}

#[test]
fn attempt_buyout_after_buyout_limit_exceeded_fails() {
	run_test(|| {
		let user = USER;
		let dot_currency_id = RelayChainCurrencyId::get();
		let exchange_amount = 100 * UNIT;

		let current_block = frame_system::Pallet::<Test>::block_number().saturated_into::<u32>();

		// With buyout limit
		BuyoutLimit::<Test>::put(150 * UNIT);
		// Previous buyout at current_block
		Buyouts::<Test>::insert(user, (100 * UNIT, current_block));

		assert_eq!(Buyouts::<Test>::get(user), (100 * UNIT, current_block));

		// This buyout attempt should fail because the limit is exceeded for the current period
		assert_noop!(
			crate::Pallet::<Test>::buyout(
				RuntimeOrigin::signed(user),
				dot_currency_id,
				Amount::Exchange(exchange_amount),
			),
			Error::<Test>::BuyoutLimitExceeded
		);
	});
}

#[test]
fn buyout_after_buyout_limit_reset_succeeds() {
	run_test(|| {
		let user = USER;
		let dot_currency_id = RelayChainCurrencyId::get();
		let buyout_amount = 100 * UNIT;

		let current_block = frame_system::Pallet::<Test>::block_number().saturated_into::<u32>();

		// With buyout limit
		BuyoutLimit::<Test>::put(200 * UNIT);
		// Previous buyout at current_block
		Buyouts::<Test>::insert(user, (150 * UNIT, current_block));

		assert_eq!(Buyouts::<Test>::get(user), (150 * UNIT, current_block));

		let buyout_period: u32 = BuyoutPeriod::get();
		// Skip buyout_period + 1 blocks, when the initial buyout period has already passed
		run_to_block((current_block + buyout_period + 1).into());

		assert_ok!(crate::Pallet::<Test>::buyout(
			RuntimeOrigin::signed(user),
			dot_currency_id,
			Amount::Buyout(buyout_amount),
		));

		let new_current_block =
			frame_system::Pallet::<Test>::block_number().saturated_into::<u32>();
		// Buyouts should be reset and the total buyout amount should be equal to the last buyout amount
		assert_eq!(Buyouts::<Test>::get(user), (100 * UNIT, new_current_block));
	});
}

#[test]
fn attempt_buyout_with_insufficient_user_balance_fails() {
	run_test(|| {
		let user = USER;
		let dot_currency_id = RelayChainCurrencyId::get();
		let buyout_amount = 10000 * UNIT;

		// This buyout attempt should fail because the user balance is insufficient
		assert_noop!(
			crate::Pallet::<Test>::buyout(
				RuntimeOrigin::signed(user),
				dot_currency_id,
				Amount::Buyout(buyout_amount),
			),
			Error::<Test>::InsufficientAccountBalance
		);
	});
}

#[test]
fn attempt_buyout_with_insufficient_treasury_balance_fails() {
	run_test(|| {
		let user = USER;
		let native_currency_id = GetNativeCurrencyId::get();
		let dot_currency_id = RelayChainCurrencyId::get();
		let buyout_amount = 100 * UNIT;

		// Transfer all treasury balance to user just for testing purposes
		// Makes treasury balance insufficient
		assert_ok!(<<Test as Config>::Currency>::transfer(
			RuntimeOrigin::signed(TREASURY_ACCOUNT),
			user,
			native_currency_id,
			TREASURY_INITIAL_BALANCE
		));

		// This buyout attempt should fail because the treasury balance is insufficient
		assert_noop!(
			crate::Pallet::<Test>::buyout(
				RuntimeOrigin::signed(user),
				dot_currency_id,
				Amount::Buyout(buyout_amount),
			),
			Error::<Test>::InsufficientTreasuryBalance
		);
	});
}

mod signed_extension {
	use frame_support::{dispatch::DispatchInfo, weights::Weight};
	use sp_runtime::traits::SignedExtension;

	use crate::CheckBuyout;

	use super::*;

	pub fn info_from_weight(w: Weight) -> DispatchInfo {
		DispatchInfo { weight: w, ..Default::default() }
	}

	#[test]
	fn validate_skip_other_calls_succeeds() {
		run_test(|| {
			let buyout_call =
				RuntimeCall::TreasuryBuyoutExtension(crate::Call::update_buyout_limit {
					limit: None,
				});

			let check = CheckBuyout::<Test>::new();
			let info = info_from_weight(Weight::zero());
			assert_ok!(check.validate(&1, &buyout_call, &info, 0));
		});
	}

	#[test]
	fn validate_when_wrong_asset_fails() {
		run_test(|| {
			let user = USER;

			// Some unsupported assets for buyout
			let native_currency_id = GetNativeCurrencyId::get();
			let brz_currency_id = 4u64;

			// Call with unsupported asset
			for asset in [native_currency_id, brz_currency_id] {
				let buyout_call = RuntimeCall::TreasuryBuyoutExtension(crate::Call::buyout {
					asset,
					amount: Amount::Buyout(100 * UNIT),
				});

				let check = CheckBuyout::<Test>::new();
				let info = info_from_weight(Weight::zero());

				assert_err!(
					check.validate(&user, &buyout_call, &info, 1),
					TransactionValidityError::Invalid(InvalidTransaction::Custom(
						ValidityError::WrongAssetToBuyout.into()
					))
				);
			}
		});
	}

	#[test]
	fn validate_when_no_price_found_fails() {
		run_test(|| {
			let user = USER;
			// For currency id 2u64 there is no price defined in the mock in order to test this case
			let buyout_call = RuntimeCall::TreasuryBuyoutExtension(crate::Call::buyout {
				asset: 2u64,
				amount: Amount::Buyout(100 * UNIT),
			});

			let check = CheckBuyout::<Test>::new();
			let info = info_from_weight(Weight::zero());

			assert_err!(
				check.validate(&user, &buyout_call, &info, 1),
				TransactionValidityError::Invalid(InvalidTransaction::Custom(
					ValidityError::Math.into()
				))
			);
		});
	}

	#[test]
	fn validate_when_not_enough_to_buyout_fails() {
		run_test(|| {
			let user = USER;
			let dot_currency_id = RelayChainCurrencyId::get();
			let buyout_call = RuntimeCall::TreasuryBuyoutExtension(crate::Call::buyout {
				asset: dot_currency_id,
				amount: Amount::Buyout(1000 * UNIT),
			});

			let check = CheckBuyout::<Test>::new();
			let info = info_from_weight(Weight::zero());

			assert_err!(
				check.validate(&user, &buyout_call, &info, 1),
				TransactionValidityError::Invalid(InvalidTransaction::Custom(
					ValidityError::NotEnoughToBuyout.into()
				))
			);
		});
	}

	#[test]
	fn validate_when_buyout_limit_exceeded_fails() {
		run_test(|| {
			let user = USER;
			let dot_currency_id = RelayChainCurrencyId::get();

			let buyout_call = RuntimeCall::TreasuryBuyoutExtension(crate::Call::buyout {
				asset: dot_currency_id,
				amount: Amount::Buyout(100 * UNIT),
			});

			let current_block =
				frame_system::Pallet::<Test>::block_number().saturated_into::<u32>();

			// With buyout limit
			BuyoutLimit::<Test>::put(100 * UNIT);
			// Previous buyout at current_block
			Buyouts::<Test>::insert(&user, (80 * UNIT, current_block));

			let check = CheckBuyout::<Test>::new();
			let info = info_from_weight(Weight::zero());

			assert_err!(
				check.validate(&user, &buyout_call, &info, 1),
				TransactionValidityError::Invalid(InvalidTransaction::Custom(
					ValidityError::BuyoutLimitExceeded.into()
				))
			);
		});
	}

	#[test]
	fn validate_when_less_than_min_amount_to_buyout_fails() {
		run_test(|| {
			let user = USER;
			let dot_currency_id = RelayChainCurrencyId::get();

			let buyout_call = RuntimeCall::TreasuryBuyoutExtension(crate::Call::buyout {
				asset: dot_currency_id,
				amount: Amount::Buyout(10 * UNIT),
			});

			let check = CheckBuyout::<Test>::new();
			let info = info_from_weight(Weight::zero());

			assert_err!(
				check.validate(&user, &buyout_call, &info, 1),
				TransactionValidityError::Invalid(InvalidTransaction::Custom(
					ValidityError::LessThanMinBuyoutAmount.into()
				))
			);
		});
	}

	#[test]
	fn validate_succeeds() {
		run_test(|| {
			let user = USER;
			let dot_currency_id = RelayChainCurrencyId::get();

			let buyout_call = RuntimeCall::TreasuryBuyoutExtension(crate::Call::buyout {
				asset: dot_currency_id,
				amount: Amount::Buyout(100 * UNIT),
			});

			let check = CheckBuyout::<Test>::new();
			let info = info_from_weight(Weight::zero());

			assert_ok!(check.validate(&user, &buyout_call, &info, 1));
		});
	}
}
