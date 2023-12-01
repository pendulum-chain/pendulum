#![allow(warnings)]
use super::{Pallet as TokenExtension, *};

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite};
use frame_system::RawOrigin;
use sp_std::prelude::*;
use sp_runtime::traits::Get;
use frame_support::assert_ok;
use orml_traits::arithmetic::{One, Zero};

const AMOUNT_MINTED: u64= 0;


benchmarks!{
    
    create {
        let token_currency_id = <T as Config>::GetTestCurrency::get();
        let origin = RawOrigin::Signed(account("Tester", 0, 0));
	}: _(origin,token_currency_id)
    verify {
        assert!(crate::Pallet::<T>::currency_details(token_currency_id).is_some());
	}

    mint { 
        let token_currency_id = <T as Config>::GetTestCurrency::get();
        let origin = RawOrigin::Signed(account("Tester", 0, 0));
        let destination = account::<AccountIdOf<T>>("Receiver", 0, 0);
        assert_ok!(TokenExtension::<T>::create(origin.clone().into(), token_currency_id));

	}: _(origin,token_currency_id, destination.clone(),BalanceOf::<T>::one())
    verify {
        assert_eq!(<orml_currencies::Pallet<T> as MultiCurrency<AccountIdOf<T>>>::total_balance(token_currency_id, &destination), BalanceOf::<T>::one());
	}

    burn {  
        let token_currency_id = <T as Config>::GetTestCurrency::get();
        let origin = RawOrigin::Signed(account("Tester", 0, 0));
        let destination = account::<AccountIdOf<T>>("Receiver", 0, 0);
        assert_ok!(TokenExtension::<T>::create(origin.clone().into(), token_currency_id));
        assert_ok!(TokenExtension::<T>::mint(origin.clone().into(), token_currency_id, destination.clone(), BalanceOf::<T>::one()));

	}: _(origin,token_currency_id, destination.clone(),BalanceOf::<T>::one())
    verify {
        assert_eq!(<orml_currencies::Pallet<T> as MultiCurrency<AccountIdOf<T>>>::total_balance(token_currency_id, &destination), BalanceOf::<T>::zero());
	}

    transfer_ownership {  
        let token_currency_id = <T as Config>::GetTestCurrency::get();
        let origin = RawOrigin::Signed(account("Tester", 0, 0));
        let new_owner = account::<AccountIdOf<T>>("NewOwner", 0, 0);
        assert_ok!(TokenExtension::<T>::create(origin.clone().into(), token_currency_id));

	}: _(origin,token_currency_id, new_owner)
    verify {
        assert!(crate::Pallet::<T>::currency_details(token_currency_id).is_some());
	}

    set_managers {  
        let token_currency_id = <T as Config>::GetTestCurrency::get();
        let origin = RawOrigin::Signed(account("Tester", 0, 0));
        let new_issuer = account::<AccountIdOf<T>>("Issuer", 0, 0);
        let new_admin = account::<AccountIdOf<T>>("Admin", 0, 0);
        assert_ok!(TokenExtension::<T>::create(origin.clone().into(), token_currency_id));

	}: _(origin,token_currency_id, new_issuer, new_admin)
    verify {
        assert!(crate::Pallet::<T>::currency_details(token_currency_id).is_some());
	}



}

impl_benchmark_test_suite!(TokenExtension, crate::mock::ExtBuilder::build(), crate::mock::Test);