
//! Autogenerated weights for orml_currencies_allowance_extension
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2024-03-13, STEPS: `50`, REPEAT: `20`, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! WORST CASE MAP SIZE: `1000000`
//! HOSTNAME: `Bogdans-M2-MacBook-Pro.local`, CPU: `<UNKNOWN>`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("foucoco"), DB CACHE: 1024

// Executed Command:
// ./target/release/pendulum-node
// benchmark
// pallet
// --chain
// foucoco
// --execution=wasm
// --wasm-execution=compiled
// --pallet
// orml-currencies-allowance-extension
// --extrinsic
// *
// --steps
// 50
// --repeat
// 20
// --output
// runtime/foucoco/src/weights/orml-currencies-allowance-extension.rs
// --template
// .maintain/frame-weight-template.hbs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weights for orml_currencies_allowance_extension using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> orml_currencies_allowance_extension::default_weights::WeightInfo for SubstrateWeight<T> {
	/// Storage: TokenAllowance AllowedCurrencies (r:2 w:1)
	/// Proof Skipped: TokenAllowance AllowedCurrencies (max_values: None, max_size: None, mode: Measured)
	/// The range of component `n` is `[1, 256]`.
	fn add_allowed_currencies(n: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `142`
		//  Estimated: `6082`
		// Minimum execution time: 17_000_000 picoseconds.
		Weight::from_parts(16_786_577, 6082)
			// Standard Error: 2_416
			.saturating_add(Weight::from_parts(1_273_968, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: TokenAllowance AllowedCurrencies (r:0 w:1)
	/// Proof Skipped: TokenAllowance AllowedCurrencies (max_values: None, max_size: None, mode: Measured)
	/// The range of component `n` is `[1, 256]`.
	fn remove_allowed_currencies(n: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 11_000_000 picoseconds.
		Weight::from_parts(11_332_861, 0)
			// Standard Error: 1_861
			.saturating_add(Weight::from_parts(1_244_517, 0).saturating_mul(n.into()))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: TokenAllowance AllowedCurrencies (r:1 w:0)
	/// Proof Skipped: TokenAllowance AllowedCurrencies (max_values: None, max_size: None, mode: Measured)
	/// Storage: TokenAllowance Approvals (r:0 w:1)
	/// Proof Skipped: TokenAllowance Approvals (max_values: None, max_size: None, mode: Measured)
	fn approve() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `184`
		//  Estimated: `3833`
		// Minimum execution time: 17_000_000 picoseconds.
		Weight::from_parts(18_000_000, 3833)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: TokenAllowance AllowedCurrencies (r:1 w:0)
	/// Proof Skipped: TokenAllowance AllowedCurrencies (max_values: None, max_size: None, mode: Measured)
	/// Storage: TokenAllowance Approvals (r:1 w:1)
	/// Proof Skipped: TokenAllowance Approvals (max_values: None, max_size: None, mode: Measured)
	/// Storage: System Account (r:2 w:2)
	/// Proof: System Account (max_values: None, max_size: Some(128), added: 2603, mode: MaxEncodedLen)
	fn transfer_from() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `490`
		//  Estimated: `14106`
		// Minimum execution time: 49_000_000 picoseconds.
		Weight::from_parts(50_000_000, 14106)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
	}
}