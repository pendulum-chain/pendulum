//! Runtime API definition for Parachain Staking.

#![cfg_attr(not(feature = "std"), no_std)]
use parity_scale_codec::{Codec, Decode, Encode, MaxEncodedLen};
use frame_support::scale_info::TypeInfo;
use frame_support::sp_runtime::Perquintill;
use sp_std::fmt::Debug;
use module_oracle_rpc_runtime_api::BalanceWrapper;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

#[derive(Decode, Encode, TypeInfo, MaxEncodedLen, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "std", derive( Serialize, Deserialize))]
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
