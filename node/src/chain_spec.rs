use cumulus_primitives_core::ParaId;
use runtime_common::{AccountId, AuraId, Balance, Signature, EXISTENTIAL_DEPOSIT, UNIT};
use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup};
use sc_service::ChainType;
use serde::{Deserialize, Serialize};
use sp_core::{
	crypto::{Ss58Codec, UncheckedInto},
	sr25519, Pair, Public,
};
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	Perquintill,
};

/// Specialized `ChainSpec` for the normal parachain runtime.
pub type AmplitudeChainSpec =
	sc_service::GenericChainSpec<amplitude_runtime::GenesisConfig, Extensions>;

pub type DevelopmentChainSpec =
	sc_service::GenericChainSpec<development_runtime::GenesisConfig, Extensions>;

/// The default XCM version to set in genesis config.
const SAFE_XCM_VERSION: u32 = xcm::prelude::XCM_VERSION;

const AMPLITUDE_PARACHAIN_ID: u32 = 2124;

const INITIAL_ISSUANCE_PER_SIGNATORY: Balance = 200 * UNIT;
const INITIAL_COLLATOR_STAKING: Balance = 10_000 * UNIT;

const INITIAL_AMPLITUDE_SUDO_SIGNATORIES: [&str; 5] = [
	"6nJwMD3gk36fe6pMRL2UpbwAEjDdjjxdngQGShe753pyAvCT",
	"6i4xDEE1Q2Bv8tnJtgD4jse4YTAzzCwCJVUehRQ93hCqKp8f",
	"6n62KZWvmZHgeyEXTvQFmoHRMqjKfFWvQVsApkePekuNfek5",
	"6kwxQBRKMadrY9Lq3K8gZXkw1UkjacpqYhcqvX3AqmN9DofF",
	"6kKHwcpCVC18KepwvdMSME8Q7ZTNr1RoRUrFDH9AdAmhL3Pt",
];

const INITIAL_AMPLITUDE_VALIDATORS: [&str; 8] = [
	"6mTATq7Ug9RPk4s8aMv5H7WVZ7RvwrJ1JitbYMXWPhanzqiv",
	"6n8WiWqjEB8nCNRo5mxXc89FqhuMd2dgXNSrzuPxoZSnatnL",
	"6ic56zZmjqo746yifWzcNxxzxLe3pRo8WNitotniUQvgKnyU",
	"6gvFApEyYj4EavJP26mwbVu7YxFBYZ9gaJFB7gv5gA6vNfze",
	"6mz3ymVAsfHotEhHphVRvLLBhMZ2frnwbuvW5QZiMRwJghxE",
	"6mpD3zcHcUBkxCjTsGg2tMTfmQZdXLVYZnk4UkN2XAUTLkRe",
	"6mGcZntk59RK2JfxfdmprgDJeByVUgaffMQYkp1ZeoEKeBJA",
	"6jq7obxC7AxhWeJNzopwYidKNNe48cLrbGSgB2zs2SuRTWGA",
];

/// Helper function to generate a crypto pair from seed
pub fn get_public_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

/// The extensions for the [`ChainSpec`].
#[derive(
	Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension,
)]
#[serde(deny_unknown_fields)]
pub struct Extensions {
	/// The relay chain of the Parachain.
	pub relay_chain: String,
	/// The id of the Parachain.
	pub para_id: u32,
}

impl Extensions {
	/// Try to get the extension from the given `ChainSpec`.
	pub fn try_get(chain_spec: &dyn sc_service::ChainSpec) -> Option<&Self> {
		sc_chain_spec::get_extension(chain_spec.extensions())
	}
}

type AccountPublic = <Signature as Verify>::Signer;

/// Generate collator keys from seed.
///
/// This function's return type must always match the session keys of the chain in tuple format.
pub fn get_collator_keys_from_seed(seed: &str) -> AuraId {
	get_public_from_seed::<AuraId>(seed)
}

/// Helper function to generate an account ID from seed
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_public_from_seed::<TPublic>(seed)).into_account()
}

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn get_amplitude_session_keys(keys: AuraId) -> amplitude_runtime::SessionKeys {
	amplitude_runtime::SessionKeys { aura: keys }
}

pub fn get_development_session_keys(keys: AuraId) -> development_runtime::SessionKeys {
	development_runtime::SessionKeys { aura: keys }
}

pub fn amplitude_config() -> AmplitudeChainSpec {
	// Give your base currency a unit name and decimal places
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "AMPE".into());
	properties.insert("tokenDecimals".into(), 12u32.into());
	properties.insert("ss58Format".into(), amplitude_runtime::SS58Prefix::get().into());

	let mut signatories: Vec<_> = INITIAL_AMPLITUDE_SUDO_SIGNATORIES
		.iter()
		.map(|ss58| AccountId::from_ss58check(ss58).unwrap())
		.collect();
	signatories.sort();

	let invulnerables: Vec<_> = INITIAL_AMPLITUDE_VALIDATORS
		.iter()
		.map(|ss58| AccountId::from_ss58check(ss58).unwrap())
		.collect();

	AmplitudeChainSpec::from_genesis(
		// Name
		"Amplitude",
		// ID
		"amplitude",
		ChainType::Live,
		move || {
			amplitude_genesis(
				// initial collators.
				invulnerables.clone(),
				signatories.clone(),
				AMPLITUDE_PARACHAIN_ID.into(),
			)
		},
		// Bootnodes
		Vec::new(),
		// Telemetry
		None,
		// Protocol ID
		Some("amplitude"),
		// Fork ID
		None,
		// Properties
		Some(properties),
		// Extensions
		Extensions {
			relay_chain: "kusama".into(), // You MUST set this to the correct network!
			para_id: AMPLITUDE_PARACHAIN_ID,
		},
	)
}

pub fn development_config() -> DevelopmentChainSpec {
	// Give your base currency a unit name and decimal places
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "UNIT".into());
	properties.insert("tokenDecimals".into(), 12.into());
	properties.insert("ss58Format".into(), 42.into());

	DevelopmentChainSpec::from_genesis(
		// Name
		"Development",
		// ID
		"dev",
		ChainType::Development,
		move || {
			testnet_genesis(
				// initial collators.
				vec![
					(
						get_account_id_from_seed::<sr25519::Public>("Alice"),
						get_collator_keys_from_seed("Alice"),
					),
					(
						get_account_id_from_seed::<sr25519::Public>("Bob"),
						get_collator_keys_from_seed("Bob"),
					),
				],
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
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
				],
				1000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		None,
		Extensions {
			relay_chain: "rococo-local".into(), // You MUST set this to the correct network!
			para_id: 1000,
		},
	)
}

pub fn local_testnet_config() -> DevelopmentChainSpec {
	// Give your base currency a unit name and decimal places
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "UNIT".into());
	properties.insert("tokenDecimals".into(), 12.into());
	properties.insert("ss58Format".into(), 42.into());

	DevelopmentChainSpec::from_genesis(
		// Name
		"Local Testnet",
		// ID
		"local_testnet",
		ChainType::Local,
		move || {
			testnet_genesis(
				// initial collators.
				vec![
					(
						get_account_id_from_seed::<sr25519::Public>("Alice"),
						get_collator_keys_from_seed("Alice"),
					),
					(
						get_account_id_from_seed::<sr25519::Public>("Bob"),
						get_collator_keys_from_seed("Bob"),
					),
				],
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
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
				],
				1000.into(),
			)
		},
		// Bootnodes
		Vec::new(),
		// Telemetry
		None,
		// Protocol ID
		Some("pendulum-development"),
		// Fork ID
		None,
		// Properties
		Some(properties),
		// Extensions
		Extensions {
			relay_chain: "rococo-local".into(), // You MUST set this to the correct network!
			para_id: 1000,
		},
	)
}

fn amplitude_genesis(
	invulnerables: Vec<AccountId>,
	signatories: Vec<AccountId>,
	id: ParaId,
) -> amplitude_runtime::GenesisConfig {
	let balances: Vec<_> = signatories
		.iter()
		.cloned()
		.map(|k| (k, INITIAL_ISSUANCE_PER_SIGNATORY))
		.chain(invulnerables.iter().cloned().map(|k| (k, INITIAL_COLLATOR_STAKING)))
		.collect();

	let stakers: Vec<_> = invulnerables
		.iter()
		.cloned()
		.map(|account_id| (account_id, None, INITIAL_COLLATOR_STAKING))
		.collect();

	// Review Comment: After adding `pub use parachain_staking::InflationInfo;` in `runtime/amplitude/lib.rs`
	// We can remove `parachain-staking` from `Cargo.toml` and
	// We can replace `parachain_staking::InflationInfo` with `amplitude_runtime::InflationInfo`
	let inflation_config = parachain_staking::InflationInfo::new(
		amplitude_runtime::BLOCKS_PER_YEAR.into(),
		Perquintill::from_percent(10),
		Perquintill::from_percent(11),
		Perquintill::from_percent(40),
		Perquintill::from_percent(9),
	);

	amplitude_runtime::GenesisConfig {
		system: amplitude_runtime::SystemConfig {
			code: amplitude_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		balances: amplitude_runtime::BalancesConfig { balances },
		parachain_info: amplitude_runtime::ParachainInfoConfig { parachain_id: id },
		parachain_staking: amplitude_runtime::ParachainStakingConfig {
			stakers,
			inflation_config,
			max_candidate_stake: 400_000 * UNIT,
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
		},
		council: amplitude_runtime::CouncilConfig {
			members: signatories.clone(),
			..Default::default()
		},
		democracy: Default::default(),
		technical_committee: amplitude_runtime::TechnicalCommitteeConfig {
			members: signatories,
			..Default::default()
		},
	}
}

fn testnet_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> development_runtime::GenesisConfig {
	development_runtime::GenesisConfig {
		system: development_runtime::SystemConfig {
			code: development_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		balances: development_runtime::BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
		},
		parachain_info: development_runtime::ParachainInfoConfig { parachain_id: id },
		collator_selection: development_runtime::CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: EXISTENTIAL_DEPOSIT * 16,
			..Default::default()
		},
		session: development_runtime::SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                        // account id
						acc,                                // validator id
						get_development_session_keys(aura), // session keys
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
		},
	}
}
