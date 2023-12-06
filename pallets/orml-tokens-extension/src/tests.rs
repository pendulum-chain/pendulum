use crate::{mock::*, types::CurrencyDetails, AccountIdOf, Error};
use frame_support::{assert_err, assert_ok};
use orml_traits::MultiCurrency;
use spacewalk_primitives::CurrencyId;

fn get_balance(currency_id: CurrencyId, account: &AccountId) -> Balance {
	<orml_currencies::Pallet<Test> as MultiCurrency<AccountId>>::free_balance(currency_id, account)
}

fn get_total_issuance(currency_id: CurrencyId) -> Balance {
	<orml_currencies::Pallet<Test> as MultiCurrency<AccountId>>::total_issuance(currency_id)
}

#[test]
fn can_create_currency_and_mint() {
	run_test(|| {
		let amount_minted = 10;
		let beneficiary_id = 1;
		let owner_id = 0;
		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(owner_id),
			CurrencyId::Token(1),
		));

		assert_ok!(crate::Pallet::<Test>::mint(
			RuntimeOrigin::signed(owner_id),
			CurrencyId::Token(1),
			beneficiary_id,
			amount_minted
		));

		assert_eq!(get_balance(CurrencyId::Token(1), &beneficiary_id), amount_minted);
		assert_eq!(get_total_issuance(CurrencyId::Token(1)), amount_minted);
	})
}

#[test]
fn cannot_mint_if_not_owner() {
	run_test(|| {
		let amount_minted = 10;

		let owner_id = 0;
		let beneficiary_id = 1;
		let not_owner_id = 2;
		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(owner_id),
			CurrencyId::Token(1),
		));

		assert_err!(
			crate::Pallet::<Test>::mint(
				RuntimeOrigin::signed(not_owner_id),
				CurrencyId::Token(1),
				beneficiary_id,
				amount_minted
			),
			Error::<Test>::NoPermission
		);
	})
}

#[test]
fn cannot_create_invalid_currency() {
	run_test(|| {
		let owner_id = 0;
		assert_err!(
			crate::Pallet::<Test>::create(RuntimeOrigin::signed(owner_id), CurrencyId::XCM(1),),
			Error::<Test>::NotOwnableCurrency
		);
	})
}

#[test]
fn can_mint_and_burn() {
	run_test(|| {
		let amount_minted = 10;
		let amount_burned = 5;
		let beneficiary_id = 1;
		let owner_id = 0;
		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(owner_id),
			CurrencyId::Token(1),
		));

		assert_ok!(crate::Pallet::<Test>::mint(
			RuntimeOrigin::signed(owner_id),
			CurrencyId::Token(1),
			beneficiary_id,
			amount_minted
		));

		assert_ok!(crate::Pallet::<Test>::burn(
			RuntimeOrigin::signed(owner_id),
			CurrencyId::Token(1),
			beneficiary_id,
			amount_burned
		));

		assert_eq!(
			get_balance(CurrencyId::Token(1), &beneficiary_id),
			(amount_minted - amount_burned)
		);
		assert_eq!(get_total_issuance(CurrencyId::Token(1)), (amount_minted - amount_burned));
	})
}

#[test]
fn can_change_ownership() {
	run_test(|| {
		let creator_id = 0;
		let new_owner_id = 2;
		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(creator_id),
			CurrencyId::Token(1),
		));

		assert_ok!(crate::Pallet::<Test>::transfer_ownership(
			RuntimeOrigin::signed(creator_id),
			CurrencyId::Token(1),
			new_owner_id
		));

		assert_eq!(
			crate::Pallet::<Test>::currency_details(CurrencyId::Token(1)),
			Some(CurrencyDetails::<AccountIdOf<Test>> {
				owner: new_owner_id,
				issuer: creator_id,
				admin: creator_id
			})
		);
	})
}

#[test]
fn cannot_change_ownership_if_not_owner() {
	run_test(|| {
		let creator_id = 0;
		let new_owner_id = 2;
		let fake_creator_id=3;
		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(creator_id),
			CurrencyId::Token(1),
		));

		assert_err!(crate::Pallet::<Test>::transfer_ownership(
			RuntimeOrigin::signed(fake_creator_id),
			CurrencyId::Token(1),
			new_owner_id
		),Error::<Test>::NoPermission);
	})
}

#[test]
fn root_can_change_ownership() {
	run_test(|| {
		let creator_id = 0;
		let new_owner_id = 2;
		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(creator_id),
			CurrencyId::Token(1),
		));

		assert_ok!(crate::Pallet::<Test>::force_transfer_ownership(
			RuntimeOrigin::root(),
			CurrencyId::Token(1),
			new_owner_id
		));

		assert_eq!(
			crate::Pallet::<Test>::currency_details(CurrencyId::Token(1)),
			Some(CurrencyDetails::<AccountIdOf<Test>> {
				owner: new_owner_id,
				issuer: creator_id,
				admin: creator_id
			})
		);
	})
}

#[test]
fn owner_can_set_managers() {
	run_test(|| {
		let creator_id = 0;
		let new_admin= 10;
		let new_issuer= 10;

		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(creator_id),
			CurrencyId::Token(1),
		));

		assert_ok!(crate::Pallet::<Test>::set_managers(
			RuntimeOrigin::signed(creator_id),
			CurrencyId::Token(1),
			new_admin,
			new_issuer
		));

		assert_eq!(
			crate::Pallet::<Test>::currency_details(CurrencyId::Token(1)),
			Some(CurrencyDetails::<AccountIdOf<Test>> {
				owner: creator_id,
				issuer: new_issuer,
				admin: new_admin
			})
		);
	})
}

#[test]
fn cannot_set_managers_if_not_owner() {
	run_test(|| {
		let creator_id = 0;
		let other_id=1;
		let new_admin= 10;
		let new_issuer= 10;

		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(creator_id),
			CurrencyId::Token(1),
		));

		assert_err!(crate::Pallet::<Test>::set_managers(
			RuntimeOrigin::signed(other_id),
			CurrencyId::Token(1),
			new_admin,
			new_issuer
		), Error::<Test>::NoPermission);
	})
}