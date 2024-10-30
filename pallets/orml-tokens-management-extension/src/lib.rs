#![deny(warnings)]
#![cfg_attr(not(feature = "std"), no_std)]


use sp_std::{convert::TryInto, prelude::*, vec};

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

pub mod default_weights;

#[cfg(test)]
mod tests;

mod ext;

mod types;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::types::{AccountIdOf, BalanceOf, CurrencyDetails, CurrencyOf};
	use frame_support::{pallet_prelude::*, transactional};
	use frame_system::{ensure_root, ensure_signed, pallet_prelude::OriginFor};

	pub use default_weights::WeightInfo;

	/// ## Configuration
	/// The pallet's configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config + orml_tokens::Config + orml_currencies::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Weight information for the extrinsics in this module.
		type WeightInfo: WeightInfo;

		/// Type that allows for checking if currency type is ownable by users
		type CurrencyIdChecker: CurrencyIdCheck<CurrencyId = CurrencyOf<Self>>;

		/// The deposit currency
		#[pallet::constant]
		type DepositCurrency: Get<CurrencyOf<Self>>;

		/// The deposit amount required to take a currency
		#[pallet::constant]
		type AssetDeposit: Get<BalanceOf<Self>>;
	}

	#[pallet::storage]
	#[pallet::getter(fn currency_details)]
	pub type CurrencyData<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		CurrencyOf<T>,
		CurrencyDetails<AccountIdOf<T>, BalanceOf<T>>,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Some currency was issued.
		Mint { currency_id: CurrencyOf<T>, to: AccountIdOf<T>, amount: BalanceOf<T> },
		/// Some currency was burned.
		Burned { currency_id: CurrencyOf<T>, from: AccountIdOf<T>, amount: BalanceOf<T> },
		/// Some currency class was created.
		Created { currency_id: CurrencyOf<T>, creator: AccountIdOf<T>, owner: AccountIdOf<T> },
		/// Change of ownership
		OwnershipChanged { currency_id: CurrencyOf<T>, new_owner: AccountIdOf<T> },
		/// Issuer and admin changed
		ManagersChanged {
			currency_id: CurrencyOf<T>,
			new_admin: AccountIdOf<T>,
			new_issuer: AccountIdOf<T>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Trying to register a new currency when id is in use
		AlreadyCreated,
		/// Trying to register a currency variant that is not ownable
		NotOwnableCurrency,
		/// Currency has not been created
		NotCreated,
		/// No permission to call the operation
		NoPermission,
		/// Insuficient balance to make the creation deposit
		InsufficientBalance,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// The pallet's dispatchable functions.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create and take ownership of one CurrencyId
		///
		/// The creator will have full control of this pallet's functions
		/// regarding this currency
		///
		/// Parameters:
		/// - `currency_id`: Allowed Currency Id.
		///
		/// Emits `Created` event when successful.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::create())]
		#[transactional]
		pub fn create(origin: OriginFor<T>, currency_id: CurrencyOf<T>) -> DispatchResult {
			let creator = ensure_signed(origin)?;

			ensure!(
				T::CurrencyIdChecker::is_valid_currency_id(&currency_id),
				Error::<T>::NotOwnableCurrency
			);
			ensure!(!CurrencyData::<T>::contains_key(currency_id), Error::<T>::AlreadyCreated);

			let deposit = T::AssetDeposit::get();
			ext::orml_currencies_ext::reserve::<T>(T::DepositCurrency::get(), &creator, deposit)
				.map_err(|_| Error::<T>::InsufficientBalance)?;

			CurrencyData::<T>::insert(
				currency_id,
				CurrencyDetails {
					owner: creator.clone(),
					issuer: creator.clone(),
					admin: creator.clone(),
					deposit,
				},
			);

			Self::deposit_event(Event::Created {
				currency_id,
				creator: creator.clone(),
				owner: creator,
			});

			Ok(())
		}

		/// Mint currency of a particular class.
		///
		/// The origin must be Signed and the sender must be the Issuer of the currency `id`.
		///
		/// - `id`: The identifier of the currency to have some amount minted.
		/// - `beneficiary`: The account to be credited with the minted currency.
		/// - `amount`: The amount of the currency to be minted.
		///
		/// Emits `Issued` event when successful.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::mint())]
		#[transactional]
		pub fn mint(
			origin: OriginFor<T>,
			currency_id: CurrencyOf<T>,
			to: AccountIdOf<T>,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let origin = ensure_signed(origin)?;

			// get currency details and check issuer
			let currency_data =
				CurrencyData::<T>::get(currency_id).ok_or(Error::<T>::NotCreated)?;
			ensure!(origin == currency_data.issuer, Error::<T>::NoPermission);

			// do mint via orml-currencies
			ext::orml_currencies_ext::mint::<T>(currency_id, &to, amount)?;

			Self::deposit_event(Event::Mint { currency_id, to, amount });
			Ok(())
		}

		/// Burn currency of a particular class.
		///
		/// The origin must be Signed and the sender must be the Admin of the currency `id`.
		///
		/// - `id`: The identifier of the currency to have some amount burned.
		/// - `from`: The account to be debited.
		/// - `amount`: The amount of the currency to be burned.
		///
		/// Emits `Burned` event when successful.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::burn())]
		#[transactional]
		pub fn burn(
			origin: OriginFor<T>,
			currency_id: CurrencyOf<T>,
			from: AccountIdOf<T>,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let origin = ensure_signed(origin)?;

			// get currency details and check admin
			let currency_data =
				CurrencyData::<T>::get(currency_id).ok_or(Error::<T>::NotCreated)?;
			ensure!(origin == currency_data.admin, Error::<T>::NoPermission);

			// do burn via orml-currencies
			ext::orml_currencies_ext::burn::<T>(currency_id, &from, amount)?;

			Self::deposit_event(Event::Burned { currency_id, from, amount });
			Ok(())
		}

		/// Change the Owner of a currency.
		///
		/// Origin must be Signed and the sender should be the Owner of the currency.
		///
		/// - `currency_id`: Currency id.
		/// - `new_owner`: The new Owner of this currency.
		///
		/// Emits `OwnershipChanged`.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::transfer_ownership())]
		#[transactional]
		pub fn transfer_ownership(
			origin: OriginFor<T>,
			currency_id: CurrencyOf<T>,
			new_owner: AccountIdOf<T>,
		) -> DispatchResult {
			let origin = ensure_signed(origin)?;

			CurrencyData::<T>::try_mutate(currency_id, |maybe_details| {
				let details = maybe_details.as_mut().ok_or(Error::<T>::NotCreated)?;
				ensure!(origin == details.owner, Error::<T>::NoPermission);

				if details.owner == new_owner {
					return Ok(())
				}
				details.owner = new_owner.clone();

				// move reserved balance to the new owner's account
				ext::orml_currencies_ext::repatriate_reserve::<T>(
					T::DepositCurrency::get(),
					&origin,
					&new_owner,
					details.deposit,
				)?;

				Self::deposit_event(Event::OwnershipChanged { currency_id, new_owner });
				Ok(())
			})
		}

		/// Force transfer ownership from root.
		///
		/// Origin must be root.
		///
		/// - `currency_id`: Currency id.
		/// - `new_owner`: The new Owner of this currency.
		///
		/// Emits `OwnershipChanged`.
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::transfer_ownership())]
		#[transactional]
		pub fn force_transfer_ownership(
			origin: OriginFor<T>,
			currency_id: CurrencyOf<T>,
			new_owner: AccountIdOf<T>,
		) -> DispatchResult {
			ensure_root(origin)?;

			CurrencyData::<T>::try_mutate(currency_id, |maybe_details| {
				let details = maybe_details.as_mut().ok_or(Error::<T>::NotCreated)?;

				if details.owner == new_owner {
					return Ok(())
				}
				ext::orml_currencies_ext::repatriate_reserve::<T>(
					T::DepositCurrency::get(),
					&details.owner,
					&new_owner,
					details.deposit,
				)?;

				details.owner = new_owner.clone();

				Self::deposit_event(Event::OwnershipChanged { currency_id, new_owner });
				Ok(())
			})
		}

		/// Change the Issuer and Admin.
		///
		/// Origin must be Signed and the sender should be the Owner of the currency.
		///
		/// - `currency_id`: Identifier of the currency.
		/// - `new_admin`: The new Admin of this currency.
		/// - `new_issuer`: The new Issuer of this currency.
		///
		/// Emits `ManagersChanged`.
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::set_managers())]
		#[transactional]
		pub fn set_managers(
			origin: OriginFor<T>,
			currency_id: CurrencyOf<T>,
			new_admin: AccountIdOf<T>,
			new_issuer: AccountIdOf<T>,
		) -> DispatchResult {
			let origin = ensure_signed(origin)?;

			CurrencyData::<T>::try_mutate(currency_id, |maybe_details| {
				let details = maybe_details.as_mut().ok_or(Error::<T>::NotCreated)?;

				ensure!(origin == details.owner, Error::<T>::NoPermission);

				details.issuer = new_issuer.clone();
				details.admin = new_admin.clone();

				Self::deposit_event(Event::ManagersChanged { currency_id, new_admin, new_issuer });
				Ok(())
			})
		}
	}
}

impl<T: Config> Pallet<T> {}

pub trait CurrencyIdCheck {
	type CurrencyId;
	fn is_valid_currency_id(currency_id: &Self::CurrencyId) -> bool;
}
