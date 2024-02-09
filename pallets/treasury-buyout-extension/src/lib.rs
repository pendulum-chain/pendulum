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
	transactional,
};
use orml_traits::MultiCurrency;
pub use pallet::*;
use sp_arithmetic::{
	per_things::Rounding,
	traits::{CheckedAdd, Saturating},
};
use sp_runtime::{
	traits::{DispatchInfoOf, One, SignedExtension, Zero},
	transaction_validity::{
		InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransaction,
	},
	ArithmeticError, FixedPointNumber, FixedU128,
};
use sp_std::{fmt::Debug, marker::PhantomData, vec::Vec};

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

		/// Buyout period in blocks in which a caller can buyout up to the amount limit stored in `BuyoutLimit`
		/// When attempting to buyout after this period, the buyout limit is reset for the caller
		#[pallet::constant]
		type BuyoutPeriod: Get<u32>;

		/// Fee from the native asset buyouts
		#[pallet::constant]
		type SellFee: Get<Permill>;

		/// Used for fetching prices of currencies from oracle
		type PriceGetter: PriceGetter<CurrencyIdOf<Self>>;

		/// Min amount of native token to buyout
		#[pallet::constant]
		type MinAmountToBuyout: Get<BalanceOf<Self>>;

		/// Maximum number of allowed currencies for buyout
		#[pallet::constant]
		type MaxAllowedBuyoutCurrencies: Get<u32>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// Currency id of the relay chain, only used in benchmarks
		#[cfg(feature = "runtime-benchmarks")]
		type RelayChainCurrencyId: Get<CurrencyIdOf<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::error]
	pub enum Error<T> {
		/// Storage clearing of `AllowedCurrencies` failed
		StorageClearingFailure,
		/// Attempt to add native token to allowed assets
		NativeTokenNotAllowed,
		/// Exceeds number of allowed currencies for buyout
		ExceedsNumberOfAllowedCurrencies,
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
		/// Less than minimum amoount allowed for buyout
		LessThanMinBuyoutAmount,
		/// Attempt to use treasury account for buyout
		BuyoutWithTreasuryAccount,
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

		/// Updated allowed assets for buyout event
		AllowedAssetsForBuyoutUpdated {
			allowed_assets: Vec<CurrencyIdOf<T>>,
		},
	}

	/// Stores buyout limit amount user could buy for a period of `BuyoutPeriod` blocks.
	/// Each user can buyout up to this amount in a period. After each period passes, buyout limit is reset
	/// When `None` - buyouts are not limited
	#[pallet::storage]
	pub type BuyoutLimit<T: Config> = StorageValue<_, BalanceOf<T>, OptionQuery>;

	/// Stores amount of buyouts (amount, block number of last buyout)
	#[pallet::storage]
	pub type Buyouts<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountIdOf<T>, (BalanceOf<T>, u32), ValueQuery>;

	/// Stores allowed currencies for buyout
	#[pallet::storage]
	pub(super) type AllowedCurrencies<T: Config> =
		StorageMap<_, Blake2_128Concat, CurrencyIdOf<T>, (), OptionQuery>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub allowed_currencies: Vec<CurrencyIdOf<T>>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { allowed_currencies: vec![] }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			for i in &self.allowed_currencies.clone() {
				AllowedCurrencies::<T>::insert(i, ());
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Allows caller to buyout a given amount of native token.
		/// When denoting the `amount` as `Buyout` the caller will receive this exact amount of the native token in exchange for a corresponding amount of an allowed asset.
		/// When denoting the `amount` as `Exchange`, the caller will spend this exact amount of an allowed asset in exchange for a corresponding amount of the native token.
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

		/// Allows root to update the allowed currencies for buyout.
		/// `AllowedCurrencies` storage will be reset and updated with provided `assets`.
		/// 
		/// Parameters
		///
		/// - `origin`: Origin must be root.
		/// - `assets`: List of assets to be inserted into `AllowedCurrencies` storage.
		///
		/// Emits `AllowedAssetsForBuyoutUpdated` event when successful.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as pallet::Config>::WeightInfo::update_allowed_assets())]
		#[transactional]
		pub fn update_allowed_assets(
			origin: OriginFor<T>,
			assets: Vec<CurrencyIdOf<T>>,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			let max_allowed_currencies_for_buyout = T::MaxAllowedBuyoutCurrencies::get();
			// Ensure number of currencies doesn't exceed the maximum allowed
			ensure!(
				assets.len() <= max_allowed_currencies_for_buyout as usize,
				Error::<T>::ExceedsNumberOfAllowedCurrencies
			);

			// Ensure that native token is not allowed for buyout
			let basic_asset = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
			ensure!(
				!assets.iter().any(|asset| *asset == basic_asset),
				Error::<T>::NativeTokenNotAllowed
			);

			// Clear `AllowedCurrencies` storage
			// `AllowedCurrencies` should have at most `max_allowed_currencies_for_buyout` entries
			let result = AllowedCurrencies::<T>::clear(max_allowed_currencies_for_buyout, None);
			// If storage clearing returns cursor which is `Some`, then clearing was not entirely successful
			ensure!(result.maybe_cursor.is_none(), Error::<T>::StorageClearingFailure);


			// Used for event data
			let mut allowed_assets = Vec::new();

			// Update `AllowedCurrencies` storage with provided `assets`
			for asset in assets.clone() {
				// Check for duplicates
				if !AllowedCurrencies::<T>::contains_key(&asset) {
					AllowedCurrencies::<T>::insert(asset, ());
					allowed_assets.push(asset);
				}
			}

			Self::deposit_event(Event::<T>::AllowedAssetsForBuyoutUpdated {
				allowed_assets
			});
			Ok(().into())
		}
	}
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
			let current_block_number =
				<frame_system::Pallet<T>>::block_number().saturated_into::<u32>();
			let current_period_start_number = current_block_number
				.checked_div(buyout_period)
				.and_then(|n| Some(n.saturating_mul(buyout_period)))
				.unwrap_or_default();
			let (mut buyouts, last_buyout) = Buyouts::<T>::get(account_id);

			// Check if caller's last buyout was in the previous period
			// If true, reset buyout amount limit for the caller since this is the first buyout in the current period
			if !buyouts.is_zero() && last_buyout < current_period_start_number {
				buyouts = Default::default();
				Buyouts::<T>::insert(account_id, (buyouts, current_block_number));
			};

			ensure!(
				buyouts.saturating_add(buyout_amount) <= buyout_limit,
				Error::<T>::BuyoutLimitExceeded
			);
		}

		Ok(())
	}

	/// Ensures that asset is allowed for buyout
	fn ensure_asset_allowed_for_buyout(asset: &CurrencyIdOf<T>) -> DispatchResult {
		ensure!(AllowedCurrencies::<T>::get(asset) == Some(()), Error::<T>::WrongAssetToBuyout);

		Ok(())
	}

	/// Updates buyouts storage for the account
	fn update_buyouts(account_id: &AccountIdOf<T>, buyout_amount: BalanceOf<T>) {
		if BuyoutLimit::<T>::get().is_some() {
			Buyouts::<T>::mutate(account_id, |(prev_buyouts, last)| {
				*prev_buyouts = prev_buyouts.saturating_add(buyout_amount);
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
		let fee_plus_one = FixedU128::from(T::SellFee::get())
			.checked_add(&FixedU128::one())
			.ok_or::<DispatchError>(ArithmeticError::Overflow.into())?;
		let basic_asset_price_with_fee = basic_asset_price.saturating_mul(fee_plus_one);

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
		let fee_plus_one = FixedU128::from(T::SellFee::get())
			.checked_add(&FixedU128::one())
			.ok_or::<DispatchError>(ArithmeticError::Overflow.into())?;
		let basic_asset_price_with_fee = basic_asset_price.saturating_mul(fee_plus_one);

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
		Self::ensure_asset_allowed_for_buyout(&asset)?;

		let basic_asset = <T as orml_currencies::Config>::GetNativeCurrencyId::get();
		let (buyout_amount, exchange_amount) = Self::split_to_buyout_and_exchange(asset, amount)?;

		Self::ensure_buyout_limit_not_exceeded(&who, buyout_amount)?;
		let treasury_account_id = T::TreasuryAccount::get();

		// Start exchanging
		// Check for same accounts
		if who == treasury_account_id {
			return Err(Error::<T>::BuyoutWithTreasuryAccount.into())
		}
		// Check for exchanging zero values
		if exchange_amount.is_zero() && buyout_amount.is_zero() {
			return Err(Error::<T>::LessThanMinBuyoutAmount.into())
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
				Pallet::<T>::ensure_asset_allowed_for_buyout(asset).map_err(|_| {
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
