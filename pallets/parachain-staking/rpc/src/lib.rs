//! RPC interface for the parachain staking pallet.

use codec::Codec;
use jsonrpsee::{
	core::{async_trait, Error as JsonRpseeError, RpcResult},
	proc_macros::rpc,
	types::error::{CallError, ErrorCode, ErrorObject},
};
use module_oracle_rpc_runtime_api::BalanceWrapper;
use module_pallet_staking_rpc_runtime_api::{
	ParachainStakingApi as ParachainStakingRuntimeApi, StakingRates,
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::{Block as BlockT, MaybeDisplay, MaybeFromStr};
use std::sync::Arc;

#[rpc(client, server)]
pub trait ParachainStakingApi<BlockHash, AccountId, Balance>
where
	Balance: Codec + MaybeDisplay + MaybeFromStr,
	AccountId: Codec,
{
	#[method(name = "staking_getUnclaimedStakingRewards")]
	fn get_unclaimed_staking_rewards(
		&self,
		account: AccountId,
		at: Option<BlockHash>,
	) -> RpcResult<BalanceWrapper<Balance>>;

	#[method(name = "staking_getStakingRates")]
	fn get_staking_rates(&self, at: Option<BlockHash>) -> RpcResult<StakingRates>;
}

fn internal_err<T: ToString>(message: T) -> JsonRpseeError {
	JsonRpseeError::Call(CallError::Custom(ErrorObject::owned(
		ErrorCode::InternalError.code(),
		message.to_string(),
		None::<()>,
	)))
}

/// A struct that implements the [`Staking`].
pub struct Staking<C, B> {
	client: Arc<C>,
	_marker: std::marker::PhantomData<B>,
}

impl<C, B> Staking<C, B> {
	/// Create new `Staking` with the given reference to the client.
	pub fn new(client: Arc<C>) -> Self {
		Staking { client, _marker: Default::default() }
	}
}

#[async_trait]
impl<C, Block, AccountId, Balance>
	ParachainStakingApiServer<<Block as BlockT>::Hash, AccountId, Balance> for Staking<C, Block>
where
	Block: BlockT,
	C: Send + Sync + 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	C::Api: ParachainStakingRuntimeApi<Block, AccountId, Balance>,
	Balance: Codec + MaybeDisplay + MaybeFromStr,
	AccountId: Codec,
{
	fn get_unclaimed_staking_rewards(
		&self,
		account: AccountId,
		at: Option<<Block as BlockT>::Hash>,
	) -> RpcResult<BalanceWrapper<Balance>> {
		let api = self.client.runtime_api();
		let at = at.unwrap_or_else(|| self.client.info().best_hash);

		api.get_unclaimed_staking_rewards(at, account)
			.map_err(|_e| internal_err("Unable to get unclaimed staking rewards"))
	}

	fn get_staking_rates(&self, at: Option<<Block as BlockT>::Hash>) -> RpcResult<StakingRates> {
		let api = self.client.runtime_api();
		let at = at.unwrap_or_else(|| self.client.info().best_hash);

		api.get_staking_rates(at)
			.map_err(|_e| internal_err("Unable to get staking rates"))
	}
}
