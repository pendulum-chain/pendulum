
//! Autogenerated weights for vault_registry
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 4.0.0-dev
//! DATE: 2024-04-16, STEPS: `50`, REPEAT: `20`, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! WORST CASE MAP SIZE: `1000000`
//! HOSTNAME: `Bogdans-M2-MacBook-Pro.local`, CPU: `<UNKNOWN>`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("amplitude"), DB CACHE: 1024

// Executed Command:
// ./target/release/pendulum-node
// benchmark
// pallet
// --chain
// amplitude
// --execution=wasm
// --wasm-execution=compiled
// --pallet
// vault-registry
// --extrinsic
// *
// --steps
// 50
// --repeat
// 20
// --output
// runtime/amplitude/src/weights/
// --template
// .maintain/frame-weight-template.hbs

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weights for vault_registry using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> vault_registry::WeightInfo for SubstrateWeight<T> {
	/// Storage: VaultRegistry SecureCollateralThreshold (r:1 w:0)
	/// Proof Skipped: VaultRegistry SecureCollateralThreshold (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry PremiumRedeemThreshold (r:1 w:0)
	/// Proof Skipped: VaultRegistry PremiumRedeemThreshold (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry LiquidationCollateralThreshold (r:1 w:0)
	/// Proof Skipped: VaultRegistry LiquidationCollateralThreshold (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry MinimumCollateralVault (r:1 w:0)
	/// Proof Skipped: VaultRegistry MinimumCollateralVault (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry SystemCollateralCeiling (r:1 w:0)
	/// Proof Skipped: VaultRegistry SystemCollateralCeiling (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry VaultStellarPublicKey (r:1 w:0)
	/// Proof Skipped: VaultRegistry VaultStellarPublicKey (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry Vaults (r:1 w:1)
	/// Proof Skipped: VaultRegistry Vaults (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry TotalUserVaultCollateral (r:1 w:1)
	/// Proof Skipped: VaultRegistry TotalUserVaultCollateral (max_values: None, max_size: None, mode: Measured)
	/// Storage: Tokens Accounts (r:1 w:1)
	/// Proof: Tokens Accounts (max_values: None, max_size: Some(150), added: 2625, mode: MaxEncodedLen)
	/// Storage: VaultStaking RewardCurrencies (r:1 w:0)
	/// Proof Skipped: VaultStaking RewardCurrencies (max_values: Some(1), max_size: None, mode: Measured)
	/// Storage: PooledVaultRewards Stake (r:1 w:1)
	/// Proof: PooledVaultRewards Stake (max_values: None, max_size: Some(202), added: 2677, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardPerToken (r:1 w:0)
	/// Proof: PooledVaultRewards RewardPerToken (max_values: None, max_size: Some(140), added: 2615, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardTally (r:1 w:1)
	/// Proof: PooledVaultRewards RewardTally (max_values: None, max_size: Some(264), added: 2739, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards TotalRewards (r:1 w:1)
	/// Proof: PooledVaultRewards TotalRewards (max_values: None, max_size: Some(78), added: 2553, mode: MaxEncodedLen)
	/// Storage: VaultStaking Nonce (r:1 w:0)
	/// Proof Skipped: VaultStaking Nonce (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking TotalCurrentStake (r:1 w:1)
	/// Proof Skipped: VaultStaking TotalCurrentStake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking Stake (r:1 w:1)
	/// Proof Skipped: VaultStaking Stake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking SlashPerToken (r:1 w:0)
	/// Proof Skipped: VaultStaking SlashPerToken (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking SlashTally (r:1 w:1)
	/// Proof Skipped: VaultStaking SlashTally (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking TotalStake (r:1 w:1)
	/// Proof Skipped: VaultStaking TotalStake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking RewardTally (r:1 w:1)
	/// Proof Skipped: VaultStaking RewardTally (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking RewardPerToken (r:1 w:0)
	/// Proof Skipped: VaultStaking RewardPerToken (max_values: None, max_size: None, mode: Measured)
	/// Storage: PooledVaultRewards TotalStake (r:1 w:1)
	/// Proof: PooledVaultRewards TotalStake (max_values: None, max_size: Some(78), added: 2553, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardCurrencies (r:1 w:0)
	/// Proof: PooledVaultRewards RewardCurrencies (max_values: None, max_size: Some(523), added: 2998, mode: MaxEncodedLen)
	fn register_vault() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1079`
		//  Estimated: `4544`
		// Minimum execution time: 154_000_000 picoseconds.
		Weight::from_parts(159_000_000, 4544)
			.saturating_add(T::DbWeight::get().reads(24_u64))
			.saturating_add(T::DbWeight::get().writes(12_u64))
	}
	/// Storage: VaultRegistry Vaults (r:1 w:0)
	/// Proof Skipped: VaultRegistry Vaults (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry TotalUserVaultCollateral (r:1 w:1)
	/// Proof Skipped: VaultRegistry TotalUserVaultCollateral (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry SystemCollateralCeiling (r:1 w:0)
	/// Proof Skipped: VaultRegistry SystemCollateralCeiling (max_values: None, max_size: None, mode: Measured)
	/// Storage: Tokens Accounts (r:1 w:1)
	/// Proof: Tokens Accounts (max_values: None, max_size: Some(150), added: 2625, mode: MaxEncodedLen)
	/// Storage: VaultStaking RewardCurrencies (r:1 w:0)
	/// Proof Skipped: VaultStaking RewardCurrencies (max_values: Some(1), max_size: None, mode: Measured)
	/// Storage: PooledVaultRewards Stake (r:1 w:1)
	/// Proof: PooledVaultRewards Stake (max_values: None, max_size: Some(202), added: 2677, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardPerToken (r:1 w:0)
	/// Proof: PooledVaultRewards RewardPerToken (max_values: None, max_size: Some(140), added: 2615, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardTally (r:1 w:1)
	/// Proof: PooledVaultRewards RewardTally (max_values: None, max_size: Some(264), added: 2739, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards TotalRewards (r:1 w:1)
	/// Proof: PooledVaultRewards TotalRewards (max_values: None, max_size: Some(78), added: 2553, mode: MaxEncodedLen)
	/// Storage: VaultStaking Nonce (r:1 w:0)
	/// Proof Skipped: VaultStaking Nonce (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking TotalCurrentStake (r:1 w:1)
	/// Proof Skipped: VaultStaking TotalCurrentStake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking RewardPerToken (r:1 w:1)
	/// Proof Skipped: VaultStaking RewardPerToken (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking Stake (r:1 w:1)
	/// Proof Skipped: VaultStaking Stake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking SlashPerToken (r:1 w:0)
	/// Proof Skipped: VaultStaking SlashPerToken (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking SlashTally (r:1 w:1)
	/// Proof Skipped: VaultStaking SlashTally (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking TotalStake (r:1 w:1)
	/// Proof Skipped: VaultStaking TotalStake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking RewardTally (r:1 w:1)
	/// Proof Skipped: VaultStaking RewardTally (max_values: None, max_size: None, mode: Measured)
	/// Storage: PooledVaultRewards TotalStake (r:1 w:1)
	/// Proof: PooledVaultRewards TotalStake (max_values: None, max_size: Some(78), added: 2553, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardCurrencies (r:1 w:0)
	/// Proof: PooledVaultRewards RewardCurrencies (max_values: None, max_size: Some(523), added: 2998, mode: MaxEncodedLen)
	/// Storage: VaultRegistry SecureCollateralThreshold (r:1 w:0)
	/// Proof Skipped: VaultRegistry SecureCollateralThreshold (max_values: None, max_size: None, mode: Measured)
	/// Storage: Security ParachainStatus (r:1 w:0)
	/// Proof Skipped: Security ParachainStatus (max_values: Some(1), max_size: None, mode: Measured)
	fn deposit_collateral() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `2501`
		//  Estimated: `5966`
		// Minimum execution time: 180_000_000 picoseconds.
		Weight::from_parts(186_000_000, 5966)
			.saturating_add(T::DbWeight::get().reads(21_u64))
			.saturating_add(T::DbWeight::get().writes(12_u64))
	}
	/// Storage: VaultRegistry Vaults (r:1 w:0)
	/// Proof Skipped: VaultRegistry Vaults (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking Nonce (r:1 w:0)
	/// Proof Skipped: VaultStaking Nonce (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking TotalCurrentStake (r:1 w:1)
	/// Proof Skipped: VaultStaking TotalCurrentStake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry SecureCollateralThreshold (r:1 w:0)
	/// Proof Skipped: VaultRegistry SecureCollateralThreshold (max_values: None, max_size: None, mode: Measured)
	/// Storage: Security ParachainStatus (r:1 w:0)
	/// Proof Skipped: Security ParachainStatus (max_values: Some(1), max_size: None, mode: Measured)
	/// Storage: VaultStaking Stake (r:1 w:1)
	/// Proof Skipped: VaultStaking Stake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking SlashPerToken (r:1 w:0)
	/// Proof Skipped: VaultStaking SlashPerToken (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking SlashTally (r:1 w:1)
	/// Proof Skipped: VaultStaking SlashTally (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry PremiumRedeemThreshold (r:1 w:0)
	/// Proof Skipped: VaultRegistry PremiumRedeemThreshold (max_values: None, max_size: None, mode: Measured)
	/// Storage: Tokens Accounts (r:1 w:1)
	/// Proof: Tokens Accounts (max_values: None, max_size: Some(150), added: 2625, mode: MaxEncodedLen)
	/// Storage: VaultRegistry TotalUserVaultCollateral (r:1 w:1)
	/// Proof Skipped: VaultRegistry TotalUserVaultCollateral (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking RewardCurrencies (r:1 w:0)
	/// Proof Skipped: VaultStaking RewardCurrencies (max_values: Some(1), max_size: None, mode: Measured)
	/// Storage: PooledVaultRewards Stake (r:1 w:1)
	/// Proof: PooledVaultRewards Stake (max_values: None, max_size: Some(202), added: 2677, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardPerToken (r:1 w:0)
	/// Proof: PooledVaultRewards RewardPerToken (max_values: None, max_size: Some(140), added: 2615, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardTally (r:1 w:1)
	/// Proof: PooledVaultRewards RewardTally (max_values: None, max_size: Some(264), added: 2739, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards TotalRewards (r:1 w:1)
	/// Proof: PooledVaultRewards TotalRewards (max_values: None, max_size: Some(78), added: 2553, mode: MaxEncodedLen)
	/// Storage: VaultStaking RewardPerToken (r:1 w:1)
	/// Proof Skipped: VaultStaking RewardPerToken (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking TotalStake (r:1 w:1)
	/// Proof Skipped: VaultStaking TotalStake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking RewardTally (r:1 w:1)
	/// Proof Skipped: VaultStaking RewardTally (max_values: None, max_size: None, mode: Measured)
	/// Storage: PooledVaultRewards TotalStake (r:1 w:1)
	/// Proof: PooledVaultRewards TotalStake (max_values: None, max_size: Some(78), added: 2553, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardCurrencies (r:1 w:0)
	/// Proof: PooledVaultRewards RewardCurrencies (max_values: None, max_size: Some(523), added: 2998, mode: MaxEncodedLen)
	fn withdraw_collateral() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `2475`
		//  Estimated: `5940`
		// Minimum execution time: 182_000_000 picoseconds.
		Weight::from_parts(188_000_000, 5940)
			.saturating_add(T::DbWeight::get().reads(21_u64))
			.saturating_add(T::DbWeight::get().writes(12_u64))
	}
	/// Storage: VaultRegistry VaultStellarPublicKey (r:1 w:1)
	/// Proof Skipped: VaultRegistry VaultStellarPublicKey (max_values: None, max_size: None, mode: Measured)
	fn register_public_key() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `334`
		//  Estimated: `3799`
		// Minimum execution time: 15_000_000 picoseconds.
		Weight::from_parts(16_000_000, 3799)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: VaultRegistry Vaults (r:1 w:1)
	/// Proof Skipped: VaultRegistry Vaults (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking RewardCurrencies (r:1 w:0)
	/// Proof Skipped: VaultStaking RewardCurrencies (max_values: Some(1), max_size: None, mode: Measured)
	/// Storage: PooledVaultRewards Stake (r:1 w:0)
	/// Proof: PooledVaultRewards Stake (max_values: None, max_size: Some(202), added: 2677, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardPerToken (r:1 w:0)
	/// Proof: PooledVaultRewards RewardPerToken (max_values: None, max_size: Some(140), added: 2615, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardTally (r:1 w:1)
	/// Proof: PooledVaultRewards RewardTally (max_values: None, max_size: Some(264), added: 2739, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards TotalRewards (r:1 w:1)
	/// Proof: PooledVaultRewards TotalRewards (max_values: None, max_size: Some(78), added: 2553, mode: MaxEncodedLen)
	/// Storage: VaultStaking Nonce (r:1 w:0)
	/// Proof Skipped: VaultStaking Nonce (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking TotalCurrentStake (r:1 w:0)
	/// Proof Skipped: VaultStaking TotalCurrentStake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking RewardPerToken (r:1 w:1)
	/// Proof Skipped: VaultStaking RewardPerToken (max_values: None, max_size: None, mode: Measured)
	fn accept_new_issues() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `1484`
		//  Estimated: `4949`
		// Minimum execution time: 59_000_000 picoseconds.
		Weight::from_parts(61_000_000, 4949)
			.saturating_add(T::DbWeight::get().reads(9_u64))
			.saturating_add(T::DbWeight::get().writes(4_u64))
	}
	/// Storage: VaultRegistry SecureCollateralThreshold (r:1 w:0)
	/// Proof Skipped: VaultRegistry SecureCollateralThreshold (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry Vaults (r:1 w:1)
	/// Proof Skipped: VaultRegistry Vaults (max_values: None, max_size: None, mode: Measured)
	fn set_custom_secure_threshold() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `711`
		//  Estimated: `4176`
		// Minimum execution time: 17_000_000 picoseconds.
		Weight::from_parts(17_000_000, 4176)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: VaultRegistry MinimumCollateralVault (r:0 w:1)
	/// Proof Skipped: VaultRegistry MinimumCollateralVault (max_values: None, max_size: None, mode: Measured)
	fn set_minimum_collateral() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 6_000_000 picoseconds.
		Weight::from_parts(6_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: VaultRegistry SystemCollateralCeiling (r:0 w:1)
	/// Proof Skipped: VaultRegistry SystemCollateralCeiling (max_values: None, max_size: None, mode: Measured)
	fn set_system_collateral_ceiling() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 6_000_000 picoseconds.
		Weight::from_parts(6_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: VaultStaking RewardCurrencies (r:1 w:1)
	/// Proof Skipped: VaultStaking RewardCurrencies (max_values: Some(1), max_size: None, mode: Measured)
	/// Storage: VaultRegistry SecureCollateralThreshold (r:0 w:1)
	/// Proof Skipped: VaultRegistry SecureCollateralThreshold (max_values: None, max_size: None, mode: Measured)
	fn set_secure_collateral_threshold() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `80`
		//  Estimated: `1565`
		// Minimum execution time: 10_000_000 picoseconds.
		Weight::from_parts(11_000_000, 1565)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(2_u64))
	}
	/// Storage: VaultRegistry PremiumRedeemThreshold (r:0 w:1)
	/// Proof Skipped: VaultRegistry PremiumRedeemThreshold (max_values: None, max_size: None, mode: Measured)
	fn set_premium_redeem_threshold() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 6_000_000 picoseconds.
		Weight::from_parts(6_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: VaultRegistry LiquidationCollateralThreshold (r:0 w:1)
	/// Proof Skipped: VaultRegistry LiquidationCollateralThreshold (max_values: None, max_size: None, mode: Measured)
	fn set_liquidation_collateral_threshold() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 6_000_000 picoseconds.
		Weight::from_parts(6_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: VaultRegistry Vaults (r:1 w:1)
	/// Proof Skipped: VaultRegistry Vaults (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry LiquidationCollateralThreshold (r:1 w:0)
	/// Proof Skipped: VaultRegistry LiquidationCollateralThreshold (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking Nonce (r:1 w:0)
	/// Proof Skipped: VaultStaking Nonce (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking TotalCurrentStake (r:1 w:1)
	/// Proof Skipped: VaultStaking TotalCurrentStake (max_values: None, max_size: None, mode: Measured)
	/// Storage: Security ParachainStatus (r:1 w:0)
	/// Proof Skipped: Security ParachainStatus (max_values: Some(1), max_size: None, mode: Measured)
	/// Storage: VaultStaking Stake (r:1 w:1)
	/// Proof Skipped: VaultStaking Stake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking SlashPerToken (r:1 w:0)
	/// Proof Skipped: VaultStaking SlashPerToken (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking SlashTally (r:1 w:1)
	/// Proof Skipped: VaultStaking SlashTally (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking RewardCurrencies (r:1 w:0)
	/// Proof Skipped: VaultStaking RewardCurrencies (max_values: Some(1), max_size: None, mode: Measured)
	/// Storage: PooledVaultRewards Stake (r:1 w:1)
	/// Proof: PooledVaultRewards Stake (max_values: None, max_size: Some(202), added: 2677, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardPerToken (r:1 w:0)
	/// Proof: PooledVaultRewards RewardPerToken (max_values: None, max_size: Some(140), added: 2615, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardTally (r:1 w:1)
	/// Proof: PooledVaultRewards RewardTally (max_values: None, max_size: Some(264), added: 2739, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards TotalRewards (r:1 w:1)
	/// Proof: PooledVaultRewards TotalRewards (max_values: None, max_size: Some(78), added: 2553, mode: MaxEncodedLen)
	/// Storage: VaultStaking RewardPerToken (r:1 w:1)
	/// Proof Skipped: VaultStaking RewardPerToken (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking TotalStake (r:1 w:1)
	/// Proof Skipped: VaultStaking TotalStake (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultStaking RewardTally (r:1 w:1)
	/// Proof Skipped: VaultStaking RewardTally (max_values: None, max_size: None, mode: Measured)
	/// Storage: PooledVaultRewards TotalStake (r:1 w:1)
	/// Proof: PooledVaultRewards TotalStake (max_values: None, max_size: Some(78), added: 2553, mode: MaxEncodedLen)
	/// Storage: PooledVaultRewards RewardCurrencies (r:1 w:0)
	/// Proof: PooledVaultRewards RewardCurrencies (max_values: None, max_size: Some(523), added: 2998, mode: MaxEncodedLen)
	/// Storage: VaultRegistry TotalUserVaultCollateral (r:1 w:1)
	/// Proof Skipped: VaultRegistry TotalUserVaultCollateral (max_values: None, max_size: None, mode: Measured)
	/// Storage: Tokens Accounts (r:2 w:2)
	/// Proof: Tokens Accounts (max_values: None, max_size: Some(150), added: 2625, mode: MaxEncodedLen)
	/// Storage: System Account (r:2 w:1)
	/// Proof: System Account (max_values: None, max_size: Some(128), added: 2603, mode: MaxEncodedLen)
	/// Storage: VaultRegistry SystemCollateralCeiling (r:1 w:0)
	/// Proof Skipped: VaultRegistry SystemCollateralCeiling (max_values: None, max_size: None, mode: Measured)
	/// Storage: VaultRegistry LiquidationVault (r:1 w:1)
	/// Proof Skipped: VaultRegistry LiquidationVault (max_values: None, max_size: None, mode: Measured)
	fn report_undercollateralized_vault() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `2675`
		//  Estimated: `6240`
		// Minimum execution time: 330_000_000 picoseconds.
		Weight::from_parts(336_000_000, 6240)
			.saturating_add(T::DbWeight::get().reads(25_u64))
			.saturating_add(T::DbWeight::get().writes(16_u64))
	}
	/// Storage: VaultRegistry Vaults (r:1 w:1)
	/// Proof Skipped: VaultRegistry Vaults (max_values: None, max_size: None, mode: Measured)
	fn recover_vault_id() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `649`
		//  Estimated: `4114`
		// Minimum execution time: 16_000_000 picoseconds.
		Weight::from_parts(16_000_000, 4114)
			.saturating_add(T::DbWeight::get().reads(1_u64))
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
	/// Storage: VaultRegistry PunishmentDelay (r:0 w:1)
	/// Proof Skipped: VaultRegistry PunishmentDelay (max_values: Some(1), max_size: None, mode: Measured)
	fn set_punishment_delay() -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 4_000_000 picoseconds.
		Weight::from_parts(5_000_000, 0)
			.saturating_add(T::DbWeight::get().writes(1_u64))
	}
}