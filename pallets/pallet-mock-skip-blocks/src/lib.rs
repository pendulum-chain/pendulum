#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "instant-seal")]
pub use pallet::*;

#[cfg(not(feature = "instant-seal"))]
pub use dummy as pallet;

#[cfg(not(feature = "instant-seal"))]
#[frame_support::pallet]
pub mod dummy {
	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Dummy event
		DummyEvent,
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "instant-seal")]
#[frame_support::pallet]
pub mod pallet {
	use frame_support::{dispatch::DispatchResult, pallet_prelude::*};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::error]
	pub enum Error<T> {
		InvalidBlockNumber,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Desired block number stored
		DesiredBlockStored { n: BlockNumberFor<T> },
		/// Desired block number set
		BlockSet { n: BlockNumberFor<T> },
		/// Original block number restored
		BlockReverted { n: BlockNumberFor<T> },
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: T::BlockNumber) -> Weight {
			let desired_block_number = DesiredBlockNumber::<T>::get();
			OriginalBlockNumber::<T>::put(n);
			frame_system::Pallet::<T>::set_block_number(desired_block_number);
			Self::deposit_event(Event::<T>::BlockSet { n: desired_block_number });
			0.into()
		}

		fn on_finalize(_: T::BlockNumber) {
			let original_block_number = OriginalBlockNumber::<T>::get();
			frame_system::Pallet::<T>::set_block_number(original_block_number);
			Self::deposit_event(Event::<T>::BlockReverted { n: original_block_number });
		}
	}

	#[pallet::storage]
	pub type DesiredBlockNumber<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	#[pallet::storage]
	pub type OriginalBlockNumber<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery>;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight((0, Pays::No))]
		pub fn set_block_number(
			origin: OriginFor<T>,
			block_number: T::BlockNumber,
		) -> DispatchResult {
			ensure_root(origin)?;

			let current_block_number = frame_system::Pallet::<T>::block_number();
			ensure!(block_number >= current_block_number, Error::<T>::InvalidBlockNumber);

			DesiredBlockNumber::<T>::put(block_number);
			Self::deposit_event(Event::<T>::DesiredBlockStored { n: block_number });

			Ok(())
		}
	}
}
