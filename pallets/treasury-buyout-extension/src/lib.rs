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

use crate::types::{Amount, BUYOUT_LIMIT_PERIOD_IN_SEC};
use frame_support::{dispatch::DispatchError, sp_runtime::SaturatedConversion};
use orml_traits::MultiCurrency;
pub use pallet::*;
use sp_runtime::{
	traits::{One, Zero},
	FixedPointNumber,
};
use sp_std::fmt::Debug;

#[allow(type_alias_bounds)]
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[allow(type_alias_bounds)]
pub(crate) type CurrencyIdOf<T> =
	<<T as orml_currencies::Config>::MultiCurrency as MultiCurrency<
		<T as frame_system::Config>::AccountId,
	>>::CurrencyId;

#[allow(type_alias_bounds)]
type BalanceOf<T: Config> = <<T as Config>::Currency as MultiCurrency<AccountIdOf<T>>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		pallet_prelude::*,
		sp_runtime::{traits::Zero, ArithmeticError, FixedU128, Permill},
		traits::UnixTime,
	};
	use frame_system::{ensure_root, ensure_signed, pallet_prelude::*, WeightInfo};
	use sp_arithmetic::per_things::Rounding;

	#[pallet::config]
	pub trait Config: frame_system::Config + orml_currencies::Config + oracle::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Used for currency-related operations
		type Currency: MultiCurrency<AccountIdOf<Self>, CurrencyId = CurrencyIdOf<Self>>;

		/// Used for getting the treasury account
		#[pallet::constant]
		type TreasuryAccount: Get<Self::AccountId>;

		/// Timestamp provider
		type UnixTime: UnixTime;

		/// Fee from the native asset buyouts
		#[pallet::constant]
		type SellFee: Get<Permill>;

		// might be needed?
		//type AssetRegistry: Inspect;

		/// Used for fetching prices of currencies from oracle
		type PriceGetter: PriceGetter<CurrencyIdOf<Self>>;

		/// Min amount of native token to buyout
		#[pallet::constant]
		type MinAmountToBuyout: Get<BalanceOf<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
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
		/// Exchange failed
		ExchangeFailure,
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
		// Exchange event
		Exchange {
			from: AccountIdOf<T>,
			from_asset: CurrencyIdOf<T>,
			from_amount: BalanceOf<T>,
			to: AccountIdOf<T>,
			to_asset: CurrencyIdOf<T>,
			to_amount: BalanceOf<T>,
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
				let current_period = now
					.checked_div(BUYOUT_LIMIT_PERIOD_IN_SEC)
					.and_then(|n| Some(n.saturating_mul(BUYOUT_LIMIT_PERIOD_IN_SEC)))
					.unwrap_or_default();
				let (mut buyouts, last_buyout) = Buyouts::<T>::get(account_id);

				if !buyouts.is_zero() && last_buyout < current_period {
					buyouts = Default::default();
					Buyouts::<T>::insert(account_id, (buyouts, now));
				};

				// maybe I can do it easier than this
				ensure!(
					buyouts
						.saturated_into::<u128>()
						.saturating_add(buyout_amount.saturated_into::<u128>()) <=
						buyout_limit.saturated_into::<u128>(),
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
			buyout_amount: BalanceOf<T>,
		) -> Result<BalanceOf<T>, DispatchError> {
			let basic_asset = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
			ensure!(asset != basic_asset, Error::<T>::WrongAssetToBuyout);

			let (basic_asset_price, exchange_asset_price) =
				Self::fetch_prices((&basic_asset, &asset))?;

			// Add fee to the basic asset price
			let basic_asset_price_with_fee = basic_asset_price
				* (FixedU128::from(T::SellFee::get()) + FixedU128::one());

			let exchange_amount = Self::multiply_by_rational(
				buyout_amount.saturated_into::<u128>(),
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

			ensure!(asset != basic_asset, Error::<T>::WrongAssetToBuyout);

			let (basic_asset_price, exchange_asset_price) =
				Self::fetch_prices((&basic_asset, &asset))?;

			// Add fee to the basic asset price
			let basic_asset_price_with_fee = basic_asset_price
				* (FixedU128::from(T::SellFee::get()) + FixedU128::one());

			let buyout_amount = Self::multiply_by_rational(
				exchange_amount.saturated_into::<u128>(),
				exchange_asset_price.into_inner(),
				basic_asset_price_with_fee.into_inner(),
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
				},
				Amount::Exchange(exchange_amount) => {
					let buyout_amount = Self::calc_buyout_amount(asset, exchange_amount)?;
					Ok((buyout_amount, exchange_amount))
				},
			}
		}

		fn do_buyout(
			who: AccountIdOf<T>,
			asset: CurrencyIdOf<T>,
			amount: Amount<BalanceOf<T>>,
		) -> DispatchResult {
			Self::ensure_not_native_buyout(&asset)?;
			let basic_asset = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
			let (buyout_amount, exchange_amount) =
				Self::split_to_buyout_and_exchange(asset, amount)?;
			Self::ensure_buyout_limit_not_exceeded(&who, buyout_amount)?;
			let treasury_account_id = T::TreasuryAccount::get();

			// Start exchanging
			// Check for exchanging zero values and same accounts
			if exchange_amount.is_zero() && buyout_amount.is_zero() || who == treasury_account_id {
				return Ok(())
			}

			// Check both balances before transfer
			let user_balance = T::Currency::free_balance(asset, &who);
			let treasury_balance = T::Currency::free_balance(basic_asset, &treasury_account_id);

			if user_balance < exchange_amount {
				return Err(Error::<T>::InsufficientAccountBalance.into())
			}
			if treasury_balance < buyout_amount {
				return Err(Error::<T>::InsufficientTreasuryBalance.into())
			}

			// Transfer from user account to treasury then viceversa
			T::Currency::transfer(asset, &who, &treasury_account_id, exchange_amount)
				.map_err(|_| Error::<T>::ExchangeFailure)?;
			T::Currency::transfer(basic_asset, &treasury_account_id, &who, buyout_amount)
				.map_err(|_| Error::<T>::ExchangeFailure)?;

			Self::update_buyouts(&who, buyout_amount);
			Self::deposit_event(Event::<T>::Buyout { who, buyout_amount, asset, exchange_amount });

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

        // Use NoPrice error here maybe
		fn fetch_prices(
			assets: (&CurrencyIdOf<T>, &CurrencyIdOf<T>),
		) -> Result<(FixedU128, FixedU128), DispatchError> {
			let basic_asset_price: FixedU128 =
				T::PriceGetter::get_price::<FixedU128>(*assets.0)?.into();
			let exchange_asset_price: FixedU128 =
				T::PriceGetter::get_price::<FixedU128>(*assets.1)?.into();
			Ok((basic_asset_price, exchange_asset_price))
		}
	}
}

pub trait PriceGetter<CurrencyId>
where
	CurrencyId: Clone + PartialEq + Eq + Debug,
{
	/// Gets a current price for a given currency
	fn get_price<FixedNumber: FixedPointNumber + One + Zero + Debug>(
		currency_id: CurrencyId,
	) -> Result<FixedNumber, sp_runtime::DispatchError>;
}
