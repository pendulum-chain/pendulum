#![allow(clippy::default_constructed_unit_structs)]

use crate::constants::{amplitude, foucoco, pendulum};
use core::default::Default;
use cumulus_primitives_core::ParaId;
use runtime_common::{AccountId, AuraId, Balance, Signature, UNIT};
use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup};
use sc_service::ChainType;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sp_core::{
	crypto::{Ss58Codec, UncheckedInto},
	sp_std, sr25519, Pair, Public,
};
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	FixedPointNumber, FixedU128, Perquintill,
};

use spacewalk_primitives::{oracle::Key, Asset, CurrencyId, CurrencyId::XCM, VaultCurrencyPair};

const MAINNET_USDC_CURRENCY_ID: CurrencyId = pendulum_runtime::GetWrappedCurrencyId::get();
const TESTNET_USDC_CURRENCY_ID: CurrencyId = amplitude_runtime::GetWrappedCurrencyId::get();

const MAX_SAFE_INTEGER_JSON: u128 = 1 << 53;

/// Specialized `ChainSpec` for the normal parachain runtime.
pub type AmplitudeChainSpec =
	sc_service::GenericChainSpec<amplitude_runtime::RuntimeGenesisConfig, ParachainExtensions>;

pub type FoucocoChainSpec =
	sc_service::GenericChainSpec<foucoco_runtime::RuntimeGenesisConfig, ParachainExtensions>;

pub type PendulumChainSpec =
	sc_service::GenericChainSpec<pendulum_runtime::RuntimeGenesisConfig, ParachainExtensions>;

/// The default XCM version to set in genesis config.
const SAFE_XCM_VERSION: u32 = staging_xcm::prelude::XCM_VERSION;

pub fn create_pendulum_multisig_account(id: &str) -> AccountId {
	let mut signatories: Vec<_> = pendulum::SUDO_SIGNATORIES
		.iter()
		.chain([id].iter())
		.map(|ss58| AccountId::from_ss58check(ss58).unwrap())
		.collect();
	signatories.sort();

	pallet_multisig::Pallet::<pendulum_runtime::Runtime>::multi_account_id(&signatories[..], 4)
}

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	<TPublic::Pair as Pair>::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

type AccountPublic = <Signature as Verify>::Signer;

/// Generate an account ID from seed.
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// The extensions for the [`ChainSpec`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension)]
#[serde(deny_unknown_fields)]
pub struct ParachainExtensions {
	/// The relay chain of the Parachain.
	pub relay_chain: String,
	/// The id of the Parachain.
	pub para_id: u32,
}

impl ParachainExtensions {
	/// Try to get the extension from the given `ChainSpec`.
	pub fn try_get(chain_spec: &dyn sc_service::ChainSpec) -> Option<&Self> {
		sc_chain_spec::get_extension(chain_spec.extensions())
	}
}

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn get_amplitude_session_keys(keys: AuraId) -> amplitude_runtime::SessionKeys {
	amplitude_runtime::SessionKeys { aura: keys }
}

pub fn get_foucoco_session_keys(keys: AuraId) -> foucoco_runtime::SessionKeys {
	foucoco_runtime::SessionKeys { aura: keys }
}

pub fn get_pendulum_session_keys(keys: AuraId) -> pendulum_runtime::SessionKeys {
	pendulum_runtime::SessionKeys { aura: keys }
}

fn get_amplitude_properties() -> Map<String, Value> {
	let mut properties = Map::new();
	properties.insert("tokenSymbol".into(), "AMPE".into());
	properties.insert("tokenDecimals".into(), amplitude::TOKEN_DECIMALS.into());
	properties.insert("ss58Format".into(), amplitude_runtime::SS58Prefix::get().into());
	properties
}

pub fn amplitude_config() -> AmplitudeChainSpec {
	sp_core::crypto::set_default_ss58_version(amplitude_runtime::SS58Prefix::get().into());

	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "AMPE".into());
	properties.insert("tokenDecimals".into(), amplitude::TOKEN_DECIMALS.into());
	properties.insert("ss58Format".into(), amplitude_runtime::SS58Prefix::get().into());

	let mut signatories: Vec<_> = amplitude::INITIAL_SUDO_SIGNATORIES
		.iter()
		.map(|ss58| AccountId::from_ss58check(ss58).unwrap())
		.collect();
	signatories.sort();

	let invulnerables: Vec<_> = amplitude::INITIAL_COLLATORS
		.iter()
		.map(|ss58| AccountId::from_ss58check(ss58).unwrap())
		.collect();

	let sudo_account = pallet_multisig::Pallet::<amplitude_runtime::Runtime>::multi_account_id(
		&signatories[..],
		3,
	);

	AmplitudeChainSpec::builder(
		amplitude_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		ParachainExtensions {
			relay_chain: "kusama".into(), // You MUST set this to the correct network!
			para_id: amplitude::PARACHAIN_ID,
		},
	)
	.with_name("Amplitude")
	.with_id("amplitude")
	.with_chain_type(ChainType::Live)
	.with_properties(get_amplitude_properties())
	.with_genesis_config_patch(amplitude_genesis(
		// initial collators.
		invulnerables.clone(),
		signatories.clone(),
		vec![sudo_account.clone()],
		sudo_account.clone(),
		amplitude::PARACHAIN_ID.into(),
		false,
	))
	.build()
}

pub fn foucoco_config() -> FoucocoChainSpec {
	sp_core::crypto::set_default_ss58_version(foucoco_runtime::SS58Prefix::get().into());

	let mut signatories: Vec<_> = foucoco::INITIAL_SUDO_SIGNATORIES
		.iter()
		.map(|ss58| AccountId::from_ss58check(ss58).unwrap())
		.collect();
	signatories.sort();

	let invulnerables: Vec<_> = foucoco::INITIAL_COLLATORS
		.iter()
		.map(|ss58| AccountId::from_ss58check(ss58).unwrap())
		.collect();

	let sudo_account =
		pallet_multisig::Pallet::<foucoco_runtime::Runtime>::multi_account_id(&signatories[..], 3);

	let offchain_worker_price_feeder =
		AccountId::from_ss58check(foucoco::OFF_CHAIN_WORKER_ADDRESS).unwrap();

	FoucocoChainSpec::builder(
		foucoco_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		ParachainExtensions {
			relay_chain: "kusama".into(), // You MUST set this to the correct network!
			para_id: foucoco::PARACHAIN_ID,
		},
	)
	.with_name("Foucoco")
	.with_id("foucoco")
	.with_chain_type(ChainType::Live)
	.with_properties(get_amplitude_properties()) // Same properties as Amplitude chain.
	.with_genesis_config_patch(foucoco_genesis(
		// initial collators.
		invulnerables.clone(),
		signatories.clone(),
		vec![sudo_account.clone(), offchain_worker_price_feeder.clone()],
		sudo_account.clone(),
		foucoco::PARACHAIN_ID.into(),
		false,
		vec![],
	))
	.build()
}

pub fn foucoco_standalone_config() -> FoucocoChainSpec {
	sp_core::crypto::set_default_ss58_version(foucoco_runtime::SS58Prefix::get().into());

	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "AMPE".into());
	properties.insert("tokenDecimals".into(), foucoco::TOKEN_DECIMALS.into());
	properties.insert("ss58Format".into(), foucoco_runtime::SS58Prefix::get().into());

	let mut signatories: Vec<_> = foucoco::INITIAL_SUDO_SIGNATORIES
		.iter()
		.map(|ss58| AccountId::from_ss58check(ss58).unwrap())
		.collect();
	signatories.sort();

	// add mock accounts to signatories so they get funded
	// Pre-funded accounts
	let prefunded_accounts = vec![
		get_account_id_from_seed::<sr25519::Public>("Bob"),
		get_account_id_from_seed::<sr25519::Public>("Charlie"),
		get_account_id_from_seed::<sr25519::Public>("Dave"),
		get_account_id_from_seed::<sr25519::Public>("Eve"),
		get_account_id_from_seed::<sr25519::Public>("Ferdie"),
		get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
		get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
		get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
		get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
		get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
		get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
	];

	signatories.extend(prefunded_accounts.iter().cloned());

	let invulnerables: Vec<_> = foucoco::INITIAL_COLLATORS
		.iter()
		.map(|ss58| AccountId::from_ss58check(ss58).unwrap())
		.collect();

	let sudo_account = get_account_id_from_seed::<sr25519::Public>("Alice");

	let offchain_worker_price_feeder =
		AccountId::from_ss58check(foucoco::OFF_CHAIN_WORKER_ADDRESS).unwrap();

	let allowed_currencies = vec![
		CurrencyId::Native,
		CurrencyId::XCM(0),
		CurrencyId::XCM(1),
		CurrencyId::XCM(2),
		CurrencyId::XCM(3),
		CurrencyId::XCM(4),
		CurrencyId::XCM(5),
		CurrencyId::XCM(6),
		CurrencyId::XCM(7),
		CurrencyId::XCM(8),
		CurrencyId::XCM(9),
		CurrencyId::XCM(10),
	];

	FoucocoChainSpec::builder(
		foucoco_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		ParachainExtensions {
			relay_chain: "kusama".into(), // You MUST set this to the correct network!
			para_id: foucoco::PARACHAIN_ID,
		},
	)
	.with_name("Foucoco-Standalone")
	.with_id("foucoco-standalone")
	.with_chain_type(ChainType::Live)
	.with_properties(get_amplitude_properties())
	.with_genesis_config_patch(foucoco_genesis(
		// initial collators.
		invulnerables.clone(),
		signatories.clone(),
		vec![sudo_account.clone(), offchain_worker_price_feeder.clone()],
		sudo_account.clone(),
		foucoco::PARACHAIN_ID.into(),
		false,
		allowed_currencies.clone(),
	))
	.build()
}

fn get_pendulum_properties() -> Map<String, Value> {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "PEN".into());
	properties.insert("tokenDecimals".into(), pendulum::TOKEN_DECIMALS.into());
	properties.insert("ss58Format".into(), pendulum_runtime::SS58Prefix::get().into());
	properties
}

pub fn pendulum_config() -> PendulumChainSpec {
	sp_core::crypto::set_default_ss58_version(pendulum_runtime::SS58Prefix::get().into());

	let multisig_genesis = create_pendulum_multisig_account(pendulum::MULTISIG_ID_GENESIS);
	let multisig_cl_reserves = create_pendulum_multisig_account(pendulum::MULTISIG_ID_CL_RESERVES);
	let multisig_incentives = create_pendulum_multisig_account(pendulum::MULTISIG_ID_INCENTIVES);
	let multisig_marketing = create_pendulum_multisig_account(pendulum::MULTISIG_ID_MARKETING);

	let collators: Vec<_> = pendulum::INITIAL_COLLATORS
		.iter()
		.map(|ss58| AccountId::from_ss58check(ss58).unwrap())
		.collect();

	let mut balances = vec![];

	let treasury = pallet_treasury::Pallet::<pendulum_runtime::Runtime>::account_id();

	for pendulum::Allocation { address, amount } in pendulum::ALLOCATIONS_10_24 {
		let account_id = AccountId::from_ss58check(address).unwrap();
		balances.push((account_id.clone(), amount * UNIT));
	}

	for pendulum::Allocation { address, amount } in pendulum::ALLOCATIONS_12_36 {
		let account_id = AccountId::from_ss58check(address).unwrap();
		balances.push((account_id.clone(), amount * UNIT));
	}

	for collator in collators.clone() {
		balances
			.push((collator, pendulum::INITIAL_COLLATOR_STAKING + pendulum::COLLATOR_ADDITIONAL));
	}

	balances.push((multisig_cl_reserves.clone(), pendulum::CL_RESERVES_ALLOCATION));

	balances.push((multisig_incentives.clone(), pendulum::INCENTIVES_ALLOCATION));

	balances.push((multisig_marketing.clone(), pendulum::MARKETING_ALLOCATION));

	balances.push((treasury.clone(), pendulum::TREASURY_ALLOCATION));

	let multisig_identifiers = [
		pendulum::MULTISIG_ID_GENESIS,
		pendulum::MULTISIG_ID_TEAM,
		pendulum::MULTISIG_ID_CL_RESERVES,
		pendulum::MULTISIG_ID_INCENTIVES,
		pendulum::MULTISIG_ID_MARKETING,
	];

	for signatory in pendulum::SUDO_SIGNATORIES.iter().chain(multisig_identifiers.iter()) {
		let account_id = AccountId::from_ss58check(signatory).unwrap();
		balances.push((account_id, pendulum::INITIAL_ISSUANCE_PER_SIGNATORY));
	}

	PendulumChainSpec::builder(
		pendulum_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		ParachainExtensions {
			relay_chain: "polkadot".into(), // You MUST set this to the correct network!
			para_id: pendulum::PARACHAIN_ID,
		},
	)
	.with_name("Pendulum")
	.with_id("pendulum")
	.with_chain_type(ChainType::Live)
	.with_properties(get_pendulum_properties())
	.with_genesis_config_patch(pendulum_genesis(
		// initial collators.
		collators.clone(),
		balances.clone(),
		vec![],
		multisig_genesis.clone(),
		pendulum::PARACHAIN_ID.into(),
		false,
	))
	.build()
}

fn default_pair(currency_id: CurrencyId, is_public_network: bool) -> VaultCurrencyPair<CurrencyId> {
	let wrapped =
		if is_public_network { MAINNET_USDC_CURRENCY_ID } else { TESTNET_USDC_CURRENCY_ID };
	VaultCurrencyPair { collateral: currency_id, wrapped }
}

fn amplitude_genesis(
	invulnerables: Vec<AccountId>,
	signatories: Vec<AccountId>,
	authorized_oracles: Vec<AccountId>,
	sudo_account: AccountId,
	id: ParaId,
	start_shutdown: bool,
) -> serde_json::Value {
	let balances: Vec<_> = signatories
		.iter()
		.cloned()
		.map(|k| (k, amplitude::INITIAL_ISSUANCE_PER_SIGNATORY))
		.chain(
			invulnerables
				.iter()
				.cloned()
				.map(|k| (k, amplitude::INITIAL_COLLATOR_STAKING + amplitude::COLLATOR_ADDITIONAL)),
		)
		.collect();

	let mut safe_balances = balances;
	safe_balances.push((sudo_account.clone(), MAX_SAFE_INTEGER_JSON - 1));

	let token_balances = vec![];

	let stakers: Vec<_> = invulnerables
		.iter()
		.cloned()
		.map(|account_id| (account_id, None, amplitude::INITIAL_COLLATOR_STAKING))
		.collect();

	let inflation_config = amplitude_runtime::InflationInfo::new(
		amplitude_runtime::BLOCKS_PER_YEAR.into(),
		Perquintill::from_percent(10),
		Perquintill::from_percent(11),
		Perquintill::from_percent(40),
		Perquintill::from_percent(9),
	);

	let genesis_config = amplitude_runtime::RuntimeGenesisConfig {
		asset_registry: Default::default(),
		system: amplitude_runtime::SystemConfig {
			#[allow(clippy::wrong_self_convention)]
			_config: sp_std::marker::PhantomData::default(),
		},
		balances: amplitude_runtime::BalancesConfig { balances: safe_balances },
		parachain_info: amplitude_runtime::ParachainInfoConfig {
			parachain_id: id,
			_config: sp_std::marker::PhantomData::default(),
		},
		parachain_staking: amplitude_runtime::ParachainStakingConfig {
			stakers,
			inflation_config,
			max_candidate_stake: 400_000 * UNIT,
			max_selected_candidates: 40,
		},
		session: amplitude_runtime::SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|acc| {
					(
						acc.clone(),
						acc.clone(),
						get_amplitude_session_keys(Into::<[u8; 32]>::into(acc).unchecked_into()),
					)
				})
				.collect(),
		},
		// no need to pass anything to aura, in fact it will panic if we do. Session will take care
		// of this.
		aura: Default::default(),
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: amplitude_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
			_config: sp_std::marker::PhantomData::default(),
		},
		council: amplitude_runtime::CouncilConfig {
			members: signatories.clone(),
			phantom: Default::default(),
		},
		democracy: Default::default(),
		technical_committee: amplitude_runtime::TechnicalCommitteeConfig {
			members: signatories,
			phantom: sp_std::marker::PhantomData::default(),
		},
		tokens: amplitude_runtime::TokensConfig {
			// Configure the initial token supply
			balances: token_balances,
		},
		issue: amplitude_runtime::IssueConfig {
			issue_period: amplitude_runtime::DAYS,
			issue_minimum_transfer_amount: 1_000_000_000,
			limit_volume_amount: None,
			limit_volume_currency_id: XCM(0),
			current_volume_amount: 0u32.into(),
			interval_length: (60u32 * 60 * 24),
			last_interval_index: 0u32,
		},
		redeem: amplitude_runtime::RedeemConfig {
			redeem_period: amplitude_runtime::DAYS,
			redeem_minimum_transfer_amount: 1_000_000_000,
			limit_volume_amount: None,
			limit_volume_currency_id: XCM(0),
			current_volume_amount: 0u32.into(),
			interval_length: (60u32 * 60 * 24),
			last_interval_index: 0u32,
		},
		replace: amplitude_runtime::ReplaceConfig {
			replace_period: amplitude_runtime::DAYS,
			replace_minimum_transfer_amount: 1_000_000_000,
		},
		security: amplitude_runtime::SecurityConfig {
			initial_status: if start_shutdown {
				amplitude_runtime::StatusCode::Shutdown
			} else {
				amplitude_runtime::StatusCode::Error
			},
			_phantom: sp_std::marker::PhantomData::default(),
		},
		oracle: amplitude_runtime::OracleConfig {
			max_delay: u32::MAX,
			oracle_keys: vec![
				Key::ExchangeRate(CurrencyId::Native),
				Key::ExchangeRate(CurrencyId::XCM(0)),
				Key::ExchangeRate(MAINNET_USDC_CURRENCY_ID),
			],
			_phantom: sp_std::marker::PhantomData::default(),
		},
		vault_registry: amplitude_runtime::VaultRegistryConfig {
			minimum_collateral_vault: vec![(XCM(0), 0)],
			punishment_delay: amplitude_runtime::DAYS,
			secure_collateral_threshold: vec![(
				default_pair(XCM(0), true),
				FixedU128::checked_from_rational(160, 100).unwrap(),
			)],
			/* 150% */
			premium_redeem_threshold: vec![(
				default_pair(XCM(0), true),
				FixedU128::checked_from_rational(140, 100).unwrap(),
			)],
			/* 130% */
			liquidation_collateral_threshold: vec![(
				default_pair(XCM(0), true),
				FixedU128::checked_from_rational(120, 100).unwrap(),
			)],
			/* 120% */
			system_collateral_ceiling: vec![(
				default_pair(XCM(0), true),
				60_000 * 10u128.pow(amplitude::TOKEN_DECIMALS),
			)],
		},
		stellar_relay: amplitude_runtime::StellarRelayConfig::default(),
		fee: amplitude_runtime::FeeConfig {
			issue_fee: FixedU128::checked_from_rational(15, 10000).unwrap(), // 0.15%
			issue_griefing_collateral: FixedU128::checked_from_rational(5, 100000).unwrap(), // 0.005%
			redeem_fee: FixedU128::checked_from_rational(5, 1000).unwrap(),  // 0.5%
			premium_redeem_fee: FixedU128::checked_from_rational(5, 100).unwrap(), // 5%
			punishment_fee: FixedU128::checked_from_rational(1, 10).unwrap(), // 10%
			replace_griefing_collateral: FixedU128::checked_from_rational(1, 10).unwrap(), // 10%
		},
		nomination: amplitude_runtime::NominationConfig {
			is_nomination_enabled: false,
			_phantom: sp_std::marker::PhantomData::default(),
		},
		dia_oracle_module: amplitude_runtime::DiaOracleModuleConfig {
			authorized_accounts: authorized_oracles,
			supported_currencies: vec![
				amplitude_runtime::AssetId::new(b"Amplitude".to_vec(), b"AMPE".to_vec()),
				amplitude_runtime::AssetId::new(b"Kusama".to_vec(), b"KSM".to_vec()),
				amplitude_runtime::AssetId::new(b"Stellar".to_vec(), b"XLM".to_vec()),
				amplitude_runtime::AssetId::new(b"FIAT".to_vec(), b"USD-USD".to_vec()),
			],
			batching_api: b"https://dia-00.pendulumchain.tech/currencies".to_vec(),
			coin_infos_map: vec![],
		},
		token_allowance: Default::default(),
		transaction_payment: Default::default(),
		treasury: Default::default(),
		treasury_buyout_extension: Default::default(),
		vesting: Default::default(),
		zenlink_protocol: Default::default(),
	};

	serde_json::to_value(genesis_config).expect("Serialization of genesis config should work")
}

fn foucoco_genesis(
	invulnerables: Vec<AccountId>,
	signatories: Vec<AccountId>,
	authorized_oracles: Vec<AccountId>,
	sudo_account: AccountId,
	id: ParaId,
	start_shutdown: bool,
	allowed_currencies: Vec<CurrencyId>,
) -> serde_json::Value {
	fn get_vault_currency_pair(
		collateral: CurrencyId,
		wrapped: CurrencyId,
	) -> VaultCurrencyPair<CurrencyId> {
		VaultCurrencyPair { collateral, wrapped }
	}

	let balances: Vec<_> = signatories
		.iter()
		.cloned()
		.map(|k| (k, foucoco::INITIAL_ISSUANCE_PER_SIGNATORY))
		.chain(
			invulnerables
				.iter()
				.cloned()
				.map(|k| (k, foucoco::INITIAL_COLLATOR_STAKING + foucoco::COLLATOR_ADDITIONAL)),
		)
		.collect();

	let mut safe_balances = balances;
	safe_balances.push((sudo_account.clone(), MAX_SAFE_INTEGER_JSON - 1));

	let token_balances = safe_balances
		.iter()
		.flat_map(|k| vec![(k.0.clone(), XCM(0), u128::pow(10, 18))])
		.collect();

	let stakers: Vec<_> = invulnerables
		.iter()
		.cloned()
		.map(|account_id| (account_id, None, foucoco::INITIAL_COLLATOR_STAKING))
		.collect();

	let inflation_config = foucoco_runtime::InflationInfo::new(
		foucoco_runtime::BLOCKS_PER_YEAR.into(),
		Perquintill::from_percent(10),
		Perquintill::from_percent(11),
		Perquintill::from_percent(40),
		Perquintill::from_percent(9),
	);

	let genesis_config = foucoco_runtime::RuntimeGenesisConfig {
		asset_registry: Default::default(),
		system: foucoco_runtime::SystemConfig { _config: sp_std::marker::PhantomData::default() },
		balances: foucoco_runtime::BalancesConfig { balances: safe_balances },
		parachain_info: foucoco_runtime::ParachainInfoConfig {
			parachain_id: id,
			_config: sp_std::marker::PhantomData::default(),
		},
		parachain_staking: foucoco_runtime::ParachainStakingConfig {
			stakers,
			inflation_config,
			max_candidate_stake: 400_000 * UNIT,
			max_selected_candidates: 40,
		},
		session: foucoco_runtime::SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|acc| {
					(
						acc.clone(),
						acc.clone(),
						get_foucoco_session_keys(Into::<[u8; 32]>::into(acc).unchecked_into()),
					)
				})
				.collect(),
		},
		// no need to pass anything to aura, in fact it will panic if we do. Session will take care
		// of this.
		aura: Default::default(),
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: foucoco_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
			_config: sp_std::marker::PhantomData::default(),
		},
		council: foucoco_runtime::CouncilConfig {
			members: signatories.clone(),
			..Default::default()
		},
		democracy: Default::default(),
		sudo: foucoco_runtime::SudoConfig { key: Some(sudo_account) },
		technical_committee: foucoco_runtime::TechnicalCommitteeConfig {
			members: signatories,
			..Default::default()
		},
		tokens: foucoco_runtime::TokensConfig {
			// Configure the initial token supply for the native currency and USDC asset
			balances: token_balances,
		},
		issue: foucoco_runtime::IssueConfig {
			issue_period: foucoco_runtime::DAYS,
			issue_minimum_transfer_amount: 1_000_000_000,
			limit_volume_amount: None,
			limit_volume_currency_id: XCM(0),
			current_volume_amount: 0u32.into(),
			interval_length: (60u32 * 60 * 24),
			last_interval_index: 0u32,
		},
		redeem: foucoco_runtime::RedeemConfig {
			redeem_period: foucoco_runtime::DAYS,
			redeem_minimum_transfer_amount: 1_000_000_000,
			limit_volume_amount: None,
			limit_volume_currency_id: XCM(0),
			current_volume_amount: 0u32.into(),
			interval_length: (60u32 * 60 * 24),
			last_interval_index: 0u32,
		},
		replace: foucoco_runtime::ReplaceConfig {
			replace_period: foucoco_runtime::DAYS,
			replace_minimum_transfer_amount: 1_000_000_000,
		},
		security: foucoco_runtime::SecurityConfig {
			initial_status: if start_shutdown {
				foucoco_runtime::StatusCode::Shutdown
			} else {
				foucoco_runtime::StatusCode::Error
			},
			_phantom: sp_std::marker::PhantomData::default(),
		},
		oracle: foucoco_runtime::OracleConfig {
			max_delay: 604_800_000, // 7 days
			oracle_keys: vec![
				Key::ExchangeRate(CurrencyId::XCM(0)),
				Key::ExchangeRate(CurrencyId::Native),
				Key::ExchangeRate(CurrencyId::Stellar(Asset::StellarNative)),
				Key::ExchangeRate(TESTNET_USDC_CURRENCY_ID),
			],
			_phantom: sp_std::marker::PhantomData::default(),
		},
		vault_registry: foucoco_runtime::VaultRegistryConfig {
			minimum_collateral_vault: vec![(XCM(0), 0)],
			punishment_delay: foucoco_runtime::DAYS * 2,
			secure_collateral_threshold: vec![
				(default_pair(XCM(0), false), FixedU128::checked_from_rational(160, 100).unwrap()),
				(
					get_vault_currency_pair(XCM(0), CurrencyId::Stellar(Asset::StellarNative)),
					FixedU128::checked_from_rational(160, 100).unwrap(),
				),
			],
			/* 140% */
			premium_redeem_threshold: vec![
				(default_pair(XCM(0), false), FixedU128::checked_from_rational(140, 100).unwrap()),
				(
					get_vault_currency_pair(XCM(0), CurrencyId::Stellar(Asset::StellarNative)),
					FixedU128::checked_from_rational(140, 100).unwrap(),
				),
			],
			/* 125% */
			liquidation_collateral_threshold: vec![
				(default_pair(XCM(0), false), FixedU128::checked_from_rational(125, 100).unwrap()),
				(
					get_vault_currency_pair(XCM(0), CurrencyId::Stellar(Asset::StellarNative)),
					FixedU128::checked_from_rational(125, 100).unwrap(),
				),
			],
			system_collateral_ceiling: vec![
				(default_pair(XCM(0), false), 50 * 10u128.pow(foucoco::TOKEN_DECIMALS)),
				(
					get_vault_currency_pair(XCM(0), CurrencyId::Stellar(Asset::StellarNative)),
					50 * 10u128.pow(foucoco::TOKEN_DECIMALS),
				),
			],
		},
		stellar_relay: foucoco_runtime::StellarRelayConfig::default(),
		fee: foucoco_runtime::FeeConfig {
			issue_fee: FixedU128::checked_from_rational(1, 1000).unwrap(), // 0.1%
			issue_griefing_collateral: FixedU128::checked_from_rational(5, 1000).unwrap(), // 0.5%
			redeem_fee: FixedU128::checked_from_rational(1, 1000).unwrap(), // 0.1%
			premium_redeem_fee: FixedU128::checked_from_rational(5, 100).unwrap(), // 5%
			punishment_fee: FixedU128::checked_from_rational(1, 10).unwrap(), // 10%
			replace_griefing_collateral: FixedU128::checked_from_rational(1, 10).unwrap(), // 10%
		},
		nomination: foucoco_runtime::NominationConfig {
			is_nomination_enabled: false,
			_phantom: sp_std::marker::PhantomData::default(),
		},
		dia_oracle_module: foucoco_runtime::DiaOracleModuleConfig {
			authorized_accounts: authorized_oracles,
			supported_currencies: vec![
				foucoco_runtime::AssetId::new(b"Amplitude".to_vec(), b"AMPE".to_vec()),
				foucoco_runtime::AssetId::new(b"Kusama".to_vec(), b"KSM".to_vec()),
				foucoco_runtime::AssetId::new(b"Stellar".to_vec(), b"XLM".to_vec()),
				foucoco_runtime::AssetId::new(b"FIAT".to_vec(), b"USD-USD".to_vec()),
			],
			batching_api: b"https://dia-00.pendulumchain.tech/currencies".to_vec(),
			coin_infos_map: vec![],
		},
		token_allowance: foucoco_runtime::TokenAllowanceConfig { allowed_currencies },
		transaction_payment: Default::default(),
		treasury: Default::default(),
		treasury_buyout_extension: Default::default(),
		vesting: Default::default(),
		zenlink_protocol: Default::default(),
	};

	serde_json::to_value(genesis_config).expect("Serialization of genesis config should work")
}

fn pendulum_genesis(
	collators: Vec<AccountId>,
	balances: Vec<(AccountId, Balance)>,
	authorized_oracles: Vec<AccountId>,
	sudo_account: AccountId,
	id: ParaId,
	start_shutdown: bool,
) -> serde_json::Value {
	let mut safe_balances = limit_balance_for_serialization(balances);
	safe_balances.push((sudo_account, MAX_SAFE_INTEGER_JSON - 1));

	let stakers: Vec<_> = collators
		.iter()
		.cloned()
		.map(|account_id| (account_id, None, pendulum::INITIAL_COLLATOR_STAKING))
		.collect();

	let inflation_config = pendulum_runtime::InflationInfo::new(
		pendulum_runtime::BLOCKS_PER_YEAR.into(),
		Perquintill::from_percent(10),
		Perquintill::from_percent(11),
		Perquintill::from_percent(30),
		Perquintill::from_percent(8),
	);

	let council: Vec<_> = pendulum::SUDO_SIGNATORIES
		.iter()
		.map(|address| AccountId::from_ss58check(address).unwrap())
		.collect();

	let genesis_config = pendulum_runtime::RuntimeGenesisConfig {
		asset_registry: Default::default(),
		system: pendulum_runtime::SystemConfig { _config: sp_std::marker::PhantomData::default() },
		balances: pendulum_runtime::BalancesConfig { balances: safe_balances },
		parachain_info: pendulum_runtime::ParachainInfoConfig {
			parachain_id: id,
			_config: sp_std::marker::PhantomData::default(),
		},
		parachain_staking: pendulum_runtime::ParachainStakingConfig {
			stakers,
			inflation_config,
			max_candidate_stake: 400_000 * UNIT,
			max_selected_candidates: 40,
		},
		session: pendulum_runtime::SessionConfig {
			keys: collators
				.into_iter()
				.map(|account| {
					(
						account.clone(),
						account.clone(),
						get_pendulum_session_keys(Into::<[u8; 32]>::into(account).unchecked_into()),
					)
				})
				.collect(),
		},
		// no need to pass anything to aura, in fact it will panic if we do. Session will take care
		// of this.
		aura: Default::default(),
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: pendulum_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
			_config: sp_std::marker::PhantomData::default(),
		},
		council: pendulum_runtime::CouncilConfig { members: council.clone(), ..Default::default() },
		democracy: Default::default(),
		technical_committee: pendulum_runtime::TechnicalCommitteeConfig {
			members: council,
			..Default::default()
		},
		vesting: Default::default(),
		issue: pendulum_runtime::IssueConfig {
			issue_period: pendulum_runtime::DAYS,
			issue_minimum_transfer_amount: 1_000_000_000,
			limit_volume_amount: None,
			limit_volume_currency_id: XCM(0),
			current_volume_amount: 0u32.into(),
			interval_length: (60u32 * 60 * 24),
			last_interval_index: 0u32,
		},
		redeem: pendulum_runtime::RedeemConfig {
			redeem_period: pendulum_runtime::DAYS,
			redeem_minimum_transfer_amount: 1_000_000_000,
			limit_volume_amount: None,
			limit_volume_currency_id: XCM(0),
			current_volume_amount: 0u32.into(),
			interval_length: (60u32 * 60 * 24),
			last_interval_index: 0u32,
		},
		replace: pendulum_runtime::ReplaceConfig {
			replace_period: pendulum_runtime::DAYS,
			replace_minimum_transfer_amount: 1_000_000_000,
		},
		security: pendulum_runtime::SecurityConfig {
			initial_status: if start_shutdown {
				pendulum_runtime::StatusCode::Shutdown
			} else {
				pendulum_runtime::StatusCode::Error
			},
			_phantom: sp_std::marker::PhantomData::default(),
		},
		oracle: pendulum_runtime::OracleConfig {
			max_delay: u32::MAX,
			oracle_keys: vec![
				Key::ExchangeRate(CurrencyId::Native),
				Key::ExchangeRate(CurrencyId::XCM(0)),
				Key::ExchangeRate(MAINNET_USDC_CURRENCY_ID),
			],
			_phantom: sp_std::marker::PhantomData::default(),
		},
		vault_registry: pendulum_runtime::VaultRegistryConfig {
			minimum_collateral_vault: vec![(XCM(0), 0)],
			punishment_delay: pendulum_runtime::DAYS,
			secure_collateral_threshold: vec![(
				default_pair(XCM(0), true),
				FixedU128::checked_from_rational(150, 100).unwrap(),
			)],
			/* 150% */
			premium_redeem_threshold: vec![(
				default_pair(XCM(0), true),
				FixedU128::checked_from_rational(130, 100).unwrap(),
			)],
			/* 130% */
			liquidation_collateral_threshold: vec![(
				default_pair(XCM(0), true),
				FixedU128::checked_from_rational(120, 100).unwrap(),
			)],
			/* 120% */
			system_collateral_ceiling: vec![(
				default_pair(XCM(0), true),
				60_000 * 10u128.pow(pendulum::TOKEN_DECIMALS),
			)],
		},
		stellar_relay: pendulum_runtime::StellarRelayConfig::default(),
		fee: pendulum_runtime::FeeConfig {
			issue_fee: FixedU128::checked_from_rational(15, 10000).unwrap(), // 0.15%
			issue_griefing_collateral: FixedU128::checked_from_rational(5, 100000).unwrap(), // 0.005%
			redeem_fee: FixedU128::checked_from_rational(5, 1000).unwrap(),  // 0.5%
			premium_redeem_fee: FixedU128::checked_from_rational(5, 100).unwrap(), // 5%
			punishment_fee: FixedU128::checked_from_rational(1, 10).unwrap(), // 10%
			replace_griefing_collateral: FixedU128::checked_from_rational(1, 10).unwrap(), // 10%
		},
		nomination: pendulum_runtime::NominationConfig {
			is_nomination_enabled: false,
			..Default::default()
		},
		dia_oracle_module: pendulum_runtime::DiaOracleModuleConfig {
			authorized_accounts: authorized_oracles,
			supported_currencies: vec![
				pendulum_runtime::AssetId::new(b"Pendulum".to_vec(), b"PEN".to_vec()),
				pendulum_runtime::AssetId::new(b"Polkadot".to_vec(), b"DOT".to_vec()),
				pendulum_runtime::AssetId::new(b"Stellar".to_vec(), b"XLM".to_vec()),
				pendulum_runtime::AssetId::new(b"FIAT".to_vec(), b"USD-USD".to_vec()),
			],
			batching_api: b"https://dia-00.pendulumchain.tech/currencies".to_vec(),
			coin_infos_map: vec![],
		},
		token_allowance: Default::default(),
		transaction_payment: Default::default(),
		treasury: Default::default(),
		tokens: Default::default(),
		treasury_buyout_extension: Default::default(),
	};

	serde_json::to_value(genesis_config).expect("Serialization of genesis config should work")
}

fn limit_balance_for_serialization(
	balances: Vec<(AccountId, Balance)>,
) -> Vec<(AccountId, Balance)> {
	balances
		.into_iter()
		.map(|balance| {
			if balance.1 >= MAX_SAFE_INTEGER_JSON {
				return (balance.0, MAX_SAFE_INTEGER_JSON - 1);
			}
			balance
		})
		.collect::<Vec<(AccountId, Balance)>>()
}

// These tests are useful to verify the conversion of the ChainSpec struct to the serialized json.
#[test]
fn test_genesis_serialization() {
	pendulum_config();
	foucoco_config();
	amplitude_config();
}
