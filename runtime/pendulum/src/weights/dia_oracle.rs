
//! Autogenerated weights for dia_oracle
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2024-09-11, STEPS: `50`, REPEAT: `20`, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! WORST CASE MAP SIZE: `1000000`
//! HOSTNAME: `Gianfrancos-MacBook-Pro.local`, CPU: `<UNKNOWN>`
//! EXECUTION: , WASM-EXECUTION: Compiled, CHAIN: Some("pendulum"), DB CACHE: 1024

// Executed Command:
// ../target/production/pendulum-node
// benchmark
// pallet
// --chain
// pendulum
// --wasm-execution=compiled
// --pallet
// dia_oracle
// --extrinsic
// *
// --steps
// 50
// --repeat
// 20
// --output
// ../runtime/pendulum/src/weights/dia_oracle.rs
// --template
// frame-weight-template.hbs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weights for dia_oracle using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> dia_oracle::WeightInfo for SubstrateWeight<T> {
	/// Storage: `DiaOracleModule::AuthorizedAccounts` (r:1 w:0)
	/// Proof: `DiaOracleModule::AuthorizedAccounts` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `DiaOracleModule::SupportedCurrencies` (r:1 w:1)
	/// Proof: `DiaOracleModule::SupportedCurrencies` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn add_currency() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `402`
		//  Estimated: `3867`
		// Minimum execution time: 15_000_000 picoseconds.
		Weight::from_parts(15_000_000, 3867)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `DiaOracleModule::AuthorizedAccounts` (r:1 w:0)
	/// Proof: `DiaOracleModule::AuthorizedAccounts` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `DiaOracleModule::SupportedCurrencies` (r:1 w:0)
	/// Proof: `DiaOracleModule::SupportedCurrencies` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn remove_currency() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `402`
		//  Estimated: `3867`
		// Minimum execution time: 10_000_000 picoseconds.
		Weight::from_parts(11_000_000, 3867)
			.saturating_add(T::DbWeight::get().reads(2_u64))
	}
	/// Storage: `DiaOracleModule::AuthorizedAccounts` (r:1 w:1)
	/// Proof: `DiaOracleModule::AuthorizedAccounts` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn authorize_account() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `232`
		//  Estimated: `3697`
		// Minimum execution time: 10_000_000 picoseconds.
		Weight::from_parts(10_000_000, 3697)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `DiaOracleModule::AuthorizedAccounts` (r:2 w:1)
	/// Proof: `DiaOracleModule::AuthorizedAccounts` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn authorize_account_signed() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `287`
		//  Estimated: `6227`
		// Minimum execution time: 12_000_000 picoseconds.
		Weight::from_parts(12_000_000, 6227)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `DiaOracleModule::AuthorizedAccounts` (r:1 w:0)
	/// Proof: `DiaOracleModule::AuthorizedAccounts` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn deauthorize_account() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `232`
		//  Estimated: `3697`
		// Minimum execution time: 5_000_000 picoseconds.
		Weight::from_parts(6_000_000, 3697)
			.saturating_add(T::DbWeight::get().reads(1_u64))
	}
	/// Storage: `DiaOracleModule::AuthorizedAccounts` (r:2 w:1)
	/// Proof: `DiaOracleModule::AuthorizedAccounts` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn deauthorize_account_signed() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `287`
		//  Estimated: `6227`
		// Minimum execution time: 12_000_000 picoseconds.
		Weight::from_parts(12_000_000, 6227)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `DiaOracleModule::AuthorizedAccounts` (r:1 w:0)
	/// Proof: `DiaOracleModule::AuthorizedAccounts` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `DiaOracleModule::CoinInfosMap` (r:0 w:1)
	/// Proof: `DiaOracleModule::CoinInfosMap` (`max_values`: None, `max_size`: None, mode: `Measured`)
	fn set_updated_coin_infos() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `287`
		//  Estimated: `3752`
		// Minimum execution time: 9_864_000_000 picoseconds.
		Weight::from_parts(9_973_000_000, 3752)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: `DiaOracleModule::AuthorizedAccounts` (r:1 w:0)
	/// Proof: `DiaOracleModule::AuthorizedAccounts` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `DiaOracleModule::BatchingApi` (r:0 w:1)
	/// Proof: `DiaOracleModule::BatchingApi` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	fn set_batching_api() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `287`
		//  Estimated: `3752`
		// Minimum execution time: 10_000_000 picoseconds.
		Weight::from_parts(11_000_000, 3752)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
}