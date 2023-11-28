//#![deny(warnings)]
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

// #[cfg(feature = "runtime-benchmarks")]
// mod benchmarking;

// #[cfg(test)]
// mod mock;

// TODO add after definition
//pub mod default_weights;

// #[cfg(test)]
// mod tests;

mod ext;

mod types;


pub use pallet::*;

pub(crate) type BalanceOf<T> = <<T as orml_currencies::Config>::MultiCurrency as MultiCurrency<<T as frame_system::Config>::AccountId>>::Balance;

pub(crate) type CurrencyOf<T> = <<T as orml_currencies::Config>::MultiCurrency as MultiCurrency<<T as frame_system::Config>::AccountId>>::CurrencyId;

pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[frame_support::pallet]
pub mod pallet {
	// use crate::default_weights::WeightInfo;
	use frame_support::{pallet_prelude::*, transactional};
	use frame_system::{ensure_root, ensure_signed, pallet_prelude::OriginFor, WeightInfo};
    use crate::types::AssetDetails;
	use super::*;

	/// ## Configuration
	/// The pallet's configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config + orml_tokens::Config + orml_currencies::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Weight information for the extrinsics in this module.
		type WeightInfo: WeightInfo;
 
	}

    #[pallet::storage]
	#[pallet::getter(fn asset_details)]
	pub type PremiumRedeemFee<T: Config> = StorageMap<
    _,
    Blake2_128Concat,
    CurrencyOf<T>,
    AssetDetails<BalanceOf<T>, AccountIdOf<T>>,
    >;  


	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Mint,
		Burn,
	}

	#[pallet::error]
	pub enum Error<T> {
		NotOwner,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// The pallet's dispatchable functions.
	#[pallet::call]
	impl<T: Config> Pallet<T> {

		#[pallet::call_index(0)]
		#[pallet::weight(1)]
		#[transactional]
		pub fn mint(origin: OriginFor<T>, currencies: Vec<CurrencyOf<T>>) -> DispatchResult {
			ensure_root(origin)?;

			Ok(())
		}
	}
}

#[cfg_attr(test, mockable)]
impl<T: Config> Pallet<T> {}
