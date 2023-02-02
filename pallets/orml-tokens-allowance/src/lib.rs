
// #![deny(warnings)]
#![cfg_attr(test, feature(proc_macro_hygiene))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(test)]
extern crate mocktopus;

use codec::Encode;
use frame_support::{
	dispatch::{DispatchError, DispatchResult},
	transactional,
};
#[cfg(test)]
use mocktopus::macros::mockable;
use sha2::{Digest, Sha256};
use sp_core::{H256, U256};
use sp_runtime::{traits::*, ArithmeticError};
use sp_std::{collections::btree_set::BTreeSet, convert::TryInto, prelude::*, vec};

// pub use pallet::*;


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
		UpdateActiveBlock { block_number: T::BlockNumber },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Parachain is not running.
		ParachainNotRunning,
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
	pub(super) type AllowedCurrencies<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::CurrencyId,
		bool,
	>;

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T : Config> {
		pub allowed_currencies : Vec<T::CurrencyId>
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { allowed_currencies : vec![] }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			for i in &self.allowed_currencies.clone(){
				AllowedCurrencies::<T>::insert(i, true);
			}
		}
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	// The pallet's dispatchable functions.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		
	}
}