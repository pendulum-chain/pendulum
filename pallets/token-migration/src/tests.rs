use crate::{mock::*, Error, Event, NextNonce, Paused, TotalMigrated};
use frame_support::{
	assert_noop, assert_ok,
	traits::{LockableCurrency, WithdrawReasons},
};
use sp_core::H160;
use sp_runtime::DispatchError;

fn base_address() -> H160 {
	H160::from_low_u64_be(0xBEEF)
}

#[test]
fn migrate_burns_amount_and_emits_event() {
	run_test(|| {
		let amount = 10 * UNIT;
		let issuance_before = Balances::total_issuance();

		assert_ok!(TokenMigration::migrate(
			RuntimeOrigin::signed(USER),
			amount,
			base_address()
		));

		// The amount is burned, not moved: issuance shrinks by exactly `amount`.
		assert_eq!(Balances::total_issuance(), issuance_before - amount);
		assert_eq!(Balances::free_balance(USER), USER_INITIAL_BALANCE - amount);
		assert_eq!(TotalMigrated::<Test>::get(), amount);

		System::assert_last_event(
			Event::MigrationInitiated {
				nonce: 0,
				who: USER,
				base_address: base_address(),
				amount,
			}
			.into(),
		);
	});
}

#[test]
fn nonces_are_unique_and_monotonic() {
	run_test(|| {
		for expected_nonce in 0u64..3 {
			assert_eq!(NextNonce::<Test>::get(), expected_nonce);
			assert_ok!(TokenMigration::migrate(
				RuntimeOrigin::signed(USER),
				UNIT,
				base_address()
			));
			System::assert_last_event(
				Event::MigrationInitiated {
					nonce: expected_nonce,
					who: USER,
					base_address: base_address(),
					amount: UNIT,
				}
				.into(),
			);
		}
		assert_eq!(NextNonce::<Test>::get(), 3);
		assert_eq!(TotalMigrated::<Test>::get(), 3 * UNIT);
	});
}

#[test]
fn migrate_fails_below_minimum_amount() {
	run_test(|| {
		assert_noop!(
			TokenMigration::migrate(RuntimeOrigin::signed(USER), UNIT - 1, base_address()),
			Error::<Test>::AmountBelowMinimum
		);
	});
}

#[test]
fn migrate_fails_for_zero_base_address() {
	run_test(|| {
		assert_noop!(
			TokenMigration::migrate(RuntimeOrigin::signed(USER), UNIT, H160::zero()),
			Error::<Test>::InvalidBaseAddress
		);
	});
}

#[test]
fn migrate_fails_with_insufficient_balance() {
	run_test(|| {
		assert_noop!(
			TokenMigration::migrate(
				RuntimeOrigin::signed(USER),
				USER_INITIAL_BALANCE + 1,
				base_address()
			),
			Error::<Test>::InsufficientBalance
		);
	});
}

#[test]
fn migrate_fails_if_dust_would_remain() {
	run_test(|| {
		// Leaves a remainder of ED - 1, which the balances pallet would reap as dust.
		let ed = ExistentialDeposit::get();
		assert_noop!(
			TokenMigration::migrate(
				RuntimeOrigin::signed(USER),
				USER_INITIAL_BALANCE - (ed - 1),
				base_address()
			),
			Error::<Test>::WouldLeaveDust
		);
	});
}

#[test]
fn migrate_entire_balance_works() {
	run_test(|| {
		assert_ok!(TokenMigration::migrate(
			RuntimeOrigin::signed(USER),
			USER_INITIAL_BALANCE,
			base_address()
		));
		assert_eq!(Balances::free_balance(USER), 0);
		assert_eq!(Balances::total_issuance(), 0);
		assert_eq!(TotalMigrated::<Test>::get(), USER_INITIAL_BALANCE);
	});
}

#[test]
fn migrate_fails_for_locked_funds() {
	run_test(|| {
		// Lock all but 5 UNIT; migrating more than the usable balance must fail.
		Balances::set_lock(
			*b"lock1234",
			&USER,
			USER_INITIAL_BALANCE - 5 * UNIT,
			WithdrawReasons::all(),
		);
		assert_noop!(
			TokenMigration::migrate(RuntimeOrigin::signed(USER), 10 * UNIT, base_address()),
			pallet_balances::Error::<Test>::LiquidityRestrictions
		);
		// Migrating within the usable balance still works.
		assert_ok!(TokenMigration::migrate(
			RuntimeOrigin::signed(USER),
			5 * UNIT,
			base_address()
		));
	});
}

#[test]
fn pause_blocks_migrations_and_unpause_restores_them() {
	run_test(|| {
		assert_ok!(TokenMigration::set_paused(RuntimeOrigin::root(), true));
		assert!(Paused::<Test>::get());
		System::assert_last_event(Event::MigrationPauseSet { paused: true }.into());

		assert_noop!(
			TokenMigration::migrate(RuntimeOrigin::signed(USER), UNIT, base_address()),
			Error::<Test>::MigrationsPaused
		);

		assert_ok!(TokenMigration::set_paused(RuntimeOrigin::root(), false));
		assert_ok!(TokenMigration::migrate(
			RuntimeOrigin::signed(USER),
			UNIT,
			base_address()
		));
	});
}

#[test]
fn set_paused_requires_pause_origin() {
	run_test(|| {
		assert_noop!(
			TokenMigration::set_paused(RuntimeOrigin::signed(USER), true),
			DispatchError::BadOrigin
		);
	});
}
