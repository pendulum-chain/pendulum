
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
	pub trait Config: frame_system::Config {
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

	#[pallet::hooks]
	impl<T: Config> Hooks<T::BlockNumber> for Pallet<T> {}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self {  }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
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