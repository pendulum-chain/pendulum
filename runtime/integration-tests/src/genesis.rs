use crate::*;
// Substrate
use sp_core::{sr25519, storage::Storage};

// Cumulus
use polkadot_parachain::primitives::{HeadData, ValidationCode};
use integration_tests_common::{
    constants::{accounts, collators, validators, polkadot, asset_hub_polkadot},
};

use runtime_common::Balance;

pub const ED: Balance = runtime_common::EXISTENTIAL_DEPOSIT;
pub const STASH: u128 = 100 * polkadot_runtime_constants::currency::UNITS;

pub const SAFE_XCM_VERSION: u32 = 3;

use spacewalk_primitives::{CurrencyId::XCM, CurrencyId};
use crate::mock::{assets_metadata_for_registry_pendulum,units};

use polkadot_runtime_parachains::{
    paras::{ParaGenesisArgs, ParaKind},
};
pub use sp_runtime::{MultiAddress, Perbill, Permill, Perquintill};

pub fn relay_genesis() -> Storage {
    use polkadot_runtime::BuildStorage;

    let genesis_config = polkadot_runtime::RuntimeGenesisConfig {
        system: polkadot_runtime::SystemConfig {
            code: polkadot_runtime::WASM_BINARY.unwrap().to_vec(),
            ..Default::default()
        },
        balances: polkadot_runtime::BalancesConfig {
            balances: accounts::init_balances()
                .iter()
                .cloned()
                .map(|k| (k, ED * 4096))
                .collect(),
        },
        session: polkadot_runtime::SessionConfig {
            keys: validators::initial_authorities()
                .iter()
                .map(|x| {
                    (
                        x.0.clone(),
                        x.0.clone(),
                        polkadot_runtime::SessionKeys {
                            babe: x.2.clone(),
                            grandpa: x.3.clone(),
                            im_online: x.4.clone(),
                            para_validator: x.5.clone(),
                            para_assignment: x.6.clone(),
                            authority_discovery: x.7.clone(),
                        }
                    )
                })
                .collect::<Vec<_>>(),
        },
        staking: polkadot_runtime::StakingConfig {
            validator_count: validators::initial_authorities().len() as u32,
            minimum_validator_count: 1,
            stakers: validators::initial_authorities()
                .iter()
                .map(|x| {
                    (x.0.clone(), x.1.clone(), STASH, polkadot_runtime::StakerStatus::Validator)
                })
                .collect(),
            invulnerables: validators::initial_authorities()
                .iter()
                .map(|x| x.0.clone())
                .collect(),
            force_era: pallet_staking::Forcing::ForceNone,
            slash_reward_fraction: Perbill::from_percent(10),
            ..Default::default()
        },
        babe: polkadot_runtime::BabeConfig {
            authorities: Default::default(),
            epoch_config: Some(polkadot_runtime::BABE_GENESIS_EPOCH_CONFIG),
            ..Default::default()
        },
        configuration: polkadot_runtime::ConfigurationConfig { config: polkadot::get_host_config() },
        paras: polkadot_runtime::ParasConfig {
            paras: vec![
                (
                    asset_hub_polkadot::PARA_ID.into(),
                    ParaGenesisArgs {
                        genesis_head: HeadData::default(),
                        validation_code: ValidationCode(
                            asset_hub_polkadot_runtime::WASM_BINARY.unwrap().to_vec(),
                        ),
                        para_kind: ParaKind::Parachain,
                    },
                ),
                (
                    crate::PENDULUM_ID.into(),
                    ParaGenesisArgs {
                        genesis_head: HeadData::default(),
                        validation_code: ValidationCode(
                            pendulum_runtime::WASM_BINARY.unwrap().to_vec(),
                        ),
                        para_kind: ParaKind::Parachain,
                    },
                ),
            ],
            ..Default::default()
        },
        ..Default::default()
    };

    genesis_config.build_storage().unwrap()
}

pub fn genesis(para_id: u32) -> Storage {
    use pendulum_runtime::BuildStorage;

    let token_balances = accounts::init_balances()
        .iter()
        .flat_map(|k| vec![(k.clone(), XCM(0), units(1000))])
        .collect();

    let genesis_config = pendulum_runtime::RuntimeGenesisConfig {
        system: pendulum_runtime::SystemConfig {
            code: pendulum_runtime::WASM_BINARY
                .expect("WASM binary was not build, please build it!")
                .to_vec(),
            ..Default::default()
        },
        balances: pendulum_runtime::BalancesConfig {
            balances: accounts::init_balances()
                .iter()
                .cloned()
                .map(|k| (k, units(1000)))
                .collect(),
        },
        tokens: pendulum_runtime::TokensConfig {
            balances: token_balances
        },
        parachain_info: pendulum_runtime::ParachainInfoConfig {
            parachain_id: para_id.into(),
            ..Default::default()
        },
        session: pendulum_runtime::SessionConfig {
            keys: collators::invulnerables()
                .into_iter()
                .map(|(acc, aura)| {
                    (
                        acc.clone(),                          // account id
                        acc,                                  // validator id
                        pendulum_runtime::SessionKeys { aura }, // session keys
                    )
                })
                .collect(),
        },
        polkadot_xcm: pendulum_runtime::PolkadotXcmConfig {
            safe_xcm_version: Some(SAFE_XCM_VERSION),
            ..Default::default()
        },
        asset_registry: pendulum_runtime::AssetRegistryConfig {
            assets: assets_metadata_for_registry_pendulum(),
            last_asset_id: CurrencyId::Native,
        },
        ..Default::default()
    };

    genesis_config.build_storage().unwrap()
}

pub fn genesis_sibling(para_id: u32) -> Storage {
    use sibling::BuildStorage;

    let token_balances = accounts::init_balances()
        .iter()
        .flat_map(|k| vec![(k.clone(), sibling::CurrencyId::XCM(0), units(100))])
        .collect();

    let genesis_config = sibling::RuntimeGenesisConfig {
        system: sibling::SystemConfig {
            code: pendulum_runtime::WASM_BINARY
                .expect("WASM binary was not build, please build it!")
                .to_vec(),
            ..Default::default()
        },
        balances: sibling::BalancesConfig {
            balances: accounts::init_balances()
                .iter()
                .cloned()
                .map(|k| (k, units(100)))
                .collect(),
        },
        tokens: sibling::TokensConfig {
            balances: token_balances
        },
        parachain_info: sibling::ParachainInfoConfig {
            parachain_id: para_id.into(),
            ..Default::default()
        },
        session: sibling::SessionConfig {
            keys: collators::invulnerables()
                .into_iter()
                .map(|(acc, aura)| {
                    (
                        acc.clone(),                          // account id
                        acc,                                  // validator id
                        sibling::SessionKeys { aura }, // session keys
                    )
                })
                .collect(),
        },
        polkadot_xcm: sibling::PolkadotXcmConfig {
            safe_xcm_version: Some(SAFE_XCM_VERSION),
            ..Default::default()
        },
        ..Default::default()
    };

    genesis_config.build_storage().unwrap()
}