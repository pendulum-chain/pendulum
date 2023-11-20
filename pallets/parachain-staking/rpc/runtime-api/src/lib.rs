//! Runtime API definition for Parachain Staking.

#![cfg_attr(not(feature = "std"), no_std)]
use frame_support::{scale_info::TypeInfo, sp_runtime::Perquintill};
use module_oracle_rpc_runtime_api::BalanceWrapper;
use parity_scale_codec::{Codec, Decode, Encode, MaxEncodedLen};
use sp_std::fmt::Debug;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

#[derive(Decode, Encode, TypeInfo, MaxEncodedLen, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct StakingRates {
	pub collator_staking_rate: Perquintill,
	pub collator_reward_rate: Perquintill,
	pub delegator_staking_rate: Perquintill,
	pub delegator_reward_rate: Perquintill,
}

sp_api::decl_runtime_apis! {
	pub trait ParachainStakingApi<AccountId, Balance>
	where
		AccountId:  Codec,
		Balance: Codec
	{
		fn get_unclaimed_staking_rewards(account: AccountId) -> BalanceWrapper<Balance>;
		fn get_staking_rates() -> StakingRates;
	}
}
