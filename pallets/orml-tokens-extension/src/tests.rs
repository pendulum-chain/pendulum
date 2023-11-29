use frame_support::{assert_ok, assert_err};
use orml_traits::MultiCurrency;
use crate::{mock::*, Error, AccountIdOf};
use crate::types::CurrencyDetails;
use spacewalk_primitives::CurrencyId;

fn get_balance(currency_id: CurrencyId, account: &AccountId) -> Balance {
	<orml_currencies::Pallet<Test> as MultiCurrency<AccountId>>::free_balance(
		currency_id,
		account,
	)
}

fn get_total_issuance(currency_id: CurrencyId) -> Balance {
	<orml_currencies::Pallet<Test> as MultiCurrency<AccountId>>::total_issuance(
		currency_id
	)
}

#[test]
fn can_create_currency_and_mint() {
	run_test(|| {
		let amount_minted= 10;
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
		assert_eq!(get_total_issuance(CurrencyId::Token(1)),amount_minted);

	})
}

#[test]
fn cannot_mint_if_not_owner() {
	run_test(|| {
		let amount_minted= 10;
		
		let owner_id = 0;
		let beneficiary_id = 1; 
		let not_owner_id = 2;
		assert_ok!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(owner_id),
			CurrencyId::Token(1),
		));

		assert_err!(crate::Pallet::<Test>::mint(
			RuntimeOrigin::signed(not_owner_id),
			CurrencyId::Token(1),
			beneficiary_id,
			amount_minted
		),Error::<Test>::NoPermission);

	})
}

#[test]
fn cannot_create_invalid_currency() {
	run_test(|| {
		let owner_id = 0;
		assert_err!(crate::Pallet::<Test>::create(
			RuntimeOrigin::signed(owner_id),
			CurrencyId::XCM(1),
		), Error::<Test>::NotOwnableCurrency);

	})
}

#[test]
fn can_mint_and_burn() {
	run_test(|| {
		let amount_minted= 10;
		let amount_burned= 5;
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

		assert_eq!(get_balance(CurrencyId::Token(1), &beneficiary_id), (amount_minted-amount_burned));
		assert_eq!(get_total_issuance(CurrencyId::Token(1)),(amount_minted-amount_burned));

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

		assert_eq!(crate::Pallet::<Test>::currency_details(
			CurrencyId::Token(1)
		), Some(CurrencyDetails::<AccountIdOf<Test>> {owner:new_owner_id, issuer: creator_id, admin: creator_id  }));

	})
}