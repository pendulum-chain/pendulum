//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::sync::Arc;

use runtime_common::{opaque::Block, AccountId, Balance, Index as Nonce};

use sc_client_api::AuxStore;
pub use sc_rpc::{DenyUnsafe, SubscriptionTaskExecutor};
use sc_transaction_pool_api::TransactionPool;
use sp_api::ProvideRuntimeApi;
use sp_arithmetic::FixedU128;
use sp_block_builder::BlockBuilder;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_core::H256;

use spacewalk_primitives::{
	issue::IssueRequest, redeem::RedeemRequest, replace::ReplaceRequest, BlockNumber, CurrencyId,
	VaultId,
};

use zenlink_protocol_rpc::{ZenlinkProtocol, ZenlinkProtocolApiServer};
use zenlink_protocol_runtime_api::ZenlinkProtocolApi as ZenlinkProtocolRuntimeApi;
use zenlink_protocol::AssetId;

/// A type representing all RPC extensions.
pub type RpcExtension = jsonrpsee::RpcModule<()>;

/// Full client dependencies
pub struct FullDeps<C, P> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// Whether to deny unsafe calls
	pub deny_unsafe: DenyUnsafe,
}

/// Instantiate all RPC extensions.
pub fn create_full<C, P>(
	deps: FullDeps<C, P>,
) -> Result<RpcExtension, Box<dyn std::error::Error + Send + Sync>>
where
	C: ProvideRuntimeApi<Block>
		+ HeaderBackend<Block>
		+ AuxStore
		+ HeaderMetadata<Block, Error = BlockChainError>
		+ Send
		+ Sync
		+ 'static,
	C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
	C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	C::Api: BlockBuilder<Block>,
	P: TransactionPool + Sync + Send + 'static,
{
	use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
	use substrate_frame_rpc_system::{System, SystemApiServer};

	let mut module = RpcExtension::new(());
	let FullDeps { client, pool, deny_unsafe } = deps;

	module.merge(System::new(client.clone(), pool, deny_unsafe).into_rpc())?;
	module.merge(TransactionPayment::new(client).into_rpc())?;
	Ok(module)
}

pub fn create_full_spacewalk<C, P>(
	deps: FullDeps<C, P>,
) -> Result<RpcExtension, Box<dyn std::error::Error + Send + Sync>>
where
	C: ProvideRuntimeApi<Block>
		+ HeaderBackend<Block>
		+ AuxStore
		+ HeaderMetadata<Block, Error = BlockChainError>
		+ Send
		+ Sync
		+ 'static,
	C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
	C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	C::Api: module_vault_registry_rpc::VaultRegistryRuntimeApi<
		Block,
		VaultId<AccountId, CurrencyId>,
		Balance,
		FixedU128,
		CurrencyId,
		AccountId,
	>,
	C::Api: module_replace_rpc::ReplaceRuntimeApi<
		Block,
		AccountId,
		H256,
		ReplaceRequest<AccountId, BlockNumber, Balance, CurrencyId>,
	>,
	C::Api: module_issue_rpc::IssueRuntimeApi<
		Block,
		AccountId,
		H256,
		IssueRequest<AccountId, BlockNumber, Balance, CurrencyId>,
	>,
	C::Api: module_redeem_rpc::RedeemRuntimeApi<
		Block,
		AccountId,
		H256,
		RedeemRequest<AccountId, BlockNumber, Balance, CurrencyId>,
	>,
	C::Api: module_oracle_rpc::OracleRuntimeApi<Block, Balance, CurrencyId>,
	C::Api: BlockBuilder<Block>,
	C::Api: ZenlinkProtocolRuntimeApi<Block, AccountId, AssetId>,
	P: TransactionPool + Sync + Send + 'static,
{
	use module_issue_rpc::{Issue, IssueApiServer};
	use module_oracle_rpc::{Oracle, OracleApiServer};
	use module_redeem_rpc::{Redeem, RedeemApiServer};
	use module_replace_rpc::{Replace, ReplaceApiServer};
	use module_vault_registry_rpc::{VaultRegistry, VaultRegistryApiServer};
	use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
	use substrate_frame_rpc_system::{System, SystemApiServer};

	let mut module = RpcExtension::new(());
	let FullDeps { client, pool, deny_unsafe } = deps;

	module.merge(System::new(client.clone(), pool, deny_unsafe).into_rpc())?;
	module.merge(TransactionPayment::new(client.clone()).into_rpc())?;
	module.merge(Issue::new(client.clone()).into_rpc())?;
	module.merge(Redeem::new(client.clone()).into_rpc())?;
	module.merge(Replace::new(client.clone()).into_rpc())?;
	module.merge(VaultRegistry::new(client.clone()).into_rpc())?;
	module.merge(ZenlinkProtocol::new(client.clone()).into_rpc())?;
	module.merge(Oracle::new(client).into_rpc())?;

	Ok(module)
}
