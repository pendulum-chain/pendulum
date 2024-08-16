use crate::*;
// Substrate
use sp_core::{sr25519, storage::Storage};

// Polkadot
// Cumulus
use integration_tests_common::{
    constants::{accounts, collators},
};

use runtime_common::Balance;

pub const PARA_ID_A: u32 = 2000;
pub const PARA_ID_B: u32 = 2001;
pub const ED: Balance = runtime_common::EXISTENTIAL_DEPOSIT;


pub const SAFE_XCM_VERSION: u32 = 3;

use spacewalk_primitives::{CurrencyId::XCM, CurrencyId};
use crate::mock::{assets_metadata_for_registry_pendulum,units};

pub fn genesis(para_id: u32) -> Storage {
    use pendulum_runtime::BuildStorage;

    let token_balances = accounts::init_balances()
        .iter()
        .flat_map(|k| vec![(k.clone(), XCM(0), units(100))])
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
                .map(|k| (k, units(100)))
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