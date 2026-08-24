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

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

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

		/// The keyless treasury account whose funds `migrate_treasury` moves to
		/// Base. Set in the runtime to the treasury pallet account.
		type TreasuryAccount: Get<Self::AccountId>;

		/// Origin allowed to set the treasury's Base destination and trigger a
		/// treasury migration (root or a council majority in the runtime).
		type TreasuryMigrateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

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
		/// The fixed Base destination for treasury migrations was set.
		TreasuryDestinationSet { base_address: H160 },
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
		/// The Base address is structurally invalid (e.g. the zero address).
		/// The vault on Base would reject the release, permanently stranding
		/// the burned tokens and stalling the attestor pipeline.
		InvalidBaseAddress,
		/// A treasury migration was attempted before the Base destination was
		/// set via `set_treasury_destination`.
		NoTreasuryDestination,
	}

	/// Nonce of the next migration. Monotonically increasing, never reused;
	/// each emitted `MigrationInitiated` event consumes one value.
	#[pallet::storage]
	pub type NextNonce<T> = StorageValue<_, u64, ValueQuery>;

	/// Cumulative amount burned for migration, for the invariant monitor
	/// (PRD M2: vault balance on Base + total released == max issuance).
	#[pallet::storage]
	pub type TotalMigrated<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	/// Migrations ship **paused**. A runtime upgrade that adds this pallet
	/// writes no storage, so the pallet reads as paused until governance
	/// explicitly enables it with `set_paused(false)`.
	///
	/// This is deliberately fail-safe rather than a one-shot storage
	/// migration: the runtime upgrade that enables `migrate` and the decision
	/// to go live are separate acts. If `migrate` were live on enactment, a
	/// referendum enacting before the Base vault and attestor set are
	/// operational would let holders burn PEN with nothing able to release it.
	/// Making it a property of the storage default means it cannot be
	/// defeated by forgetting to wire a migration into `Executive`, and it
	/// re-arms if the value is ever cleared.
	#[pallet::type_value]
	pub fn DefaultPaused<T: Config>() -> bool {
		true
	}

	/// Whether migrations are paused. Defaults to `true` — see [`DefaultPaused`].
	#[pallet::storage]
	pub type Paused<T> = StorageValue<_, bool, ValueQuery, DefaultPaused<T>>;

	/// The fixed Base destination for treasury migrations. `migrate_treasury`
	/// always sends here; `None` until set by `set_treasury_destination`.
	#[pallet::storage]
	pub type TreasuryDestination<T> = StorageValue<_, H160, OptionQuery>;

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
			// The vault contract rejects the zero address; burning towards it
			// would emit an event no attestor can ever execute.
			ensure!(base_address != H160::zero(), Error::<T>::InvalidBaseAddress);

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

			// Shared accounting + event (identical to a treasury migration, so
			// the attestor set decodes both the same way).
			Self::emit_migration(who, base_address, amount)
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

		/// Set the fixed Base destination for treasury migrations.
		///
		/// Callable by the treasury-migrate origin (root or a council majority).
		/// This is the single security anchor for treasury migrations: once set,
		/// `migrate_treasury` always sends here, so the routine call carries no
		/// address and cannot be sent to the wrong place by a typo.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::set_treasury_destination())]
		pub fn set_treasury_destination(origin: OriginFor<T>, base_address: H160) -> DispatchResult {
			T::TreasuryMigrateOrigin::ensure_origin(origin)?;
			ensure!(base_address != H160::zero(), Error::<T>::InvalidBaseAddress);
			TreasuryDestination::<T>::put(base_address);
			Self::deposit_event(Event::TreasuryDestinationSet { base_address });
			Ok(())
		}

		/// Burn `amount` of the treasury's native tokens for migration to the
		/// pre-set Base destination.
		///
		/// Callable by the treasury-migrate origin. Requires a destination to
		/// have been set. Emits the same `MigrationInitiated` event as a user
		/// migration (with `who` = the treasury account), so the attestor set,
		/// vault and monitor process it identically.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::migrate_treasury())]
		pub fn migrate_treasury(
			origin: OriginFor<T>,
			#[pallet::compact] amount: BalanceOf<T>,
		) -> DispatchResult {
			T::TreasuryMigrateOrigin::ensure_origin(origin)?;
			ensure!(!Paused::<T>::get(), Error::<T>::MigrationsPaused);
			ensure!(
				amount >= T::MinimumMigrationAmount::get(),
				Error::<T>::AmountBelowMinimum
			);
			let base_address =
				TreasuryDestination::<T>::get().ok_or(Error::<T>::NoTreasuryDestination)?;

			let treasury = T::TreasuryAccount::get();
			ensure!(
				T::Currency::free_balance(&treasury) >= amount,
				Error::<T>::InsufficientBalance
			);

			// KeepAlive: the treasury is a persistent system account and must
			// never be reaped by a migration.
			let imbalance = T::Currency::withdraw(
				&treasury,
				amount,
				WithdrawReasons::TRANSFER,
				ExistenceRequirement::KeepAlive,
			)?;
			drop(imbalance);

			Self::emit_migration(treasury, base_address, amount)
		}
	}

	impl<T: Config> Pallet<T> {
		/// Shared tail of every migration: consume a unique nonce, update the
		/// cumulative total, and emit `MigrationInitiated`. The caller must have
		/// already burned `amount` from `who`.
		fn emit_migration(who: T::AccountId, base_address: H160, amount: BalanceOf<T>) -> DispatchResult {
			let nonce = NextNonce::<T>::get();
			let next = nonce.checked_add(1).ok_or(ArithmeticError::Overflow)?;
			NextNonce::<T>::put(next);
			TotalMigrated::<T>::mutate(|total| *total = total.saturating_add(amount));

			Self::deposit_event(Event::MigrationInitiated { nonce, who, base_address, amount });
			Ok(())
		}
	}
}
