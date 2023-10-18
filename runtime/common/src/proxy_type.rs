use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use sp_runtime::RuntimeDebug;
/// The type used to represent the kinds of proxying allowed.
#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	RuntimeDebug,
	MaxEncodedLen,
	scale_info::TypeInfo,
)]
pub enum ProxyType {
	/// Allows all runtime calls for proxy account
	Any,
	// /// Allows only NonTransfer runtime calls for proxy account
	// /// To know exact calls check InstanceFilter inmplementation for ProxyTypes
	// NonTransfer,
	// /// All Runtime calls from Pallet Balances allowed for proxy account
	// Balances,
	// /// Only provide_judgement call from pallet identity allowed for proxy account
	// IdentityJudgement,
	// /// Only reject_announcement call from pallet proxy allowed for proxy account
	// CancelProxy,
	// /// Only claim_reward call from pallet staking is allowed for proxy account
	// StakerRewardClaim,
}

impl Default for ProxyType {
	fn default() -> Self {
		Self::Any
	}
}
