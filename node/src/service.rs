//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

// std
use std::{sync::Arc, time::Duration};

use cumulus_client_cli::{CollatorOptions, RelayChainMode};
// Local Runtime Types
use runtime_common::{opaque::Block, AccountId, Balance, Index as Nonce};

// Cumulus Imports
use cumulus_client_collator::service::CollatorService;
use cumulus_client_consensus_aura::collators::basic::{
	self as basic_aura, Params as BasicAuraParams,
};
use cumulus_client_consensus_common::ParachainBlockImport as TParachainBlockImport;
use cumulus_client_consensus_proposer::Proposer;
use cumulus_client_network::RequireSecondedInBlockAnnounce;
use cumulus_client_service::{
	prepare_node_config, start_relay_chain_tasks, DARecoveryProfile, StartRelayChainTasksParams,
};
use cumulus_primitives_core::{relay_chain::Hash, ParaId};
use cumulus_primitives_parachain_inherent::{
	MockValidationDataInherentDataProvider, MockXcmConfig,
};
use cumulus_relay_chain_inprocess_interface::build_inprocess_relay_chain;
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};
use cumulus_relay_chain_minimal_node::build_minimal_relay_chain_node_with_rpc;

// Substrate Imports
use sc_executor::{
	HeapAllocStrategy, NativeElseWasmExecutor, WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY,
};
use sc_network::NetworkBlock;
use sc_network_sync::SyncingService;

use sc_client_api::HeaderBackend;
use sc_service::{
	Configuration, NetworkStarter, PartialComponents, TFullBackend, TFullClient, TaskManager,
};
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker, TelemetryWorkerHandle};
use sp_api::ConstructRuntimeApi;
use sp_consensus_aura::{sr25519::AuthorityId, AuraApi};
use sp_keystore::KeystorePtr;
use sp_runtime::traits::BlakeTwo256;
use substrate_prometheus_endpoint::Registry;

use crate::rpc::{
	create_full_amplitude, create_full_foucoco, create_full_pendulum, FullDeps, ResultRpcExtension,
};
use polkadot_service::{CollatorPair, Handle};
use sc_consensus::{import_queue::ImportQueueService, ImportQueue};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;

use sc_client_api::Backend;

use futures::FutureExt;

pub use amplitude_runtime::RuntimeApi as AmplitudeRuntimeApi;
pub use foucoco_runtime::RuntimeApi as FoucocoRuntimeApi;
pub use pendulum_runtime::RuntimeApi as PendulumRuntimeApi;

pub type AmplitudeClient =
	TFullClient<Block, AmplitudeRuntimeApi, NativeElseWasmExecutor<AmplitudeRuntimeExecutor>>;
pub type FoucocoClient =
	TFullClient<Block, FoucocoRuntimeApi, NativeElseWasmExecutor<FoucocoRuntimeExecutor>>;
pub type PendulumClient =
	TFullClient<Block, PendulumRuntimeApi, NativeElseWasmExecutor<PendulumRuntimeExecutor>>;

type ParachainBlockImport<RuntimeApi, Executor> = TParachainBlockImport<
	Block,
	Arc<TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>>,
	TFullBackend<Block>,
>;

type FullPool<RuntimeApi, Executor> = sc_transaction_pool::FullPool<
	Block,
	TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>,
>;

type DefaultImportQueue = sc_consensus::DefaultImportQueue<Block>;

type OtherComponents<RuntimeApi, Executor> =
	(ParachainBlockImport<RuntimeApi, Executor>, Option<Telemetry>, Option<TelemetryWorkerHandle>);

pub trait ParachainRuntimeApiImpl:
	sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
	+ sp_api::Metadata<Block>
	+ sp_session::SessionKeys<Block>
	+ sp_api::ApiExt<Block>
	+ sp_offchain::OffchainWorkerApi<Block>
	+ sp_block_builder::BlockBuilder<Block>
	+ cumulus_primitives_core::CollectCollationInfo<Block>
	+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
	+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>
	+ AuraApi<Block, AuthorityId>
{
}

impl ParachainRuntimeApiImpl for amplitude_runtime::RuntimeApiImpl<Block, AmplitudeClient> {}
impl ParachainRuntimeApiImpl for pendulum_runtime::RuntimeApiImpl<Block, PendulumClient> {}
impl ParachainRuntimeApiImpl for foucoco_runtime::RuntimeApiImpl<Block, FoucocoClient> {}

/// Amplitude executor type.
pub struct AmplitudeRuntimeExecutor;

impl sc_executor::NativeExecutionDispatch for AmplitudeRuntimeExecutor {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		amplitude_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		amplitude_runtime::native_version()
	}
}

/// Foucoco executor type.
pub struct FoucocoRuntimeExecutor;

impl sc_executor::NativeExecutionDispatch for FoucocoRuntimeExecutor {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		foucoco_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		foucoco_runtime::native_version()
	}
}

/// Pendulum executor type.
pub struct PendulumRuntimeExecutor;

impl sc_executor::NativeExecutionDispatch for PendulumRuntimeExecutor {
	#[cfg(feature = "runtime-benchmarks")]
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type ExtendHostFunctions = ();

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		pendulum_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		pendulum_runtime::native_version()
	}
}

type ResultNewPartial<RuntimeApi, Executor> = PartialComponents<
	TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>,
	TFullBackend<Block>,
	(),
	DefaultImportQueue,
	FullPool<RuntimeApi, Executor>,
	OtherComponents<RuntimeApi, Executor>,
>;

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
#[allow(clippy::type_complexity)]
pub fn new_partial<RuntimeApi, Executor>(
	config: &Configuration,
	instant_seal: bool,
) -> Result<ResultNewPartial<RuntimeApi, Executor>, sc_service::Error>
where
	RuntimeApi: ConstructRuntimeApi<Block, TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>>
		+ Send
		+ Sync
		+ 'static,
	RuntimeApi::RuntimeApi: ParachainRuntimeApiImpl,
	Executor: sc_executor::NativeExecutionDispatch + 'static,
{
	let telemetry = config
		.telemetry_endpoints
		.clone()
		.filter(|x| !x.is_empty())
		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
			let worker = TelemetryWorker::new(16)?;
			let telemetry = worker.handle().new_telemetry(endpoints);
			Ok((worker, telemetry))
		})
		.transpose()?;

	let heap_pages = config
		.default_heap_pages
		.map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |h| HeapAllocStrategy::Static { extra_pages: h as _ });

	let wasm = WasmExecutor::builder()
		.with_execution_method(config.wasm_method)
		.with_onchain_heap_alloc_strategy(heap_pages)
		.with_offchain_heap_alloc_strategy(heap_pages)
		.with_max_runtime_instances(config.max_runtime_instances)
		.with_runtime_cache_size(config.runtime_cache_size)
		.build();

	let executor = NativeElseWasmExecutor::<Executor>::new_with_wasm_executor(wasm);

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(
			config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
		)?;
	let client = Arc::new(client);

	let telemetry_worker_handle = telemetry.as_ref().map(|(worker, _)| worker.handle());

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	let block_import = ParachainBlockImport::new(client.clone(), backend.clone());

	if instant_seal {
		let import_queue = sc_consensus_manual_seal::import_queue(
			Box::new(client.clone()),
			&task_manager.spawn_essential_handle(),
			config.prometheus_registry(),
		);

		return Ok(PartialComponents {
			backend,
			client,
			import_queue,
			keystore_container,
			task_manager,
			transaction_pool,
			select_chain: (),
			other: (block_import, telemetry, telemetry_worker_handle),
		})
	}

	let import_queue = build_import_queue(
		client.clone(),
		block_import.clone(),
		config,
		telemetry.as_ref().map(|telemetry| telemetry.handle()),
		&task_manager,
	)?;

	Ok(PartialComponents {
		backend,
		client,
		import_queue,
		keystore_container,
		task_manager,
		transaction_pool,
		select_chain: (),
		other: (block_import, telemetry, telemetry_worker_handle),
	})
}

async fn build_relay_chain_interface(
	polkadot_config: Configuration,
	parachain_config: &Configuration,
	telemetry_worker_handle: Option<TelemetryWorkerHandle>,
	task_manager: &mut TaskManager,
	collator_options: CollatorOptions,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> RelayChainResult<(Arc<(dyn RelayChainInterface + 'static)>, Option<CollatorPair>)> {
	if let RelayChainMode::ExternalRpc(rpc) = collator_options.relay_chain_mode {
		build_minimal_relay_chain_node_with_rpc(polkadot_config, task_manager, rpc).await
	} else {
		build_inprocess_relay_chain(
			polkadot_config,
			parachain_config,
			telemetry_worker_handle,
			task_manager,
			hwbench,
		)
	}
}

type FullDepsOf<RuntimeApi, Executor> = FullDeps<
	TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>,
	sc_transaction_pool::FullPool<
		Block,
		TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>,
	>,
>;

// Define and start the services shared across the standalone implementation of the node and
// the full parachain implementation.
async fn setup_common_services<RuntimeApi, Executor>(
	parachain_config: Configuration,
	params: ResultNewPartial<RuntimeApi, Executor>,
	create_full_rpc: fn(deps: FullDepsOf<RuntimeApi, Executor>) -> ResultRpcExtension,
	block_announce_validator: Option<
		RequireSecondedInBlockAnnounce<Block, Arc<dyn RelayChainInterface>>,
	>,
) -> Result<
	(
		NetworkStarter,
		Arc<SyncingService<Block>>,
		Option<Telemetry>,
		TaskManager,
		ParachainBlockImport<RuntimeApi, Executor>,
		Box<dyn ImportQueueService<Block>>,
	),
	sc_service::Error,
>
where
	RuntimeApi: ConstructRuntimeApi<Block, TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>>
		+ Send
		+ Sync
		+ 'static,
	RuntimeApi::RuntimeApi: ParachainRuntimeApiImpl,
	Executor: sc_executor::NativeExecutionDispatch + 'static,
{
	let client = params.client.clone();
	let backend = params.backend.clone();
	let mut task_manager = params.task_manager;
	let (block_import, mut telemetry, _telemetry_worker_handle) = params.other;
	let import_queue_service = params.import_queue.service();

	let net_config = sc_network::config::FullNetworkConfiguration::new(&parachain_config.network);

	let (network, system_rpc_tx, tx_handler_controller, start_network, sync_service) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &parachain_config,
			net_config,
			client: client.clone(),
			transaction_pool: params.transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue: params.import_queue,
			block_announce_validator_builder: {
				match block_announce_validator {
					Some(block_announce_validator_value) =>
						Some(Box::new(|_| Box::new(block_announce_validator_value))),
					None => None,
				}
			},
			warp_sync_params: None,
		})?;

	let rpc_builder = {
		let transaction_pool = params.transaction_pool.clone();
		let client = Arc::clone(&client);
		Box::new(move |deny_unsafe, _| {
			let deps =
				FullDeps { client: client.clone(), pool: transaction_pool.clone(), deny_unsafe };
			create_full_rpc(deps).map_err(Into::into)
		})
	};

	if parachain_config.offchain_worker.enabled {
		task_manager.spawn_handle().spawn(
			"offchain-workers-runner",
			"offchain-worker",
			sc_offchain::OffchainWorkers::new(sc_offchain::OffchainWorkerOptions {
				runtime_api_provider: client.clone(),
				is_validator: parachain_config.role.is_authority(),
				keystore: Some(params.keystore_container.keystore()),
				offchain_db: backend.offchain_storage(),
				transaction_pool: Some(OffchainTransactionPoolFactory::new(
					params.transaction_pool.clone(),
				)),
				network_provider: network.clone(),
				enable_http_requests: true,
				custom_extensions: |_| vec![],
			})
			.run(client.clone(), task_manager.spawn_handle())
			.boxed(),
		);
	}

	sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		rpc_builder,
		client: client.clone(),
		transaction_pool: params.transaction_pool.clone(),
		task_manager: &mut task_manager,
		config: parachain_config,
		keystore: params.keystore_container.keystore(),
		backend: backend.clone(),
		network: network.clone(),
		system_rpc_tx,
		tx_handler_controller,
		sync_service: sync_service.clone(),
		telemetry: telemetry.as_mut(),
	})?;

	Ok((start_network, sync_service, telemetry, task_manager, block_import, import_queue_service))
}

#[sc_tracing::logging::prefix_logs_with("Parachain")]
async fn start_node_impl<RuntimeApi, Executor>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
	create_full_rpc: fn(deps: FullDepsOf<RuntimeApi, Executor>) -> ResultRpcExtension,
) -> sc_service::error::Result<(
	TaskManager,
	Arc<TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>>,
)>
where
	RuntimeApi: ConstructRuntimeApi<Block, TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>>
		+ Send
		+ Sync
		+ 'static,
	RuntimeApi::RuntimeApi: ParachainRuntimeApiImpl,
	sc_client_api::StateBackendFor<TFullBackend<Block>, Block>: sp_api::StateBackend<BlakeTwo256>,
	Executor: sc_executor::NativeExecutionDispatch + 'static,
{
	let is_standalone = false;
	let mut parachain_config = prepare_node_config(parachain_config);
	let mut params = new_partial(&mut parachain_config, is_standalone)?;

	let client = params.client.clone();

	//just clone the last element of the "other" tuple
	let telemetry_worker_handle_clone = params.other.2.clone();

	let (relay_chain_interface, collator_key) = build_relay_chain_interface(
		polkadot_config,
		&parachain_config,
		telemetry_worker_handle_clone,
		&mut params.task_manager,
		collator_options.clone(),
		hwbench.clone(),
	)
	.await
	.map_err(|e| sc_service::Error::Application(Box::new(e)))?;
	let block_announce_validator =
		RequireSecondedInBlockAnnounce::new(relay_chain_interface.clone(), id);

	let validator = parachain_config.role.is_authority();
	let prometheus_registry = parachain_config.prometheus_registry().cloned();
	let transaction_pool = params.transaction_pool.clone();
	let keystore_ptr = params.keystore_container.keystore().clone();

	let (
		start_network,
		sync_service,
		mut telemetry,
		mut task_manager,
		block_import,
		import_queue_service,
	) = setup_common_services(
		parachain_config,
		params,
		create_full_rpc,
		Some(block_announce_validator),
	)
	.await?;

	if let Some(hwbench) = hwbench {
		sc_sysinfo::print_hwbench(&hwbench);

		if let Some(ref mut telemetry) = telemetry {
			let telemetry_handle = telemetry.handle();
			task_manager.spawn_handle().spawn(
				"telemetry_hwbench",
				None,
				sc_sysinfo::initialize_hwbench_telemetry(telemetry_handle, hwbench),
			);
		}
	}

	let relay_chain_slot_duration = Duration::from_secs(6);

	let overseer_handle = relay_chain_interface
		.overseer_handle()
		.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

	let announce_block = {
		let sync_service = sync_service.clone();
		Arc::new(move |hash, data| sync_service.announce_block(hash, data))
	};

	start_relay_chain_tasks(StartRelayChainTasksParams {
		client: client.clone(),
		announce_block: announce_block.clone(),
		para_id: id,
		relay_chain_interface: relay_chain_interface.clone(),
		task_manager: &mut task_manager,
		da_recovery_profile: if validator {
			DARecoveryProfile::Collator
		} else {
			DARecoveryProfile::FullNode
		},
		import_queue: import_queue_service,
		relay_chain_slot_duration,
		recovery_handle: Box::new(overseer_handle.clone()),
		sync_service: sync_service.clone(),
	})?;

	if validator {
		start_consensus(
			client.clone(),
			block_import,
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|t| t.handle()),
			&task_manager,
			relay_chain_interface.clone(),
			transaction_pool,
			sync_service.clone(),
			keystore_ptr,
			overseer_handle.clone(),
			id,
			relay_chain_slot_duration,
			collator_key.clone().expect("Command line arguments do not allow this. qed"),
			announce_block.clone(),
		)?;
	}

	start_network.start_network();

	Ok((task_manager, client))
}

/// Build the import queue for the parachain runtime.
fn build_import_queue<RuntimeApi, Executor>(
	client: Arc<TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>>,
	block_import: ParachainBlockImport<RuntimeApi, Executor>,
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<DefaultImportQueue, sc_service::Error>
where
	RuntimeApi: ConstructRuntimeApi<Block, TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>>
		+ Send
		+ Sync
		+ 'static,
	RuntimeApi::RuntimeApi: ParachainRuntimeApiImpl,
	Executor: sc_executor::NativeExecutionDispatch + 'static,
{
	let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

	cumulus_client_consensus_aura::import_queue::<
		sp_consensus_aura::sr25519::AuthorityPair,
		_,
		_,
		_,
		_,
		_,
	>(cumulus_client_consensus_aura::ImportQueueParams {
		block_import,
		client,
		create_inherent_data_providers: move |_, _| async move {
			let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

			let slot =
				sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
					*timestamp,
					slot_duration,
				);

			Ok((slot, timestamp))
		},
		registry: config.prometheus_registry(),
		spawner: &task_manager.spawn_essential_handle(),
		telemetry,
	})
	.map_err(Into::into)
}

#[sc_tracing::logging::prefix_logs_with("Parachain")]
async fn start_standalone_node_impl<RuntimeApi, Executor>(
	parachain_config: Configuration,
	create_full_rpc: fn(deps: FullDepsOf<RuntimeApi, Executor>) -> ResultRpcExtension,
) -> sc_service::error::Result<(
	TaskManager,
	Arc<TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>>,
)>
where
	RuntimeApi: ConstructRuntimeApi<Block, TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>>
		+ Send
		+ Sync
		+ 'static,
	RuntimeApi::RuntimeApi: ParachainRuntimeApiImpl,
	sc_client_api::StateBackendFor<TFullBackend<Block>, Block>: sp_api::StateBackend<BlakeTwo256>,
	Executor: sc_executor::NativeExecutionDispatch + 'static,
{
	let parachain_config = prepare_node_config(parachain_config);

	let is_standalone = true;
	let params = new_partial(&parachain_config, is_standalone)?;

	let client = params.client.clone();
	let backend = params.backend.clone();

	let prometheus_registry = parachain_config.prometheus_registry().cloned();
	let transaction_pool = params.transaction_pool.clone();

	let (
		start_network,
		_sync_service,
		telemetry,
		task_manager,
		_block_import,
		_import_queue_service,
	) = setup_common_services(parachain_config, params, create_full_rpc, None).await?;

	let proposer_factory = sc_basic_authorship::ProposerFactory::new(
		task_manager.spawn_handle(),
		client.clone(),
		transaction_pool.clone(),
		prometheus_registry.as_ref(),
		telemetry.as_ref().map(|x| x.handle()),
	);

	let select_chain = sc_consensus::LongestChain::new(backend.clone());
	let client_clone_move = client.clone();

	let instant_seal_params = sc_consensus_manual_seal::InstantSealParams {
		block_import: client.clone(),
		env: proposer_factory,
		client: client.clone(),
		pool: transaction_pool,
		select_chain,
		consensus_data_provider: None,
		create_inherent_data_providers: move |block, _| {
			let current_para_block = (*client_clone_move)
				.number(block)
				.expect("Header lookup should succeed")
				.expect("Header passed in as parent should be present in backend.");
			let client_for_xcm = client_clone_move.clone();
			async move {
				let mocked_parachain = MockValidationDataInherentDataProvider {
					current_para_block,
					relay_offset: 1000,
					relay_blocks_per_para_block: 2,
					para_blocks_per_relay_epoch: 0,
					relay_randomness_config: (),
					xcm_config: MockXcmConfig::new(
						&*client_for_xcm,
						block,
						Default::default(),
						Default::default(),
					),
					raw_downward_messages: vec![],
					raw_horizontal_messages: vec![],
				};
				Ok((sp_timestamp::InherentDataProvider::from_system_time(), mocked_parachain))
			}
		},
	};

	let authorship_future = sc_consensus_manual_seal::run_instant_seal(instant_seal_params);

	task_manager
		.spawn_essential_handle()
		.spawn_blocking("instant-seal", None, authorship_future);

	start_network.start_network();

	Ok((task_manager, client))
}

#[allow(clippy::too_many_arguments)]
fn start_consensus<RuntimeApi, Executor>(
	client: Arc<TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>>,
	block_import: ParachainBlockImport<RuntimeApi, Executor>,
	prometheus_registry: Option<&Registry>,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	transaction_pool: Arc<FullPool<RuntimeApi, Executor>>,
	sync_oracle: Arc<SyncingService<Block>>,
	keystore: KeystorePtr,
	overseer_handle: Handle,
	id: ParaId,
	relay_chain_slot_duration: Duration,
	collator_key: CollatorPair,
	announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
) -> Result<(), sc_service::Error>
where
	RuntimeApi: ConstructRuntimeApi<Block, TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<Executor>>>
		+ Send
		+ Sync
		+ 'static,
	RuntimeApi::RuntimeApi: ParachainRuntimeApiImpl,
	sc_client_api::StateBackendFor<TFullBackend<Block>, Block>: sp_api::StateBackend<BlakeTwo256>,
	Executor: sc_executor::NativeExecutionDispatch + 'static,
{
	let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

	let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
		task_manager.spawn_handle(),
		client.clone(),
		transaction_pool,
		prometheus_registry,
		telemetry.clone(),
	);
	let proposer = Proposer::new(proposer_factory);

	let collator_service = CollatorService::new(
		client.clone(),
		Arc::new(task_manager.spawn_handle()),
		announce_block.clone(),
		client.clone(),
	);

	let params = BasicAuraParams {
		proposer,
		create_inherent_data_providers: move |_, ()| async move { Ok(()) },
		block_import,
		collator_key,
		collator_service,
		para_client: client,
		relay_client: relay_chain_interface,
		sync_oracle,
		keystore,
		slot_duration,
		authoring_duration: Duration::from_millis(500),
		relay_chain_slot_duration,
		para_id: id,
		overseer_handle,
	};

	let fut =
		basic_aura::run::<Block, sp_consensus_aura::sr25519::AuthorityPair, _, _, _, _, _, _, _>(
			params,
		);

	task_manager.spawn_essential_handle().spawn("aura", None, fut);
	Ok(())
}

/// Start a parachain node.
pub async fn start_parachain_node_pendulum(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<PendulumClient>)> {
	start_node_impl(
		parachain_config,
		polkadot_config,
		collator_options,
		id,
		hwbench,
		create_full_pendulum,
	)
	.await
}

/// Start a parachain node with the Spacewalk RPC exposed using the foucoco runtime definitions.
pub async fn start_parachain_node_spacewalk_foucoco(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<FoucocoClient>)> {
	start_node_impl(
		parachain_config,
		polkadot_config,
		collator_options,
		id,
		hwbench,
		create_full_foucoco,
	)
	.await
}

pub async fn start_parachain_node_spacewalk_foucoco_standalone(
	parachain_config: Configuration,
) -> sc_service::error::Result<(TaskManager, Arc<FoucocoClient>)> {
	start_standalone_node_impl(parachain_config, create_full_foucoco).await
}

/// Start a parachain node with the Spacewalk RPC exposed using the amplitude runtime definitions.
pub async fn start_parachain_node_spacewalk_amplitude(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<AmplitudeClient>)> {
	start_node_impl(
		parachain_config,
		polkadot_config,
		collator_options,
		id,
		hwbench,
		create_full_amplitude,
	)
	.await
}
