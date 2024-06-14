#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use frame_support::traits::VestingSchedule;
use sp_runtime::traits::StaticLookup;

type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

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
		type VestingSchedule: VestingSchedule<Self::AccountId>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		VestingScheduleRemoved { who: T::AccountId, schedule_index: u32 },
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		// TODO to remove this manually assigned weight, we need to add benchmarks to the pallet
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64)))]
		pub fn remove_vesting_schedule(
			origin: OriginFor<T>,
			who: AccountIdLookupOf<T>,
			schedule_index: u32,
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			let who = T::Lookup::lookup(who)?;
			T::VestingSchedule::remove_vesting_schedule(&who, schedule_index)?;

			Self::deposit_event(Event::VestingScheduleRemoved { who, schedule_index });

			// waive the fee
			Ok(Pays::No.into())
		}
	}
}
