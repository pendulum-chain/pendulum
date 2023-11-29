#![deny(warnings)]
#![cfg_attr(test, feature(proc_macro_hygiene))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
extern crate mocktopus;

#[cfg(test)]
use mocktopus::macros::mockable;
use orml_traits::MultiCurrency;
use sp_std::{convert::TryInto, prelude::*, vec};
pub use default_weights::{SubstrateWeight, WeightInfo};
// #[cfg(feature = "runtime-benchmarks")]
// mod benchmarking;

#[cfg(test)]
mod mock;

pub mod default_weights;

#[cfg(test)]
mod tests;

mod ext;

mod types;


pub use pallet::*;

pub(crate) type BalanceOf<T> = <<T as orml_currencies::Config>::MultiCurrency as MultiCurrency<<T as frame_system::Config>::AccountId>>::Balance;

pub(crate) type CurrencyOf<T> = <<T as orml_currencies::Config>::MultiCurrency as MultiCurrency<<T as frame_system::Config>::AccountId>>::CurrencyId;

pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{pallet_prelude::*, transactional};
	use frame_system::{ ensure_signed, pallet_prelude::OriginFor};
    use crate::types::CurrencyDetails;
	use super::*;

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
 
	}

    #[pallet::storage]
	#[pallet::getter(fn currency_details)]
	pub type CurrencyData<T: Config> = StorageMap<
    _,
    Blake2_128Concat,
    CurrencyOf<T>,
    CurrencyDetails<AccountIdOf<T>>,
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
		/// Some currency was destroyed (it's data)
		Destroyed { currency_id: CurrencyOf<T> },
		/// Change of ownership
		OwnershipChanged {currency_id: CurrencyOf<T>, new_owner: AccountIdOf<T>},
		/// Issuer and admin changed
		ManagersChanged {currency_id: CurrencyOf<T>, new_admin: AccountIdOf<T>, new_issuer: AccountIdOf<T>}
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
		NoPermission
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// The pallet's dispatchable functions.
	#[pallet::call]
	impl<T: Config> Pallet<T> {

		/// Create and take ownership of one CurrencyId
		///
		/// The creator will have full control of this pallelt's functions
		/// regarding this currency 
		///
		/// Parameters:
		/// - `currency_id`: Currency id of the Token(u64) variant.
		/// 
		/// Emits `Created` event when successful.
		///
		/// Weight: `O(1)`
		#[pallet::call_index(0)]
		#[pallet::weight(1)]
		#[transactional]
		pub fn create(origin: OriginFor<T>, currency_id: CurrencyOf<T>) -> DispatchResult {
			let creator = ensure_signed(origin)?;

			ensure!(T::CurrencyIdChecker::is_valid_currency_id(&currency_id), Error::<T>::NotOwnableCurrency);
			ensure!(!CurrencyData::<T>::contains_key(&currency_id), Error::<T>::AlreadyCreated);

			CurrencyData::<T>::insert(
				currency_id.clone(),
				CurrencyDetails {
					owner: creator.clone(),
					issuer: creator.clone(),
					admin: creator.clone(),
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
		///
		/// Weight: `O(1)`
		#[pallet::call_index(1)]
		#[pallet::weight(1)]
		#[transactional]
		pub fn mint(origin: OriginFor<T>, currency_id: CurrencyOf<T>, to: AccountIdOf<T>, amount: BalanceOf<T>) -> DispatchResult {
			let origin = ensure_signed(origin)?;

			// get currency details and check issuer
			let currency_data = CurrencyData::<T>::get(currency_id).ok_or(Error::<T>::NotCreated)?;
			ensure!(origin == currency_data.issuer, Error::<T>::NoPermission);
			
			// do mint via orml-currencies
			let _ = ext::orml_tokens::mint::<T>(currency_id, &to,amount)?;

			Self::deposit_event(Event::Mint {
				currency_id,
				to,
				amount,
			});
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
		///
		/// Weight: `O(1)`
		#[pallet::call_index(2)]
		#[pallet::weight(1)]
		#[transactional]
		pub fn burn(origin: OriginFor<T>, currency_id: CurrencyOf<T>, from: AccountIdOf<T>, amount: BalanceOf<T>) -> DispatchResult {
			let origin = ensure_signed(origin)?;

			// get currency details and check admin
			let currency_data = CurrencyData::<T>::get(currency_id).ok_or(Error::<T>::NotCreated)?;
			ensure!(origin == currency_data.admin, Error::<T>::NoPermission);
			
			// do burn via orml-currencies
			let _ = ext::orml_tokens::burn::<T>(currency_id, &from,amount)?;

			Self::deposit_event(Event::Burned {
				currency_id,
				from,
				amount,
			});
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
		///
		/// Weight: `O(1)`
		#[pallet::call_index(3)]
		#[pallet::weight(1)]
		#[transactional]
		pub fn transfer_ownership(origin: OriginFor<T>, currency_id: CurrencyOf<T>, new_owner: AccountIdOf<T>) -> DispatchResult {
			let origin = ensure_signed(origin)?;

			CurrencyData::<T>::try_mutate(currency_id.clone(), |maybe_details| {
				let details = maybe_details.as_mut().ok_or(Error::<T>::NotCreated)?;

				ensure!(origin == details.owner, Error::<T>::NoPermission);

				if details.owner == new_owner {
					return Ok(())
				}

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
		/// - `issuer`: The new Issuer of this currency.
		/// - `admin`: The new Admin of this currency.
		///
		/// Emits `ManagersChanged`.
		///
		/// Weight: `O(1)`
		#[pallet::call_index(4)]
		#[pallet::weight(1)]
		#[transactional]
		pub fn set_managers(origin: OriginFor<T>, currency_id: CurrencyOf<T>, new_admin: AccountIdOf<T>, new_issuer: AccountIdOf<T>) -> DispatchResult {
			let origin = ensure_signed(origin)?;
		
			CurrencyData::<T>::try_mutate(currency_id.clone(), |maybe_details| {
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

pub trait CurrencyIdCheck {
    type CurrencyId;
    fn is_valid_currency_id(currency_id: &Self::CurrencyId) -> bool;
}

#[cfg_attr(test, mockable)]
impl<T: Config> Pallet<T> {}
