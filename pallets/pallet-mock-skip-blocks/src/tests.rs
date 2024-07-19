#![cfg(test)]
#[cfg(feature = "instant-seal")]
use crate::{mock::*, Error};
#[cfg(feature = "instant-seal")]
use frame_support::{assert_noop, assert_ok, traits::Hooks};

#[cfg(feature = "instant-seal")]
#[test]
fn sets_and_reverts_block_number_success() {
	run_test(|| {
		// Initial block number
		assert_eq!(System::block_number(), 1);

		// Set desired block number
		let desired_block_number = 95;
		assert_ok!(crate::Pallet::<Test>::set_block_number(
			RuntimeOrigin::root(),
			desired_block_number
		));

		// Simulate block production
		System::on_initialize(1);
		crate::Pallet::<Test>::on_initialize(1);
		assert_eq!(System::block_number(), desired_block_number);

		crate::Pallet::<Test>::on_finalize(1);
		System::on_finalize(1);
		assert_eq!(System::block_number(), 1);

		// Advance to the next block
		System::set_block_number(2);
		System::on_initialize(2);
		crate::Pallet::<Test>::on_initialize(2);
		assert_eq!(System::block_number(), desired_block_number);

		crate::Pallet::<Test>::on_finalize(2);
		System::on_finalize(2);
		assert_eq!(System::block_number(), 2);
	});
}

#[cfg(feature = "instant-seal")]
#[test]
fn setting_block_number_to_less_than_current_fails() {
	run_test(|| {
		// Initial block number
		assert_eq!(System::block_number(), 1);

		// Attempt to set desired block number to a value less than the current block number
		assert_noop!(
			crate::Pallet::<Test>::set_block_number(RuntimeOrigin::root(), 0),
			Error::<Test>::InvalidBlockNumber
		);

		// Block number should remain unchanged
		assert_eq!(System::block_number(), 1);
	});
}
