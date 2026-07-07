//! # Token Migration Pallet
//!
//! One-way migration of the native token (PEN) to Base.
//!
//! Holders call [`Pallet::migrate`] with an amount and the Base (EVM) address that
//! should receive the tokens. The amount is burned (total issuance decreases) and a
//! [`Event::MigrationInitiated`] event is emitted carrying a globally unique nonce.
//! An off-chain attestor set observes these events in finalized blocks and approves
//! the corresponding release from the MigrationVault contract on Base.
//!
//! The pallet is fire-and-forget by design: it has no knowledge of Base state and
//! migrations are irreversible. See docs/pen-base-migration-prd.md (§6.1) for the
//! full requirements this pallet implements.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

pub mod default_weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use default_weights::WeightInfo;

use frame_support::traits::{Currency, ExistenceRequirement, WithdrawReasons};
use sp_core::H160;
use sp_runtime::{
	traits::{CheckedSub, Saturating, Zero},
	ArithmeticError,
};

pub(crate) type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The native currency. Migrated amounts are withdrawn and burned,
		/// reducing total issuance (PRD decision D1: burn).
		type Currency: Currency<Self::AccountId>;

		/// Smallest amount accepted by `migrate`, to keep dust-sized migrations
		/// from spamming the attestor pipeline.
		#[pallet::constant]
		type MinimumMigrationAmount: Get<BalanceOf<Self>>;

		/// Origin allowed to pause and unpause migrations (incident response).
		type PauseOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// `amount` of the native token was burned for migration to Base.
		/// The attestor set releases the equivalent amount to `base_address`
		/// on Base, keyed by the globally unique `nonce`.
		MigrationInitiated {
			nonce: u64,
			who: T::AccountId,
			base_address: H160,
			amount: BalanceOf<T>,
		},
		/// Migrations were paused or unpaused by the pause origin.
		MigrationPauseSet { paused: bool },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Migrations are currently paused.
		MigrationsPaused,
		/// The amount is below the configured minimum migration amount.
		AmountBelowMinimum,
		/// The caller's free balance is lower than the requested amount.
		InsufficientBalance,
		/// The migration would leave a remainder below the existential deposit.
		/// Migrate the entire balance or leave at least the existential deposit.
		WouldLeaveDust,
	}

	/// Nonce of the next migration. Monotonically increasing, never reused;
	/// each emitted `MigrationInitiated` event consumes one value.
	#[pallet::storage]
	pub type NextNonce<T> = StorageValue<_, u64, ValueQuery>;

	/// Cumulative amount burned for migration, for the invariant monitor
	/// (PRD M2: vault balance on Base + total released == max issuance).
	#[pallet::storage]
	pub type TotalMigrated<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	/// Whether migrations are paused.
	#[pallet::storage]
	pub type Paused<T> = StorageValue<_, bool, ValueQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Burn `amount` of the caller's native tokens for migration to Base.
		///
		/// The tokens are released to `base_address` on Base by the attestor set.
		/// This action is IRREVERSIBLE: a wrong `base_address` means the tokens
		/// are lost. The caller must either migrate their entire free balance or
		/// leave at least the existential deposit behind.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::migrate())]
		pub fn migrate(
			origin: OriginFor<T>,
			#[pallet::compact] amount: BalanceOf<T>,
			base_address: H160,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(!Paused::<T>::get(), Error::<T>::MigrationsPaused);
			ensure!(
				amount >= T::MinimumMigrationAmount::get(),
				Error::<T>::AmountBelowMinimum
			);

			let free = T::Currency::free_balance(&who);
			let remainder = free.checked_sub(&amount).ok_or(Error::<T>::InsufficientBalance)?;
			ensure!(
				remainder.is_zero() || remainder >= T::Currency::minimum_balance(),
				Error::<T>::WouldLeaveDust
			);

			// Fails if locks or reserves make `amount` non-transferable (staking,
			// vesting and governance locks must be cleared before migrating).
			let imbalance = T::Currency::withdraw(
				&who,
				amount,
				WithdrawReasons::TRANSFER,
				ExistenceRequirement::AllowDeath,
			)?;
			// Dropping the negative imbalance without offsetting it burns the
			// withdrawn amount, i.e. total issuance decreases by `amount`.
			drop(imbalance);

			let nonce = NextNonce::<T>::get();
			let next = nonce.checked_add(1).ok_or(ArithmeticError::Overflow)?;
			NextNonce::<T>::put(next);
			TotalMigrated::<T>::mutate(|total| *total = total.saturating_add(amount));

			Self::deposit_event(Event::MigrationInitiated { nonce, who, base_address, amount });
			Ok(())
		}

		/// Pause or unpause migrations. Callable by the pause origin only.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::set_paused())]
		pub fn set_paused(origin: OriginFor<T>, paused: bool) -> DispatchResult {
			T::PauseOrigin::ensure_origin(origin)?;
			Paused::<T>::put(paused);
			Self::deposit_event(Event::MigrationPauseSet { paused });
			Ok(())
		}
	}
}
