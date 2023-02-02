// #![deny(warnings)]
#![cfg_attr(test, feature(proc_macro_hygiene))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
extern crate mocktopus;

use codec::Encode;
use frame_support::{
	dispatch::{DispatchError, DispatchResult},
	ensure,
	traits::fungibles,
	transactional,
};
#[cfg(test)]
use mocktopus::macros::mockable;
use sha2::{Digest, Sha256};
use sp_core::{H256, U256};
use sp_runtime::{traits::*, ArithmeticError};
use sp_std::{collections::btree_set::BTreeSet, convert::TryInto, prelude::*, vec};

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	use super::*;

	/// ## Configuration
	/// The pallet's configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config + orml_tokens::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		// RecoverFromErrors { new_status: StatusCode, cleared_errors: Vec<ErrorCode> },
		UpdateActiveBlock {
			block_number: T::BlockNumber,
		},
		/// (Additional) funds have been approved for transfer to a destination account.
		ApprovedTransfer {
			currency_id: T::CurrencyId,
			source: T::AccountId,
			delegate: T::AccountId,
			amount: T::Balance,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Parachain is not running.
		Unapproved,
		ParachainNotRunning,
		CurrencyNotLive,
	}

	#[pallet::storage]
	/// Approved balance transfers. Balance is the amount approved for transfer.
	/// First key is the asset ID, second key is the owner and third key is the delegate.
	pub(super) type Approvals<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Blake2_128Concat, T::CurrencyId>,
			NMapKey<Blake2_128Concat, T::AccountId>, // owner
			NMapKey<Blake2_128Concat, T::AccountId>, // delegate
		),
		T::Balance,
	>;

	#[pallet::storage]
	/// Currencies that can be used to give approval
	pub(super) type AllowedCurrencies<T: Config> =
		StorageMap<_, Blake2_128Concat, T::CurrencyId, bool>;

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub allowed_currencies: Vec<T::CurrencyId>,
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
				AllowedCurrencies::<T>::insert(i, true);
			}
		}
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	// The pallet's dispatchable functions.
	#[pallet::call]
	impl<T: Config> Pallet<T> {}
}

#[allow(clippy::forget_non_drop, clippy::swap_ptr_to_ref, clippy::forget_ref, clippy::forget_copy)]
#[cfg_attr(test, mockable)]
impl<T: Config> Pallet<T> {
	/// Creates an approval from `owner` to spend `amount` of asset `id` tokens by 'delegate'
	/// while reserving `T::ApprovalDeposit` from owner
	///
	/// If an approval already exists, the new amount is added to such existing approval
	pub fn do_approve_transfer(
		id: T::CurrencyId,
		owner: &T::AccountId,
		delegate: &T::AccountId,
		amount: T::Balance,
	) -> DispatchResult {
		ensure!(AllowedCurrencies::<T>::get(id) == Some(true), Error::<T>::CurrencyNotLive);
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
		id: T::CurrencyId,
		owner: &T::AccountId,
		delegate: &T::AccountId,
		destination: &T::AccountId,
		amount: T::Balance,
	) -> DispatchResult {
		ensure!(AllowedCurrencies::<T>::get(id) == Some(true), Error::<T>::CurrencyNotLive);
		Approvals::<T>::try_mutate_exists(
			(id, &owner, delegate),
			|maybe_approved| -> DispatchResult {
				let mut approved = maybe_approved.take().ok_or(Error::<T>::Unapproved)?;
				let remaining = approved.checked_sub(&amount).ok_or(Error::<T>::Unapproved)?;

				let b = <orml_tokens::Pallet<T> as frame_support::traits::fungibles::Transfer<
					<T as frame_system::Config>::AccountId,
				>>::transfer(id, owner, destination, amount, false)?;

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
