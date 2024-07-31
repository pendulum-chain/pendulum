#![deny(warnings)]
#![cfg_attr(test, feature(proc_macro_hygiene))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
extern crate mocktopus;

use frame_support::{dispatch::DispatchResult, ensure};

#[cfg(test)]
use mocktopus::macros::mockable;
use orml_traits::MultiCurrency;
use sp_runtime::traits::*;
use sp_std::{convert::TryInto, prelude::*, vec};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod default_weights;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use pallet::*;

pub(crate) type BalanceOf<T> =
	<<T as orml_currencies::Config>::MultiCurrency as orml_traits::MultiCurrency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

pub(crate) type CurrencyOf<T> =
	<<T as orml_currencies::Config>::MultiCurrency as orml_traits::MultiCurrency<
		<T as frame_system::Config>::AccountId,
	>>::CurrencyId;

#[frame_support::pallet]
pub mod pallet {
	use crate::default_weights::WeightInfo;
	use frame_support::{pallet_prelude::*, transactional};
	use frame_system::{ensure_root, ensure_signed, pallet_prelude::OriginFor};

	use super::*;

	/// ## Configuration
	/// The pallet's configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config + orml_tokens::Config + orml_currencies::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Weight information for the extrinsics in this module.
		type WeightInfo: WeightInfo;

		/// The maximum number of allowed currencies.
		#[pallet::constant]
		type MaxAllowedCurrencies: Get<u32>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AllowedCurrenciesAdded {
			currencies: Vec<CurrencyOf<T>>,
		},
		AllowedCurrenciesDeleted {
			currencies: Vec<CurrencyOf<T>>,
		},
		/// (Additional) funds have been approved for transfer to a destination account.
		TransferApproved {
			currency_id: CurrencyOf<T>,
			source: T::AccountId,
			delegate: T::AccountId,
			amount: BalanceOf<T>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		Unapproved,
		CurrencyNotLive,
		ExceedsNumberOfAllowedCurrencies,
	}

	/// Approved balance transfers. Balance is the amount approved for transfer.
	/// First key is the currency ID, second key is the owner and third key is the delegate.
	#[pallet::storage]
	pub type Approvals<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, CurrencyOf<T>>,
			NMapKey<Blake2_128Concat, T::AccountId>, // owner
			NMapKey<Blake2_128Concat, T::AccountId>, // delegate
		),
		BalanceOf<T>,
	>;

	/// Currencies that can be used in chain extension
	#[pallet::storage]
	pub(super) type AllowedCurrencies<T: Config> =
		StorageMap<_, Blake2_128Concat, CurrencyOf<T>, ()>;

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub allowed_currencies: Vec<CurrencyOf<T>>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { allowed_currencies: vec![] }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for i in &self.allowed_currencies.clone() {
				AllowedCurrencies::<T>::insert(i, ());
			}
		}
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	// The pallet's dispatchable functions.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Added allowed currencies that possible to use chain extension
		///
		/// # Arguments
		/// * `currencies` - list of currency id allowed to use in chain extension
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::add_allowed_currencies(T::MaxAllowedCurrencies::get()))]
		#[transactional]
		pub fn add_allowed_currencies(
			origin: OriginFor<T>,
			currencies: Vec<CurrencyOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			// Check if the supplied amount of currencies is less than the maximum allowed
			let max_allowed_currencies: usize = T::MaxAllowedCurrencies::get() as usize;
			ensure!(
				currencies.len() <= max_allowed_currencies,
				Error::<T>::ExceedsNumberOfAllowedCurrencies
			);

			for i in currencies.clone() {
				AllowedCurrencies::<T>::insert(i, ());
			}

			// Check if the resulting vector of allowed currencies is less than the maximum allowed.
			// We check after the insertion to avoid counting duplicates.
			let allowed_currencies_len: usize = AllowedCurrencies::<T>::iter().count();
			ensure!(
				allowed_currencies_len <= max_allowed_currencies,
				Error::<T>::ExceedsNumberOfAllowedCurrencies
			);

			Self::deposit_event(Event::AllowedCurrenciesAdded { currencies });
			Ok(())
		}

		/// Remove allowed currencies that possible to use chain extension
		///
		/// # Arguments
		/// * `currencies` - list of currency id allowed to use in chain extension
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_allowed_currencies(T::MaxAllowedCurrencies::get()))]
		#[transactional]
		pub fn remove_allowed_currencies(
			origin: OriginFor<T>,
			currencies: Vec<CurrencyOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;

			// Check if the supplied amount of currencies is less than the maximum allowed
			// Although this is not strictly necessary, it is a good sanity check and prevents callers
			// from using too large currency vectors.
			let max_allowed_currencies: usize = T::MaxAllowedCurrencies::get() as usize;
			ensure!(
				currencies.len() <= max_allowed_currencies,
				Error::<T>::ExceedsNumberOfAllowedCurrencies
			);

			for i in currencies.clone() {
				AllowedCurrencies::<T>::remove(i);
			}

			Self::deposit_event(Event::AllowedCurrenciesDeleted { currencies });
			Ok(())
		}

		/// Approve an amount for another account to spend on owner's behalf.
		///
		/// # Arguments
		/// * `id` - the currency_id of the asset to approve
		/// * `delegate` - the spender account to approve the asset for
		/// * `amount` - the amount of the asset to approve
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::approve())]
		#[transactional]
		pub fn approve(
			origin: OriginFor<T>,
			id: CurrencyOf<T>,
			delegate: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_approve_transfer(id, &owner, &delegate, amount)
		}

		/// Execute a pre-approved transfer from another account
		///
		/// # Arguments
		/// * `id` - the currency_id of the asset to transfer
		/// * `owner` - the owner account of the asset to transfer
		/// * `destination` - the destination account to transfer to
		/// * `amount` - the amount of the asset to transfer
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::transfer_from())]
		#[transactional]
		pub fn transfer_from(
			origin: OriginFor<T>,
			id: CurrencyOf<T>,
			owner: T::AccountId,
			destination: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let delegate = ensure_signed(origin)?;
			Self::do_transfer_approved(id, &owner, &delegate, &destination, amount)
		}
	}
}

#[allow(clippy::forget_non_drop, clippy::swap_ptr_to_ref, forgetting_references, forgetting_copy_types)]
#[cfg_attr(test, mockable)]
impl<T: Config> Pallet<T> {
	// Check the amount approved to be spent by an owner to a delegate
	pub fn is_allowed_currency(asset: CurrencyOf<T>) -> bool {
		AllowedCurrencies::<T>::get(asset) == Some(())
	}

	// Check the amount approved to be spent by an owner to a delegate
	pub fn allowance(
		asset: CurrencyOf<T>,
		owner: &T::AccountId,
		delegate: &T::AccountId,
	) -> BalanceOf<T> {
		Approvals::<T>::get((asset, &owner, &delegate)).unwrap_or_else(Zero::zero)
	}

	/// Creates an approval from `owner` to spend `amount` of asset `id` tokens by 'delegate'
	/// while reserving `T::ApprovalDeposit` from owner
	///
	/// If an approval already exists, the new amount is added to such existing approval
	pub fn do_approve_transfer(
		id: CurrencyOf<T>,
		owner: &T::AccountId,
		delegate: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		ensure!(Self::is_allowed_currency(id), Error::<T>::CurrencyNotLive);
		Approvals::<T>::set((id, &owner, &delegate), Some(amount));
		Self::deposit_event(Event::TransferApproved {
			currency_id: id,
			source: owner.clone(),
			delegate: delegate.clone(),
			amount,
		});

		Ok(())
	}

	/// Reduces the asset `id` balance of `owner` by some `amount` and increases the balance of
	/// `dest` by (similar) amount, checking that 'delegate' has an existing approval from `owner`
	/// to spend`amount`.
	///
	/// Will fail if `amount` is greater than the approval from `owner` to 'delegate'
	/// Will unreserve the deposit from `owner` if the entire approved `amount` is spent by
	/// 'delegate'
	pub fn do_transfer_approved(
		id: CurrencyOf<T>,
		owner: &T::AccountId,
		delegate: &T::AccountId,
		destination: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		ensure!(Self::is_allowed_currency(id), Error::<T>::CurrencyNotLive);
		Approvals::<T>::try_mutate_exists(
			(id, &owner, delegate),
			|maybe_approved| -> DispatchResult {
				let approved = maybe_approved.take().ok_or(Error::<T>::Unapproved)?;
				let remaining = approved.checked_sub(&amount).ok_or(Error::<T>::Unapproved)?;

				<orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::transfer(
					id,
					owner,
					destination,
					amount,
				)?;

				// Don't decrement allowance if it is set to the max value (which acts as infinite allowance)
				if approved == BalanceOf::<T>::max_value() {
					*maybe_approved = Some(approved);
				} else if remaining.is_zero() {
					*maybe_approved = None;
				} else {
					*maybe_approved = Some(remaining);
				}
				Ok(())
			},
		)?;
		Ok(())
	}
}
