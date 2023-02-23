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
	use frame_support::{pallet_prelude::*, transactional};
	use frame_system::{ensure_root, pallet_prelude::OriginFor};

	use super::*;

	/// ## Configuration
	/// The pallet's configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config + orml_tokens::Config + orml_currencies::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		AddedAllowedCurrencies {
			currencies: Vec<CurrencyOf<T>>,
		},
		DeletedAllowedCurrencies {
			currencies: Vec<CurrencyOf<T>>,
		},
		/// (Additional) funds have been approved for transfer to a destination account.
		ApprovedTransfer {
			currency_id: CurrencyOf<T>,
			source: T::AccountId,
			delegate: T::AccountId,
			amount: BalanceOf<T>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Parachain is not running.
		Unapproved,
		ParachainNotRunning,
		CurrencyNotLive,
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

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {}

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
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
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
		#[pallet::call_index(1)]
		#[pallet::weight(10000)]
		#[transactional]
		pub fn add_allowed_currencies(
			origin: OriginFor<T>,
			currencies: Vec<CurrencyOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;
			for i in currencies.clone() {
				AllowedCurrencies::<T>::insert(i, ());
			}

			Self::deposit_event(Event::AddedAllowedCurrencies { currencies });
			Ok(())
		}

		/// Remove allowed currencies that possible to use chain extension
		///
		/// # Arguments
		/// * `currencies` - list of currency id allowed to use in chain extension
		#[pallet::call_index(2)]
		#[pallet::weight(10000)]
		#[transactional]
		pub fn remove_allowed_currencies(
			origin: OriginFor<T>,
			currencies: Vec<CurrencyOf<T>>,
		) -> DispatchResult {
			ensure_root(origin)?;
			for i in currencies.clone() {
				AllowedCurrencies::<T>::remove(i);
			}

			Self::deposit_event(Event::DeletedAllowedCurrencies { currencies });
			Ok(())
		}
	}
}

#[allow(clippy::forget_non_drop, clippy::swap_ptr_to_ref, clippy::forget_ref, clippy::forget_copy)]
#[cfg_attr(test, mockable)]
impl<T: Config> Pallet<T> {
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
		ensure!(AllowedCurrencies::<T>::get(id) == Some(()), Error::<T>::CurrencyNotLive);
		Approvals::<T>::try_mutate((id, &owner, &delegate), |maybe_approved| -> DispatchResult {
			let mut approved = match maybe_approved.take() {
				// an approval already exists and is being updated
				Some(a) => a,
				// a new approval is created
				None => Default::default(),
			};

			approved = approved.saturating_add(amount);
			*maybe_approved = Some(approved);
			Ok(())
		})?;
		Self::deposit_event(Event::ApprovedTransfer {
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
		ensure!(AllowedCurrencies::<T>::get(id) == Some(()), Error::<T>::CurrencyNotLive);
		Approvals::<T>::try_mutate_exists(
			(id, &owner, delegate),
			|maybe_approved| -> DispatchResult {
				let mut approved = maybe_approved.take().ok_or(Error::<T>::Unapproved)?;
				let remaining = approved.checked_sub(&amount).ok_or(Error::<T>::Unapproved)?;

				<orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::transfer(
					id,
					&owner,
					&destination,
					amount,
				)?;

				if remaining.is_zero() {
					*maybe_approved = None;
				} else {
					approved = remaining;
					*maybe_approved = Some(approved);
				}
				Ok(())
			},
		)?;
		Ok(())
	}
}
