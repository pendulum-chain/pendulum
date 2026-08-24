//! Default weights for the token-migration pallet.
//!
//! TODO: replace with generated weights once benchmarks are added; these are
//! conservative manual estimates in the meantime (same approach as the other
//! pallets in this repo).

use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
	fn migrate() -> Weight;
	fn set_paused() -> Weight;
	fn set_treasury_destination() -> Weight;
	fn migrate_treasury() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn migrate() -> Weight {
		Weight::from_parts(50_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(4_u64))
	}

	fn set_paused() -> Weight {
		Weight::from_parts(10_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}

	fn set_treasury_destination() -> Weight {
		Weight::from_parts(12_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}

	fn migrate_treasury() -> Weight {
		Weight::from_parts(50_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(5_u64))
			.saturating_add(T::DbWeight::get().writes(4_u64))
	}
}
