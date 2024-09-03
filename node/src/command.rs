use codec::Encode;
use cumulus_client_cli::generate_genesis_block;
use cumulus_primitives_core::ParaId;
use frame_benchmarking_cli::{BenchmarkCmd, SUBSTRATE_REFERENCE_HARDWARE};
use log::info;
use runtime_common::opaque::Block;
use sc_chain_spec::GenericChainSpec;
use sc_cli::{
	ChainSpec, CliConfiguration, DefaultConfigurationValues, ImportParams, KeystoreParams,
	NetworkParams, Result, RuntimeVersion, SharedParams, SubstrateCli,
};
use sc_service::{
	config::{BasePath, PrometheusConfig},
	Configuration,
};
use sp_core::hexdisplay::HexDisplay;
use sp_runtime::traits::{AccountIdConversion, Block as BlockT};

use sc_executor::NativeExecutionDispatch;

use crate::{
	chain_spec::{self, ParachainExtensions},
	cli::{Cli, RelayChainCli, Subcommand},
	service::{
		new_partial, AmplitudeRuntimeExecutor, FoucocoRuntimeExecutor, PendulumRuntimeExecutor,
	},
};

#[derive(PartialEq, Eq)]
enum ChainIdentity {
	Amplitude,
	Foucoco,
	Pendulum,
	FoucocoStandalone,
}

impl ChainIdentity {
	fn identify(id: &str) -> Option<Self> {
		match id {
			"amplitude" => Some(ChainIdentity::Amplitude),
			"foucoco" => Some(ChainIdentity::Foucoco),
			"pendulum" => Some(ChainIdentity::Pendulum),
			"foucoco-standalone" => Some(ChainIdentity::FoucocoStandalone),
			_ => None,
		}
	}

	fn from_json_file(path: &str) -> std::result::Result<Self, String> {
		GenericChainSpec::<(), ParachainExtensions>::from_json_file(path.into())
			.map(|chain_spec| chain_spec.identify())
			.or_else(|_| {
				GenericChainSpec::<()>::from_json_file(path.into())
					.map(|chain_spec| chain_spec.identify())
			})
	}

	fn get_runtime_version(&self) -> &'static RuntimeVersion {
		match self {
			ChainIdentity::Amplitude => &amplitude_runtime::VERSION,
			ChainIdentity::Foucoco => &foucoco_runtime::VERSION,
			ChainIdentity::FoucocoStandalone => &foucoco_runtime::VERSION,
			ChainIdentity::Pendulum => &pendulum_runtime::VERSION,
		}
	}

	fn create_chain_spec(&self) -> Box<dyn ChainSpec> {
		match self {
			ChainIdentity::Amplitude => Box::new(chain_spec::amplitude_config()),
			ChainIdentity::Foucoco => Box::new(chain_spec::foucoco_config()),
			ChainIdentity::Pendulum => Box::new(chain_spec::pendulum_config()),
			ChainIdentity::FoucocoStandalone => Box::new(chain_spec::foucoco_standalone_config()),
		}
	}

	fn load_chain_spec_from_json_file(
		&self,
		path: &str,
	) -> std::result::Result<Box<dyn ChainSpec>, String> {
		Ok(match self {
			ChainIdentity::Amplitude =>
				Box::new(chain_spec::AmplitudeChainSpec::from_json_file(path.into())?),
			ChainIdentity::Foucoco =>
				Box::new(chain_spec::FoucocoChainSpec::from_json_file(path.into())?),
			ChainIdentity::FoucocoStandalone =>
				Box::new(chain_spec::FoucocoChainSpec::from_json_file(path.into())?),
			ChainIdentity::Pendulum =>
				Box::new(chain_spec::PendulumChainSpec::from_json_file(path.into())?),
		})
	}
}

trait IdentifyChain {
	fn identify(&self) -> ChainIdentity;
}

impl IdentifyChain for dyn sc_service::ChainSpec {
	fn identify(&self) -> ChainIdentity {
		ChainIdentity::identify(self.id()).unwrap_or(ChainIdentity::Foucoco)
	}
}

impl<T: sc_service::ChainSpec + 'static> IdentifyChain for T {
	fn identify(&self) -> ChainIdentity {
		<dyn sc_service::ChainSpec>::identify(self)
	}
}

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Pendulum Collator".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"Pendulum Collator\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relay chain node.\n\n\
		{} <parachain-args> -- <relay-chain-args>",
			Self::executable_name()
		)
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/pendulum-chain/pendulum/issues/new".into()
	}

	fn copyright_start_year() -> i32 {
		2022
	}

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
		Ok(match ChainIdentity::identify(id) {
			Some(identitiy) => identitiy.create_chain_spec(),
			None => ChainIdentity::from_json_file(id)?.load_chain_spec_from_json_file(id)?,
		})
	}
}

impl SubstrateCli for RelayChainCli {
	fn impl_name() -> String {
		"Pendulum Collator".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		format!(
			"Pendulum Collator\n\nThe command-line arguments provided first will be \
		passed to the parachain node, while the arguments provided after -- will be passed \
		to the relay chain node.\n\n\
		{} <parachain-args> -- <relay-chain-args>",
			Self::executable_name()
		)
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/pendulum-chain/pendulum/issues/new".into()
	}

	fn copyright_start_year() -> i32 {
		2022
	}

	fn load_spec(&self, id: &str) -> std::result::Result<Box<dyn sc_service::ChainSpec>, String> {
		polkadot_cli::Cli::from_iter([RelayChainCli::executable_name()].iter()).load_spec(id)
	}
}

macro_rules! construct_sync_run {
	(|$components:ident, $cli:ident, $cmd:ident, $config:ident| $code:expr ) => {{
		let runner = $cli.create_runner($cmd)?;

		// none of these are using instant seal
		let is_standalone = false;
		match runner.config().chain_spec.identify() {
			ChainIdentity::Amplitude => runner.sync_run(|$config| {
				let $components = new_partial::<
					amplitude_runtime::RuntimeApi,
					AmplitudeRuntimeExecutor,
				>(&$config, is_standalone)?;
				$code
			}),
			ChainIdentity::Foucoco => runner.sync_run(|$config| {
				let $components = new_partial::<foucoco_runtime::RuntimeApi, FoucocoRuntimeExecutor>(
					&$config,
					is_standalone,
				)?;
				$code
			}),
			ChainIdentity::Pendulum => runner.sync_run(|$config| {
				let $components = new_partial::<
					pendulum_runtime::RuntimeApi,
					PendulumRuntimeExecutor,
				>(&$config, is_standalone)?;
				$code
			}),
			// Foucoco standalone is only supported
			// with the instant seal flag.
			ChainIdentity::FoucocoStandalone => unimplemented!(),
		}
	}};
}

macro_rules! construct_generic_async_run {
	(|$components:ident, $cli:ident, $cmd:ident, $config:ident| $code:expr ) => {{
		let runner = $cli.create_runner($cmd)?;

		// none of these are using instant seal
		let is_standalone = false;
		match runner.config().chain_spec.identify() {
			ChainIdentity::Amplitude => runner.async_run(|$config| {
				let $components = new_partial::<
					amplitude_runtime::RuntimeApi,
					AmplitudeRuntimeExecutor,
				>(&$config, is_standalone)?;
				$code
			}),
			ChainIdentity::Foucoco => runner.async_run(|$config| {
				let $components = new_partial::<foucoco_runtime::RuntimeApi, FoucocoRuntimeExecutor>(
					&$config,
					is_standalone,
				)?;
				$code
			}),
			ChainIdentity::Pendulum => runner.async_run(|$config| {
				let $components = new_partial::<
					pendulum_runtime::RuntimeApi,
					PendulumRuntimeExecutor,
				>(&$config, is_standalone)?;
				$code
			}),
			// Foucoco standalone is only supported
			// with the instant seal flag.
			ChainIdentity::FoucocoStandalone => unimplemented!(),
		}
	}};
}

macro_rules! construct_async_run {
	(|$components:ident, $cli:ident, $cmd:ident, $config:ident| $( $code:tt )* ) => {{
		construct_generic_async_run!(|$components, $cli, $cmd, $config| {
			let task_manager = $components.task_manager;
					{ $( $code )* }.map(|v| (v, task_manager))
		})
	}}
}

/// Parse command line arguments into service configuration.
pub fn run() -> Result<()> {
	let cli = Cli::from_args();

	match &cli.subcommand {
		Some(Subcommand::BuildSpec(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Some(Subcommand::CheckBlock(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, components.import_queue))
			})
		},
		Some(Subcommand::ExportBlocks(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, config.database))
			})
		},
		Some(Subcommand::ExportState(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, config.chain_spec))
			})
		},
		Some(Subcommand::ImportBlocks(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, components.import_queue))
			})
		},
		Some(Subcommand::Revert(cmd)) => {
			construct_async_run!(|components, cli, cmd, config| {
				Ok(cmd.run(components.client, components.backend, None))
			})
		},
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;

			runner.sync_run(|config| {
				let polkadot_cli = RelayChainCli::new(
					&config,
					[RelayChainCli::executable_name()].iter().chain(cli.relay_chain_args.iter()),
				);

				let polkadot_config = SubstrateCli::create_configuration(
					&polkadot_cli,
					&polkadot_cli,
					config.tokio_handle.clone(),
				)
				.map_err(|err| format!("Relay chain argument error: {err}"))?;

				cmd.run(config, polkadot_config)
			})
		},
		Some(Subcommand::ExportGenesisState(cmd)) =>
			construct_async_run!(|components, cli, cmd, config| {
				Ok(async move { cmd.run(&*config.chain_spec, &*components.client) })
			}),
		Some(Subcommand::ExportGenesisWasm(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|_config| {
				let spec = cli.load_spec(&cmd.shared_params.chain.clone().unwrap_or_default())?;
				cmd.run(&*spec)
			})
		},
		Some(Subcommand::Benchmark(bench_cmd)) => match bench_cmd {
			BenchmarkCmd::Pallet(cmd) =>
				if cfg!(feature = "runtime-benchmarks") {
					let runner = cli.create_runner(cmd)?;

					match runner.config().chain_spec.identify() {
						ChainIdentity::Amplitude => runner.sync_run(|config| {
							cmd.run::<Block, <AmplitudeRuntimeExecutor as NativeExecutionDispatch>::ExtendHostFunctions>(config)
						}),
						ChainIdentity::Foucoco =>
							runner.sync_run(|config| {
								cmd.run::<Block, <FoucocoRuntimeExecutor as NativeExecutionDispatch>::ExtendHostFunctions>(config)
							}),
						ChainIdentity::Pendulum => runner.sync_run(|config| {
							cmd.run::<Block, <PendulumRuntimeExecutor as NativeExecutionDispatch>::ExtendHostFunctions>(config)
						}),
						ChainIdentity::FoucocoStandalone => unimplemented!(),
					}
				} else {
					Err("Benchmarking wasn't enabled when building the node. \
				You can enable it with `--features runtime-benchmarks`."
						.into())
				},
			BenchmarkCmd::Block(cmd) => {
				construct_sync_run!(|components, cli, cmd, config| cmd.run(components.client))
			},
			#[cfg(not(feature = "runtime-benchmarks"))]
			BenchmarkCmd::Storage(_) => Err(sc_cli::Error::Input(
				"Compile with --features=runtime-benchmarks \
						to enable storage benchmarks."
					.into(),
			)),
			#[cfg(feature = "runtime-benchmarks")]
			BenchmarkCmd::Storage(cmd) => {
				construct_sync_run!(|components, cli, cmd, config| {
					let db = components.backend.expose_db();
					let storage = components.backend.expose_storage();

					cmd.run(config, components.client.clone(), db, storage)
				})
			},
			BenchmarkCmd::Machine(cmd) => {
				let runner = cli.create_runner(cmd)?;
				runner.sync_run(|config| cmd.run(&config, SUBSTRATE_REFERENCE_HARDWARE.clone()))
			},
			// NOTE: this allows the Client to leniently implement
			// new benchmark commands without requiring a companion MR.
			#[allow(unreachable_patterns)]
			_ => Err("Benchmarking sub-command unsupported".into()),
		},
		#[cfg(feature = "try-runtime")]
		Some(Subcommand::TryRuntime(_cmd)) => Err("The `try-runtime` subcommand has been migrated to a \
			standalone CLI (https://github.com/paritytech/try-runtime-cli). It is no longer \
			being maintained here and will be removed entirely some time after January 2024. \
			Please remove this subcommand from your runtime and use the standalone CLI."
			.into()),

		None => {
			let runner = cli.create_runner(&cli.run.normalize())?;
			runner
				.run_node_until_exit(|config| async move {
					if cli.instant_seal {
						start_instant(config).await
					} else {
						start_node(cli, config).await
					}
				})
				.map_err(Into::into)
		},
	}
}

async fn start_instant(
	config: Configuration,
) -> sc_service::error::Result<sc_service::TaskManager> {
	crate::service::start_parachain_node_spacewalk_foucoco_standalone(config)
		.await
		.map(|r| r.0)
		.map_err(Into::into)
}

async fn start_node(
	cli: Cli,
	config: Configuration,
) -> sc_service::error::Result<sc_service::TaskManager> {
	let collator_options = cli.run.collator_options();

	let hwbench = if !cli.no_hardware_benchmarks {
		config.database.path().map(|database_path| {
			let _ = std::fs::create_dir_all(database_path);
			sc_sysinfo::gather_hwbench(Some(database_path))
		})
	} else {
		None
	};

	let para_id = chain_spec::ParachainExtensions::try_get(&*config.chain_spec)
		.map(|e| e.para_id)
		.ok_or("Could not find parachain ID in chain-spec.")?;

	let polkadot_cli = RelayChainCli::new(
		&config,
		[RelayChainCli::executable_name()].iter().chain(cli.relay_chain_args.iter()),
	);

	let id = ParaId::from(para_id);

	let parachain_account =
		AccountIdConversion::<polkadot_primitives::v5::AccountId>::into_account_truncating(&id);

	let state_version = config.chain_spec.identify().get_runtime_version().state_version();
	let block: Block =
		generate_genesis_block(&*config.chain_spec, state_version).map_err(|e| format!("{e:?}"))?;
	let genesis_state = format!("0x{:?}", HexDisplay::from(&block.header().encode()));

	let tokio_handle = config.tokio_handle.clone();
	let polkadot_config =
		SubstrateCli::create_configuration(&polkadot_cli, &polkadot_cli, tokio_handle)
			.map_err(|err| format!("Relay chain argument error: {err}"))?;

	info!("Parachain id: {:?}", id);
	info!("Parachain Account: {}", parachain_account);
	info!("Parachain genesis state: {}", genesis_state);
	info!("Is collating: {}", if config.role.is_authority() { "yes" } else { "no" });

	match config.chain_spec.identify() {
		ChainIdentity::Amplitude => {
			sp_core::crypto::set_default_ss58_version(amplitude_runtime::SS58Prefix::get().into());
			crate::service::start_parachain_node_spacewalk_amplitude(
				config,
				polkadot_config,
				collator_options,
				id,
				hwbench,
			)
			.await
			.map(|r| r.0)
			.map_err(Into::into)
		},
		ChainIdentity::Foucoco => {
			sp_core::crypto::set_default_ss58_version(foucoco_runtime::SS58Prefix::get().into());
			crate::service::start_parachain_node_spacewalk_foucoco(
				config,
				polkadot_config,
				collator_options,
				id,
				hwbench,
			)
			.await
			.map(|r| r.0)
			.map_err(Into::into)
		},
		ChainIdentity::Pendulum => {
			sp_core::crypto::set_default_ss58_version(pendulum_runtime::SS58Prefix::get().into());
			crate::service::start_parachain_node_pendulum(
				config,
				polkadot_config,
				collator_options,
				id,
				hwbench,
			)
			.await
			.map(|r| r.0)
			.map_err(Into::into)
		},
		ChainIdentity::FoucocoStandalone => {
			// Throw error. Foucoco standalone is only supported
			// with the instant seal flag.
			Err("Foucoco standalone is only supported with the instant seal flag".into())
		},
	}
}

impl DefaultConfigurationValues for RelayChainCli {
	fn p2p_listen_port() -> u16 {
		30334
	}

	fn prometheus_listen_port() -> u16 {
		9616
	}
}

impl CliConfiguration<Self> for RelayChainCli {
	fn shared_params(&self) -> &SharedParams {
		self.base.base.shared_params()
	}

	fn import_params(&self) -> Option<&ImportParams> {
		self.base.base.import_params()
	}

	fn network_params(&self) -> Option<&NetworkParams> {
		self.base.base.network_params()
	}

	fn keystore_params(&self) -> Option<&KeystoreParams> {
		self.base.base.keystore_params()
	}

	fn base_path(&self) -> Result<Option<BasePath>> {
		Ok(self
			.shared_params()
			.base_path()?
			.or_else(|| self.base_path.clone().map(Into::into)))
	}

	fn prometheus_config(
		&self,
		default_listen_port: u16,
		chain_spec: &Box<dyn ChainSpec>,
	) -> Result<Option<PrometheusConfig>> {
		self.base.base.prometheus_config(default_listen_port, chain_spec)
	}

	fn init<F>(
		&self,
		_support_url: &String,
		_impl_version: &String,
		_logger_hook: F,
		_config: &sc_service::Configuration,
	) -> Result<()>
	where
		F: FnOnce(&mut sc_cli::LoggerBuilder, &sc_service::Configuration),
	{
		unreachable!("PolkadotCli is never initialized; qed");
	}

	fn chain_id(&self, is_dev: bool) -> Result<String> {
		let chain_id = self.base.base.chain_id(is_dev)?;

		Ok(if chain_id.is_empty() { self.chain_id.clone().unwrap_or_default() } else { chain_id })
	}

	fn role(&self, is_dev: bool) -> Result<sc_service::Role> {
		self.base.base.role(is_dev)
	}

	fn transaction_pool(&self, is_dev: bool) -> Result<sc_service::config::TransactionPoolOptions> {
		self.base.base.transaction_pool(is_dev)
	}

	fn trie_cache_maximum_size(&self) -> Result<Option<usize>> {
		self.base.base.trie_cache_maximum_size()
	}

	fn rpc_methods(&self) -> Result<sc_service::config::RpcMethods> {
		self.base.base.rpc_methods()
	}

	fn rpc_cors(&self, is_dev: bool) -> Result<Option<Vec<String>>> {
		self.base.base.rpc_cors(is_dev)
	}

	fn default_heap_pages(&self) -> Result<Option<u64>> {
		self.base.base.default_heap_pages()
	}

	fn force_authoring(&self) -> Result<bool> {
		self.base.base.force_authoring()
	}

	fn disable_grandpa(&self) -> Result<bool> {
		self.base.base.disable_grandpa()
	}

	fn max_runtime_instances(&self) -> Result<Option<usize>> {
		self.base.base.max_runtime_instances()
	}

	fn announce_block(&self) -> Result<bool> {
		self.base.base.announce_block()
	}

	fn telemetry_endpoints(
		&self,
		chain_spec: &Box<dyn ChainSpec>,
	) -> Result<Option<sc_telemetry::TelemetryEndpoints>> {
		self.base.base.telemetry_endpoints(chain_spec)
	}

	fn node_name(&self) -> Result<String> {
		self.base.base.node_name()
	}
}
