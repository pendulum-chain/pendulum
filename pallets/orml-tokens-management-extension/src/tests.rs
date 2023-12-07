use crate::{mock::*, types::CurrencyDetails, Config, AccountIdOf, Error};
use frame_support::{assert_err, assert_ok, traits::Get};
use orml_traits::{MultiCurrency, MultiReservableCurrency};
use spacewalk_primitives::CurrencyId;

fn get_balance(currency_id: CurrencyId, account: &AccountId) -> Balance {
	<orml_currencies::Pallet<Test> as MultiCurrency<AccountId>>::free_balance(currency_id, account)
}

fn get_reserved_balance(currency_id: CurrencyId, account: &AccountId) -> Balance {
	<orml_currencies::Pallet<Test> as MultiReservableCurrency<AccountId>>::reserved_balance(currency_id, account)
}

fn get_total_issuance(currency_id: CurrencyId) -> Balance {
	<orml_currencies::Pallet<Test> as MultiCurrency<AccountId>>::total_issuance(currency_id)
}

#[test]
fn can_create_currency_and_mint() {
	run_test(|| {
		let amount_minted = 10;
		let owner_id = USER_0;
		let beneficiary_id = USER_1;
		let currency_id = CurrencyId::Token(1);
		let deposit = <Test as Config>::AssetDeposit::get();

		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(owner_id),
			currency_id,
		));

		assert_eq!(get_reserved_balance(<Test as Config>::DepositCurrency::get(), &owner_id), deposit);

		assert_ok!(crate::Pallet::<Test>::mint(
			RuntimeOrigin::signed(owner_id),
			currency_id,
			beneficiary_id,
			amount_minted
		));

		assert_eq!(get_balance(currency_id, &beneficiary_id), amount_minted);
		assert_eq!(get_total_issuance(currency_id), amount_minted);
	})
}


#[test]
fn cannot_create_if_not_enough_balance_for_deposit() {
	run_test(|| {
		let owner_id = USER_3;
		let currency_id = CurrencyId::Token(1);

		assert_err!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(owner_id),
			currency_id,
		), Error::<Test>::InsufficientBalance);

	})
}

#[test]
fn cannot_mint_if_not_owner() {
	run_test(|| {
		let amount_minted = 10;
		let currency_id = CurrencyId::Token(1);
		let owner_id = USER_0;
		let beneficiary_id = USER_1;
		let not_owner_id = USER_2;

		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(owner_id),
			currency_id,
		));

		assert_err!(
			crate::Pallet::<Test>::mint(
				RuntimeOrigin::signed(not_owner_id),
				currency_id,
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
		let owner_id = USER_0;

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
		let owner_id = USER_0;
		let beneficiary_id = USER_1;

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
		let creator_id = USER_0;
		let new_owner_id = USER_1;
		let currency_id = CurrencyId::Token(1);

		let deposit = <Test as Config>::AssetDeposit::get();
		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(creator_id),
			currency_id,
		));

		let reserved_balance_owner_before = get_reserved_balance(<Test as Config>::DepositCurrency::get(), &creator_id);
		let reserved_balance_new_owner_before = get_reserved_balance(<Test as Config>::DepositCurrency::get(), &new_owner_id);

		assert_ok!(crate::Pallet::<Test>::transfer_ownership(
			RuntimeOrigin::signed(creator_id),
			currency_id,
			new_owner_id
		));

		assert_eq!(get_reserved_balance(<Test as Config>::DepositCurrency::get(), &creator_id), (reserved_balance_owner_before - deposit));
		assert_eq!(get_reserved_balance(<Test as Config>::DepositCurrency::get(), &new_owner_id), (reserved_balance_new_owner_before + deposit));

		assert_eq!(
			crate::Pallet::<Test>::currency_details(currency_id),
			Some(CurrencyDetails::<AccountIdOf<Test>, Balance> {
				owner: new_owner_id,
				issuer: creator_id,
				admin: creator_id,
				deposit
			})
		);
	})
}

#[test]
fn cannot_change_ownership_if_not_owner() {
	run_test(|| {
		let creator_id = USER_0;
		let new_owner_id = USER_1;
		let fake_creator_id=USER_2;
		let currency_id = CurrencyId::Token(1);

		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(creator_id),
			currency_id,
		));

		assert_err!(crate::Pallet::<Test>::transfer_ownership(
			RuntimeOrigin::signed(fake_creator_id),
			currency_id,
			new_owner_id
		),Error::<Test>::NoPermission);
	})
}

#[test]
fn root_can_change_ownership() {
	run_test(|| {
		let creator_id = USER_0;
		let new_owner_id = USER_1;
		let deposit = <Test as Config>::AssetDeposit::get();
		let currency_id = CurrencyId::Token(1);

		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(creator_id),
			currency_id,
		));

		let reserved_balance_owner_before = get_reserved_balance(<Test as Config>::DepositCurrency::get(), &creator_id);
		let reserved_balance_new_owner_before = get_reserved_balance(<Test as Config>::DepositCurrency::get(), &new_owner_id);

		assert_ok!(crate::Pallet::<Test>::force_transfer_ownership(
			RuntimeOrigin::root(),
			currency_id,
			new_owner_id
		));

		assert_eq!(get_reserved_balance(<Test as Config>::DepositCurrency::get(), &creator_id), (reserved_balance_owner_before - deposit));
		assert_eq!(get_reserved_balance(<Test as Config>::DepositCurrency::get(), &new_owner_id), (reserved_balance_new_owner_before + deposit));

		assert_eq!(
			crate::Pallet::<Test>::currency_details(currency_id),
			Some(CurrencyDetails::<AccountIdOf<Test>, Balance> {
				owner: new_owner_id,
				issuer: creator_id,
				admin: creator_id,
				deposit
			})
		);
	})
}

#[test]
fn owner_can_set_managers() {
	run_test(|| {
		let creator_id = USER_0;
		let new_admin= USER_1;
		let new_issuer= USER_2;
		let deposit = <Test as Config>::AssetDeposit::get();
		let currency_id = CurrencyId::Token(1);

		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(creator_id),
			currency_id,
		));

		assert_ok!(crate::Pallet::<Test>::set_managers(
			RuntimeOrigin::signed(creator_id),
			currency_id,
			new_admin,
			new_issuer
		));

		assert_eq!(
			crate::Pallet::<Test>::currency_details(currency_id),
			Some(CurrencyDetails::<AccountIdOf<Test>, Balance> {
				owner: creator_id,
				issuer: new_issuer,
				admin: new_admin,
				deposit
			})
		);
	})
}

#[test]
fn cannot_set_managers_if_not_owner() {
	run_test(|| {
		let creator_id = 0;
		let other_id =1;
		let new_admin = 10;
		let new_issuer = 10;
		let currency_id = CurrencyId::Token(1);

		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(creator_id),
			currency_id,
		));

		assert_err!(crate::Pallet::<Test>::set_managers(
			RuntimeOrigin::signed(other_id),
			currency_id,
			new_admin,
			new_issuer
		), Error::<Test>::NoPermission);
	})
}