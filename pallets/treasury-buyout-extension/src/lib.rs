#![deny(warnings)]
#![cfg_attr(test, feature(proc_macro_hygiene))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

pub mod default_weights;

#[cfg(test)]
mod tests;

mod types;

use crate::{
	default_weights::WeightInfo,
	types::{AccountIdOf, Amount, BalanceOf, CurrencyIdOf},
};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::{DispatchError, DispatchResult},
	ensure,
	sp_runtime::SaturatedConversion,
	traits::{Get, IsSubType},
};
use orml_traits::MultiCurrency;
pub use pallet::*;
use sp_arithmetic::per_things::Rounding;
use sp_runtime::{
	traits::{DispatchInfoOf, One, SignedExtension, Zero},
	transaction_validity::{
		InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransaction,
	},
	ArithmeticError, FixedPointNumber, FixedU128,
};
use sp_std::{fmt::Debug, marker::PhantomData};

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{pallet_prelude::*, sp_runtime::Permill};
	use frame_system::{ensure_root, ensure_signed, pallet_prelude::*};

	#[pallet::config]
	pub trait Config: frame_system::Config + orml_currencies::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Used for currency-related operations
		type Currency: MultiCurrency<AccountIdOf<Self>, CurrencyId = CurrencyIdOf<Self>>;

		/// Used for getting the treasury account
		#[pallet::constant]
		type TreasuryAccount: Get<AccountIdOf<Self>>;

		/// Buyout period in blocks
		#[pallet::constant]
		type BuyoutPeriod: Get<u32>;

		/// Fee from the native asset buyouts
		#[pallet::constant]
		type SellFee: Get<Permill>;

		/// Type that allows for checking if currency type is ownable by users
		type AllowedCurrencyIdVerifier: AllowedCurrencyIdVerifier<CurrencyIdOf<Self>>;

		/// Used for fetching prices of currencies from oracle
		type PriceGetter: PriceGetter<CurrencyIdOf<Self>>;

		/// Min amount of native token to buyout
		#[pallet::constant]
		type MinAmountToBuyout: Get<BalanceOf<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// Currency id of the relay chain
		#[cfg(feature = "runtime-benchmarks")]
		type RelayChainCurrencyId: Get<CurrencyIdOf<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Allows caller to buyout a given amount of native token.
		/// Caller can specify either buyout amount of native token that he wants or exchange amount of an allowed asset.
		///
		/// Parameters
		///
		/// - `origin`: Caller's origin.
		/// - `asset`: Exchange asset used for buyout of basic asset.
		/// - `amount`: Amount of basic asset to buyout or amount of asset to exchange.
		///
		/// Emits `Buyout` event when successful.
		#[pallet::call_index(0)]
		#[pallet::weight((<T as pallet::Config>::WeightInfo::buyout(), Pays::No))]
		pub fn buyout(
			origin: OriginFor<T>,
			asset: CurrencyIdOf<T>,
			amount: Amount<BalanceOf<T>>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			Self::do_buyout(who, asset, amount)?;
			Ok(().into())
		}

		/// Allows root to update the buyout limit.
		///
		/// Parameters
		///
		/// - `origin`: Origin must be root.
		/// - `limit`: New buyout limit. If None, then buyouts are not limited.
		///
		/// Emits `BuyoutLimitUpdated` event when successful.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::update_buyout_limit())]
		pub fn update_buyout_limit(
			origin: OriginFor<T>,
			limit: Option<BalanceOf<T>>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			match limit {
				Some(limit) => BuyoutLimit::<T>::put(limit),
				None => BuyoutLimit::<T>::kill(),
			}
			Self::deposit_event(Event::<T>::BuyoutLimitUpdated { limit });
			Ok(().into())
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Attempt to exchange native token to native token
		WrongAssetToBuyout,
		/// Buyout limit exceeded for the current period
		BuyoutLimitExceeded,
		/// One of transacted currencies is missing price information
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
		/// Buyout limit updated event
		BuyoutLimitUpdated { limit: Option<BalanceOf<T>> },
	}

	/// Stores limit amount user could by for a period.
	/// When `None` - buyouts are not limited
	#[pallet::storage]
	pub type BuyoutLimit<T: Config> = StorageValue<_, BalanceOf<T>, OptionQuery>;

	/// Stores amount of buyouts (amount, block number of last buyout)
	#[pallet::storage]
	pub type Buyouts<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountIdOf<T>, (BalanceOf<T>, u32), ValueQuery>;
}

impl<T: Config> Pallet<T> {
	/// Ensures that buyout limit is not exceeded for the current buyout period
	fn ensure_buyout_limit_not_exceeded(
		account_id: &AccountIdOf<T>,
		buyout_amount: BalanceOf<T>,
	) -> DispatchResult {
		if let Some(buyout_limit) = BuyoutLimit::<T>::get() {
			let buyout_period = T::BuyoutPeriod::get();
			// Get current block number
			let now = <frame_system::Pallet<T>>::block_number().saturated_into::<u32>();
			let current_period = now
				.checked_div(buyout_period)
				.and_then(|n| Some(n.saturating_mul(buyout_period)))
				.unwrap_or_default();
			let (mut buyouts, last_buyout) = Buyouts::<T>::get(account_id);

			if !buyouts.is_zero() && last_buyout < current_period {
				buyouts = Default::default();
				Buyouts::<T>::insert(account_id, (buyouts, now));
			};

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

	/// Ensures that asset is allowed for buyout
	/// The concrete implementation of AllowedCurrencyIdVerifier trait must be provided by the runtime
	fn ensure_allowed_asset_for_buyout(asset: &CurrencyIdOf<T>) -> DispatchResult {
		ensure!(
			T::AllowedCurrencyIdVerifier::is_allowed_currency_id(asset),
			Error::<T>::WrongAssetToBuyout
		);

		Ok(())
	}

	/// Updates buyouts storage for the account
	fn update_buyouts(account_id: &AccountIdOf<T>, buyout_amount: BalanceOf<T>) {
		if BuyoutLimit::<T>::get().is_some() {
			Buyouts::<T>::mutate(account_id, |(prev_buyouts, last)| {
				*prev_buyouts = *prev_buyouts + buyout_amount;
				*last = <frame_system::Pallet<T>>::block_number().saturated_into::<u32>();
			});
		}
	}

	/// Used for calculating amount of exchange asset user will get for buyout_amount of basic asset
	fn calc_amount_to_exchange(
		asset: CurrencyIdOf<T>,
		buyout_amount: BalanceOf<T>,
	) -> Result<BalanceOf<T>, DispatchError> {
		let basic_asset = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		ensure!(asset != basic_asset, Error::<T>::WrongAssetToBuyout);

		let (basic_asset_price, exchange_asset_price) = Self::fetch_prices((&basic_asset, &asset))?;

		// Add fee to the basic asset price
		let basic_asset_price_with_fee =
			basic_asset_price * (FixedU128::from(T::SellFee::get()) + FixedU128::one());

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

	/// Used for calculating buyout amount of basic asset user will get for exchange_amount of exchange asset
	fn calc_buyout_amount(
		asset: CurrencyIdOf<T>,
		exchange_amount: BalanceOf<T>,
	) -> Result<BalanceOf<T>, DispatchError> {
		let basic_asset = <T as orml_currencies::Config>::GetNativeCurrencyId::get();

		ensure!(asset != basic_asset, Error::<T>::WrongAssetToBuyout);

		let (basic_asset_price, exchange_asset_price) = Self::fetch_prices((&basic_asset, &asset))?;

		// Add fee to the basic asset price
		let basic_asset_price_with_fee =
			basic_asset_price * (FixedU128::from(T::SellFee::get()) + FixedU128::one());

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

	/// Used for splitting calculations of amount based on the input given
	/// If user's call contains buyout amount, then exchange amount is calculated and viceversa
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
		Self::ensure_allowed_asset_for_buyout(&asset)?;

		let basic_asset = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		let (buyout_amount, exchange_amount) = Self::split_to_buyout_and_exchange(asset, amount)?;

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

	/// Used for calculating exchange amount based on buyout amount(a), price of basic asset with fee(b) and price of exchange asset(c)
	/// or buyout amount based on exchange amount(a), price of exchange asset(b) and price of basic asset with fee(c)
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

	/// Used for fetching asset prices
	/// The concrete implementation of PriceGetter trait must be provided by the runtime e.g. oracle pallet
	fn fetch_prices(
		assets: (&CurrencyIdOf<T>, &CurrencyIdOf<T>),
	) -> Result<(FixedU128, FixedU128), DispatchError> {
		let basic_asset_price: FixedU128 = T::PriceGetter::get_price::<FixedU128>(*assets.0)
			.map_err(|_| Error::<T>::NoPrice)?
			.into();
		let exchange_asset_price: FixedU128 = T::PriceGetter::get_price::<FixedU128>(*assets.1)
			.map_err(|_| Error::<T>::NoPrice)?
			.into();
		Ok((basic_asset_price, exchange_asset_price))
	}
}

/// Used for checking if asset is allowed for buyout
/// The concrete implementation of this trait must be provided by the runtime
pub trait AllowedCurrencyIdVerifier<CurrencyId>
where
	CurrencyId: Clone + PartialEq + Eq + Debug,
{
	fn is_allowed_currency_id(currency_id: &CurrencyId) -> bool;
}

/// Used for fetching prices of assets
/// This trait must be implemented by the runtime e.g. oracle pallet
pub trait PriceGetter<CurrencyId>
where
	CurrencyId: Clone + PartialEq + Eq + Debug,
{
	/// Gets a current price for a given currency
	fn get_price<FixedNumber: FixedPointNumber + One + Zero + Debug + TryFrom<FixedU128>>(
		currency_id: CurrencyId,
	) -> Result<FixedNumber, sp_runtime::DispatchError>;
}

/// Buyout validity errors
#[repr(u8)]
pub enum ValidityError {
	/// Account balance is too low to make buyout
	NotEnoughToBuyout = 0,
	/// Math error
	Math = 1,
	/// Buyout limit exceeded
	BuyoutLimitExceeded = 2,
	/// Amount to buyout less than min amount
	LessThanMinBuyoutAmount = 3,
	/// Wrong asset
	WrongAssetToBuyout = 4,
}

impl From<ValidityError> for u8 {
	fn from(err: ValidityError) -> Self {
		err as u8
	}
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, scale_info::TypeInfo)]
pub struct CheckBuyout<T: Config + Send + Sync + scale_info::TypeInfo>(PhantomData<T>)
where
	<T as frame_system::Config>::RuntimeCall: IsSubType<Call<T>>;

impl<T: Config + Send + Sync + scale_info::TypeInfo> Debug for CheckBuyout<T>
where
	<T as frame_system::Config>::RuntimeCall: IsSubType<Call<T>>,
{
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "CheckBuyout")
	}

	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

impl<T: Config + Send + Sync + scale_info::TypeInfo> Default for CheckBuyout<T>
where
	<T as frame_system::Config>::RuntimeCall: IsSubType<Call<T>>,
{
	fn default() -> Self {
		Self(PhantomData)
	}
}

impl<T: Config + Send + Sync + scale_info::TypeInfo> CheckBuyout<T>
where
	<T as frame_system::Config>::RuntimeCall: IsSubType<Call<T>>,
{
	pub fn new() -> Self {
		Self(PhantomData)
	}
}

impl<T: Config + Send + Sync + scale_info::TypeInfo> SignedExtension for CheckBuyout<T>
where
	<T as frame_system::Config>::RuntimeCall: IsSubType<Call<T>>,
{
	const IDENTIFIER: &'static str = "CheckBuyout";
	type AccountId = AccountIdOf<T>;
	type Call = T::RuntimeCall;
	type AdditionalSigned = ();
	type Pre = ();

	fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
		Ok(())
	}

	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		call: &Self::Call,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		self.validate(who, call, info, len)
			.map(|_| Self::Pre::default())
			.map_err(Into::into)
	}

	/// Checks:
	/// - asset is allowed for buyout
	/// - buyout amount is greater or equal `MinAmountToBuyout`
	/// - `who` has enough balance to make buyout
	/// - buyout limit is not exceeded for `who`
	fn validate(
		&self,
		who: &Self::AccountId,
		call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> TransactionValidity {
		if let Some(local_call) = call.is_sub_type() {
			if let Call::buyout { asset, amount } = local_call {
				Pallet::<T>::ensure_allowed_asset_for_buyout(asset).map_err(|_| {
					InvalidTransaction::Custom(ValidityError::WrongAssetToBuyout.into())
				})?;

				let (buyout_amount, exchange_amount) =
					Pallet::<T>::split_to_buyout_and_exchange(*asset, *amount)
						.map_err(|_| InvalidTransaction::Custom(ValidityError::Math.into()))?;

				ensure!(
					buyout_amount >= T::MinAmountToBuyout::get(),
					InvalidTransaction::Custom(ValidityError::LessThanMinBuyoutAmount.into())
				);

				let free_balance = T::Currency::free_balance(*asset, who);

				ensure!(
					free_balance >= exchange_amount,
					InvalidTransaction::Custom(ValidityError::NotEnoughToBuyout.into())
				);

				Pallet::<T>::ensure_buyout_limit_not_exceeded(who, buyout_amount).map_err(
					|_| InvalidTransaction::Custom(ValidityError::BuyoutLimitExceeded.into()),
				)?;
			}
		}

		Ok(ValidTransaction::default())
	}
}
