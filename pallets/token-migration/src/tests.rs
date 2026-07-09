use crate::{mock::*, Error, Event, NextNonce, Paused, TotalMigrated, TreasuryDestination};
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
		// Only the treasury's balance remains in issuance after the user's is burned.
		assert_eq!(Balances::total_issuance(), TREASURY_INITIAL_BALANCE);
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

// ---------------------------------------------------------------- treasury migration

#[test]
fn set_treasury_destination_stores_and_emits() {
	run_test(|| {
		assert_ok!(TokenMigration::set_treasury_destination(RuntimeOrigin::root(), base_address()));
		assert_eq!(TreasuryDestination::<Test>::get(), Some(base_address()));
		System::assert_last_event(Event::TreasuryDestinationSet { base_address: base_address() }.into());
	});
}

#[test]
fn set_treasury_destination_rejects_zero_and_bad_origin() {
	run_test(|| {
		assert_noop!(
			TokenMigration::set_treasury_destination(RuntimeOrigin::root(), H160::zero()),
			Error::<Test>::InvalidBaseAddress
		);
		assert_noop!(
			TokenMigration::set_treasury_destination(RuntimeOrigin::signed(USER), base_address()),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn migrate_treasury_burns_from_treasury_and_emits_same_event_shape() {
	run_test(|| {
		let amount = 10 * UNIT;
		let issuance_before = Balances::total_issuance();
		assert_ok!(TokenMigration::set_treasury_destination(RuntimeOrigin::root(), base_address()));

		assert_ok!(TokenMigration::migrate_treasury(RuntimeOrigin::root(), amount));

		// Burned from the treasury account, issuance down by exactly `amount`.
		assert_eq!(Balances::total_issuance(), issuance_before - amount);
		assert_eq!(Balances::free_balance(TREASURY), TREASURY_INITIAL_BALANCE - amount);
		assert_eq!(Balances::free_balance(USER), USER_INITIAL_BALANCE);
		assert_eq!(TotalMigrated::<Test>::get(), amount);
		assert_eq!(NextNonce::<Test>::get(), 1);

		// Same event shape as a user migration, with who = the treasury account.
		System::assert_last_event(
			Event::MigrationInitiated { nonce: 0, who: TREASURY, base_address: base_address(), amount }.into(),
		);
	});
}

#[test]
fn treasury_and_user_migrations_share_the_nonce_space() {
	run_test(|| {
		assert_ok!(TokenMigration::set_treasury_destination(RuntimeOrigin::root(), base_address()));
		assert_ok!(TokenMigration::migrate(RuntimeOrigin::signed(USER), UNIT, base_address()));
		assert_ok!(TokenMigration::migrate_treasury(RuntimeOrigin::root(), UNIT));
		assert_ok!(TokenMigration::migrate(RuntimeOrigin::signed(USER), UNIT, base_address()));

		// Nonces are globally unique across both paths.
		assert_eq!(NextNonce::<Test>::get(), 3);
		assert_eq!(TotalMigrated::<Test>::get(), 3 * UNIT);
	});
}

#[test]
fn migrate_treasury_fails_without_destination() {
	run_test(|| {
		assert_noop!(
			TokenMigration::migrate_treasury(RuntimeOrigin::root(), UNIT),
			Error::<Test>::NoTreasuryDestination
		);
	});
}

#[test]
fn migrate_treasury_requires_authorized_origin() {
	run_test(|| {
		assert_ok!(TokenMigration::set_treasury_destination(RuntimeOrigin::root(), base_address()));
		assert_noop!(
			TokenMigration::migrate_treasury(RuntimeOrigin::signed(USER), UNIT),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn migrate_treasury_validates_amount() {
	run_test(|| {
		assert_ok!(TokenMigration::set_treasury_destination(RuntimeOrigin::root(), base_address()));
		// Below the minimum.
		assert_noop!(
			TokenMigration::migrate_treasury(RuntimeOrigin::root(), UNIT - 1),
			Error::<Test>::AmountBelowMinimum
		);
		// More than the treasury holds.
		assert_noop!(
			TokenMigration::migrate_treasury(RuntimeOrigin::root(), TREASURY_INITIAL_BALANCE + 1),
			Error::<Test>::InsufficientBalance
		);
	});
}

#[test]
fn migrate_treasury_keeps_treasury_alive() {
	run_test(|| {
		assert_ok!(TokenMigration::set_treasury_destination(RuntimeOrigin::root(), base_address()));
		// Draining the whole balance would reap the account; KeepAlive rejects it.
		assert_noop!(
			TokenMigration::migrate_treasury(RuntimeOrigin::root(), TREASURY_INITIAL_BALANCE),
			pallet_balances::Error::<Test>::Expendability
		);
		// Leaving at least the existential deposit works.
		let keep = TREASURY_INITIAL_BALANCE - ExistentialDeposit::get();
		assert_ok!(TokenMigration::migrate_treasury(RuntimeOrigin::root(), keep));
		assert_eq!(Balances::free_balance(TREASURY), ExistentialDeposit::get());
	});
}

#[test]
fn migrate_treasury_respects_pause() {
	run_test(|| {
		assert_ok!(TokenMigration::set_treasury_destination(RuntimeOrigin::root(), base_address()));
		assert_ok!(TokenMigration::set_paused(RuntimeOrigin::root(), true));
		assert_noop!(
			TokenMigration::migrate_treasury(RuntimeOrigin::root(), UNIT),
			Error::<Test>::MigrationsPaused
		);
		// Setting the destination is still allowed while paused (configuration).
		assert_ok!(TokenMigration::set_treasury_destination(RuntimeOrigin::root(), base_address()));
	});
}
