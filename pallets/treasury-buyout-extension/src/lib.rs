#![deny(warnings)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

pub mod default_weights;

#[cfg(test)]
mod tests;

mod types;

pub use pallet::*;
use crate::types::{Amount, BUYOUT_LIMIT_PERIOD_IN_SEC};
use orml_traits::MultiCurrency;

#[allow(type_alias_bounds)]
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[allow(type_alias_bounds)]
pub(crate) type CurrencyIdOf<T> = <<T as orml_currencies::Config>::MultiCurrency as MultiCurrency<
	<T as frame_system::Config>::AccountId,
>>::CurrencyId;

#[allow(type_alias_bounds)]
type BalanceOf<T: Config> =
	<<T as Config>::Currency as MultiCurrency<AccountIdOf<T>>>::Balance;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use frame_support::{pallet_prelude::*, traits::UnixTime, sp_runtime::{traits::Zero, FixedU128, ArithmeticError, Permill, PerThing}};
    use sp_arithmetic::per_things::Rounding;
    use frame_system::{ensure_root, ensure_signed, pallet_prelude::*, WeightInfo};
    use oracle::OracleKey;

    #[pallet::config]
    pub trait Config: frame_system::Config + orml_currencies::Config + oracle::Config {

        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type Currency: MultiCurrency<AccountIdOf<Self>, CurrencyId = CurrencyIdOf<Self>>;

        #[pallet::constant]
		type TreasuryAccount: Get<Self::AccountId>;

        type UnixTime: UnixTime;

        #[pallet::constant]
        type SellFee: Get<Permill>;

        // might be needed?
        //type AssetRegistry: Inspect;

        #[pallet::constant]
        type MinAmountToBuyout: Get<BalanceOf<Self>>;

        type WeightInfo: WeightInfo;

        //Going to get rid of this
        type MyUnsignedFixedPoint: PerThing + From<u32> + Into<FixedU128>;

    }

    #[pallet::pallet]
	pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T> {

        #[pallet::call_index(0)]
        //TODO add weight
        //#[pallet::weight((T::WeightInfo::buyout(), Pays::No))]
        #[pallet::weight(10_000 + T::DbWeight::get().writes(1).ref_time())]
        pub fn buyout(
            origin: OriginFor<T>,
            asset: CurrencyIdOf<T>,
            amount: Amount<BalanceOf<T>>,
        ) -> DispatchResultWithPostInfo {
            let who = ensure_signed(origin)?;
            Self::do_buyout(who, asset, amount)?;
            Ok(().into())
        }

        #[pallet::call_index(1)]
        //TODO add weight
        // #[pallet::weight(T::WeightInfo::update_buyout_limit())]
        #[pallet::weight(10_000 + T::DbWeight::get().writes(1).ref_time())]
        pub fn update_buyout_limit(
            origin: OriginFor<T>,
            limit: Option<BalanceOf<T>>,
        ) -> DispatchResultWithPostInfo {
            ensure_root(origin)?;
            match limit {
                Some(limit) => BuyoutLimit::<T>::put(limit),
                None => BuyoutLimit::<T>::kill(),
            }

            Ok(().into())
        }
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Attempt to exchange native token to native token
        WrongAssetToBuyout,
        /// Daily buyout limit exceeded
        BuyoutLimitExceeded,
        /// One of transacted currencies is missing price information
        /// or the price is outdated
        NoPrice,
        /// The treasury balance is too low for an operation
        InsufficientTreasuryBalance,
        /// The account balance is too low for an operation
        InsufficientAccountBalance,
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Buyout event
        Buyout {
            who: AccountIdOf<T>,
            buyout_amount: BalanceOf<T>,
            asset: CurrencyIdOf<T>,
            exchange_amount: BalanceOf<T>,
        },
    }

    /// Stores limit amount user could by for a period.
    /// When `None` - buyouts not limited
    #[pallet::storage]
    pub type BuyoutLimit<T: Config> = StorageValue<_, BalanceOf<T>, OptionQuery>;

    /// Stores amount of buyouts (amount, timestamp of last buyout)
    #[pallet::storage]
    pub type Buyouts<T: Config> =
        StorageMap<_, Blake2_128Concat, AccountIdOf<T>, (BalanceOf<T>, u64), ValueQuery>;

    impl<T: Config> Pallet<T> {

        fn ensure_buyout_limit_not_exceeded(
            account_id: &AccountIdOf<T>,
            buyout_amount: BalanceOf<T>,
        ) -> DispatchResult {
            if let Some(buyout_limit) = BuyoutLimit::<T>::get() {
                let now = T::UnixTime::now().as_secs();
                let current_period = (now / BUYOUT_LIMIT_PERIOD_IN_SEC) * BUYOUT_LIMIT_PERIOD_IN_SEC;
                let (mut buyouts, last_buyout) = Buyouts::<T>::get(account_id);
    
                if !buyouts.is_zero() && last_buyout < current_period {
                    buyouts = Default::default();
                    Buyouts::<T>::insert(account_id, (buyouts, now));
                };
    
                ensure!(
                    buyouts + buyout_amount <= buyout_limit,
                    Error::<T>::BuyoutLimitExceeded
                );
            }
    
            Ok(())
        }
    
        fn ensure_not_native_buyout(asset: &CurrencyIdOf<T>) -> DispatchResult {
            ensure!(
                asset != &<T as orml_currencies::Config>::GetNativeCurrencyId::get(),
                Error::<T>::WrongAssetToBuyout
            );
    
            Ok(())
        }
    
        fn update_buyouts(account_id: &AccountIdOf<T>, buyout_amount: BalanceOf<T>) {
            if BuyoutLimit::<T>::get().is_some() {
                Buyouts::<T>::mutate(account_id, |(prev_buyouts, last)| {
                    *prev_buyouts = *prev_buyouts + buyout_amount;
                    *last = T::UnixTime::now().as_secs();
                });
            }
        }
    
        fn calc_amount_to_exchange(
            asset: CurrencyIdOf<T>,
            buyout_amount:BalanceOf<T>,
        ) -> Result<BalanceOf<T>, DispatchError> {
            let basic_asset = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
            //TODO
            // eq_ensure!(
            //     asset != basic_asset,
            //     Error::<T>::WrongAssetToBuyout,
            //     "{}:{}. Exchange same assets forbidden",
            //     file!(),
            //     line!(),
            // );
    
            let basic_asset_price_with_fee = {
                let basic_asset_price = oracle::Pallet::<T>::get_price(OracleKey::ExchangeRate(basic_asset))?.into();
                basic_asset_price * (T::MyUnsignedFixedPoint::from(T::SellFee::get()) + T::MyUnsignedFixedPoint::one())
            };
            let exchange_asset_price = oracle::Pallet::<T>::get_price(OracleKey::ExchangeRate(asset))?.into();
    
            let exchange_amount = Self::multiply_by_rational(
                buyout_amount.into(),
                basic_asset_price_with_fee.into_inner(),
                exchange_asset_price.into_inner(),
            )
            .map(|n| n.try_into().ok())
            .flatten()
            .ok_or(ArithmeticError::Overflow.into());
    
            exchange_amount
        }
    
        fn calc_buyout_amount(
            asset: CurrencyIdOf<T>,
            exchange_amount: BalanceOf<T>,
        ) -> Result<BalanceOf<T>, DispatchError> {
            let basic_asset = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
            //TODO
            // eq_ensure!(
            //     asset != basic_asset,
            //     Error::<T>::WrongAssetToBuyout,
            //     "{}:{}. Exchange same assets forbidden",
            //     file!(),
            //     line!(),
            // );
    
            let basic_asset_price_with_fee = {
                let basic_asset_price = oracle::Pallet::<T>::get_price(OracleKey::ExchangeRate(basic_asset))?.into();
                (basic_asset_price * (T::MyUnsignedFixedPoint::from(T::SellFee::get()) + T::MyUnsignedFixedPoint::one()))
                    .into_inner()
            };
            let exchange_asset_price = oracle::Pallet::<T>::get_price(OracleKey::ExchangeRate(asset))?.into();
    
            let buyout_amount = Self::multiply_by_rational(
                exchange_amount.into(),
                exchange_asset_price,
                basic_asset_price_with_fee,
            )
            .map(|b| b.try_into().ok())
            .flatten()
            .ok_or(ArithmeticError::Overflow.into());
    
            buyout_amount
        }
    
        fn split_to_buyout_and_exchange(
            asset: CurrencyIdOf<T>,
            amount: Amount<BalanceOf<T>>,
        ) -> Result<(BalanceOf<T>, BalanceOf<T>), DispatchError> {
            match amount {
                Amount::Buyout(buyout_amount) => {
                    let exchange_amount = Self::calc_amount_to_exchange(asset, buyout_amount)?;
                    Ok((buyout_amount, exchange_amount))
                }
                Amount::Exchange(exchange_amount) => {
                    let buyout_amount = Self::calc_buyout_amount(asset, exchange_amount)?;
                    Ok((buyout_amount, exchange_amount))
                }
            }
        }
    
        fn do_buyout(who: AccountIdOf<T>, asset: CurrencyIdOf<T>, amount: Amount<BalanceOf<T>>) -> DispatchResult {
            Self::ensure_not_native_buyout(&asset)?;
            let basic_asset = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
            let (buyout_amount, exchange_amount) = Self::split_to_buyout_and_exchange(asset, amount)?;
            Self::ensure_buyout_limit_not_exceeded(&who, buyout_amount)?;
            let treasury_account_id = T::TreasuryAccount::get();
    
            Self::exchange(
                (&who, &treasury_account_id),
                (&asset, &basic_asset),
                (exchange_amount, buyout_amount),
            )
            .map_err(|(error, maybe_acc)| match maybe_acc {
                Some(acc) => {
                    if acc == treasury_account_id {
                        Error::<T>::InsufficientTreasuryBalance.into()
                    } else if acc == who {
                        Error::<T>::InsufficientAccountBalance.into()
                    } else {
                        error
                    }
                }
                _ => error,
            })?;
    
            Self::update_buyouts(&who, buyout_amount);
            Self::deposit_event(Event::<T>::Buyout {
                who,
                buyout_amount,
                asset,
                exchange_amount,
            });
    
            Ok(())
        }

        fn multiply_by_rational(
            a: impl Into<u128>,
            b: impl Into<u128>,
            c: impl Into<u128>,
        ) -> Option<u128> {
            // return a * b / c
            sp_runtime::helpers_128bit::multiply_by_rational_with_rounding(
                a.into(),
                b.into(),
                c.into(),
                Rounding::NearestPrefDown,
            )
        }

        //Not sure if this is going to be like this
        fn exchange(
            accounts: (&AccountIdOf<T>, &AccountIdOf<T>),
            assets: (&CurrencyIdOf<T>, &CurrencyIdOf<T>),
            values: (BalanceOf<T>, BalanceOf<T>),
        ) -> Result<(), (DispatchError, Option<T::AccountId>)> {
            if assets.0 == assets.1 {
                frame_support::log::error!(
                    "{}:{}. Exchange same assets. Who: {:?}, amounts: {:?}, asset {:?}.",
                    file!(),
                    line!(),
                    accounts,
                    values,
                    format!("{:?}", assets.0)
                );
                return Err((Error::<T>::WrongAssetToBuyout.into(), None));
            }
    
            if values.0.is_zero() && values.1.is_zero() || accounts.0 == accounts.1 {
                return Ok(());
            }
            
            //TODO use this to return in case of error and event
            let mut err_acc: Option<T::AccountId> = None;
            
            
            // Return early if both values are zero or accounts are the same
            if (values.0.is_zero() && values.1.is_zero()) || accounts.0 == accounts.1 {
                return Ok(());
            }

            // Transfer from user to treasury for the first asset
            if values.0 > Zero::zero() {
                let user_balance = T::Currency::free_balance(*assets.0, accounts.0);
                ensure!(user_balance >= values.0, Error::<T>::InsufficientAccountBalance);
                T::Currency::transfer(*assets.0, accounts.0, accounts.1, values.0)?;
            }

            // Update the treasury balance after the first transfer
            let treasury_balance = T::Currency::free_balance(*assets.1, accounts.1);

            // Transfer from treasury to user for the second asset
            if values.1 > Zero::zero() {
                ensure!(treasury_balance >= values.1, Error::<T>::InsufficientTreasuryBalance);
                T::Currency::transfer(*assets.1, accounts.1, accounts.0, values.1)?;
            }

            // TODO Deposit an event for the exchange
            // Self::deposit_event(Event::Exchange(
            //     accounts.0.clone(),
            //     assets.0,
            //     values.0,
            //     accounts.1.clone(),
            //     assets.1,
            //     values.1,
            // ));

            Ok(())
        }
}

}

