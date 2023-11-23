//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::sync::Arc;

pub use sc_rpc::{DenyUnsafe, SubscriptionTaskExecutor};
use sc_transaction_pool_api::TransactionPool;

use bifrost_farming_rpc_api::{FarmingRpc, FarmingRpcApiServer};

use zenlink_protocol_rpc::{ZenlinkProtocol, ZenlinkProtocolApiServer};

use module_issue_rpc::{Issue, IssueApiServer};
use module_oracle_rpc::{Oracle, OracleApiServer};
use module_pallet_staking_rpc::{ParachainStakingApiServer, Staking};
use module_redeem_rpc::{Redeem, RedeemApiServer};
use module_replace_rpc::{Replace, ReplaceApiServer};
use module_vault_registry_rpc::{VaultRegistry, VaultRegistryApiServer};
use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
use substrate_frame_rpc_system::{System, SystemApiServer};

use crate::service::{AmplitudeClient, DevelopmentClient, FoucocoClient, PendulumClient};

/// A type representing all RPC extensions.
type RpcExtension = jsonrpsee::RpcModule<()>;

/// RpcExtension wrapped in Result<>
pub type ResultRpcExtension = Result<RpcExtension, Box<dyn std::error::Error + Send + Sync>>;

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
pub fn create_full_pendulum<P>(deps: FullDeps<PendulumClient, P>) -> ResultRpcExtension
where
	P: TransactionPool + Sync + Send + 'static,
{
	let mut module = RpcExtension::new(());
	let FullDeps { client, pool, deny_unsafe } = deps;

	module.merge(System::new(client.clone(), pool, deny_unsafe).into_rpc())?;
	module.merge(Staking::new(client.clone()).into_rpc())?;
	module.merge(TransactionPayment::new(client.clone()).into_rpc())?;
	module.merge(ZenlinkProtocol::new(client).into_rpc())?;
	Ok(module)
}

/// Instantiate all RPC extensions.
pub fn create_full_development<P>(
	deps: FullDeps<DevelopmentClient, P>,
) -> Result<RpcExtension, Box<dyn std::error::Error + Send + Sync>>
where
	P: TransactionPool + Sync + Send + 'static,
{
	let mut module = RpcExtension::new(());
	let FullDeps { client, pool, deny_unsafe } = deps;

	module.merge(System::new(client.clone(), pool, deny_unsafe).into_rpc())?;
	module.merge(TransactionPayment::new(client).into_rpc())?;
	Ok(module)
}

pub fn create_full_amplitude<P>(deps: FullDeps<AmplitudeClient, P>) -> ResultRpcExtension
where
	P: TransactionPool + Sync + Send + 'static,
{
	let mut module = RpcExtension::new(());
	let FullDeps { client, pool, deny_unsafe } = deps;

	module.merge(System::new(client.clone(), pool, deny_unsafe).into_rpc())?;
	module.merge(Staking::new(client.clone()).into_rpc())?;
	module.merge(TransactionPayment::new(client.clone()).into_rpc())?;
	module.merge(Issue::new(client.clone()).into_rpc())?;
	module.merge(Redeem::new(client.clone()).into_rpc())?;
	module.merge(Replace::new(client.clone()).into_rpc())?;
	module.merge(VaultRegistry::new(client.clone()).into_rpc())?;
	module.merge(Oracle::new(client.clone()).into_rpc())?;
	module.merge(FarmingRpc::new(client.clone()).into_rpc())?;
	module.merge(ZenlinkProtocol::new(client).into_rpc())?;

	Ok(module)
}

pub fn create_full_foucoco<P>(deps: FullDeps<FoucocoClient, P>) -> ResultRpcExtension
where
	P: TransactionPool + Sync + Send + 'static,
{
	let mut module = RpcExtension::new(());
	let FullDeps { client, pool, deny_unsafe } = deps;

	module.merge(System::new(client.clone(), pool, deny_unsafe).into_rpc())?;
	module.merge(Staking::new(client.clone()).into_rpc())?;
	module.merge(TransactionPayment::new(client.clone()).into_rpc())?;
	module.merge(Issue::new(client.clone()).into_rpc())?;
	module.merge(Redeem::new(client.clone()).into_rpc())?;
	module.merge(Replace::new(client.clone()).into_rpc())?;
	module.merge(VaultRegistry::new(client.clone()).into_rpc())?;
	module.merge(Oracle::new(client.clone()).into_rpc())?;
	module.merge(FarmingRpc::new(client.clone()).into_rpc())?;
	module.merge(ZenlinkProtocol::new(client).into_rpc())?;

	Ok(module)
}
