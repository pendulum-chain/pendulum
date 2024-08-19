use crate::*;
// Substrate
use sp_core::{ storage::Storage};

// Cumulus
use integration_tests_common::{
    constants::{accounts, collators, validators, polkadot, asset_hub_polkadot},
};

use runtime_common::Balance;

pub const ED: Balance = runtime_common::EXISTENTIAL_DEPOSIT;
pub const SAFE_XCM_VERSION: u32 = 3;


use crate::mock::{assets_metadata_for_registry_pendulum, units};
pub use sp_runtime::{MultiAddress, Perbill, Permill, Perquintill};



#[macro_export]
macro_rules! genesis_gen {
	($runtime:ident, $para_account_id: ident, $asset_metadata: ident) => {{
		use $runtime::BuildStorage;
        use crate::mock::units;
        use integration_tests_common::constants::{collators, accounts};
        use spacewalk_primitives::{CurrencyId::XCM, CurrencyId};
        pub const SAFE_XCM_VERSION: u32 = 3;

        let token_balances = accounts::init_balances()
            .iter()
            .flat_map(|k| vec![(k.clone(), CurrencyId::XCM(0), units(1000))])
            .collect();

        let genesis_config = $runtime::RuntimeGenesisConfig {
            system: $runtime::SystemConfig {
                code: $runtime::WASM_BINARY
                    .expect("WASM binary was not build, please build it!")
                    .to_vec(),
                ..Default::default()
            },
            balances: $runtime::BalancesConfig {
                balances: accounts::init_balances()
                    .iter()
                    .cloned()
                    .map(|k| (k, units(1000)))
                    .collect(),
            },
            tokens: $runtime::TokensConfig {
                balances: token_balances
            },
            parachain_info: $runtime::ParachainInfoConfig {
                parachain_id: $para_account_id.into(),
                ..Default::default()
            },
            session: $runtime::SessionConfig {
                keys: collators::invulnerables()
                    .into_iter()
                    .map(|(acc, aura)| {
                        (
                            acc.clone(),                          // account id
                            acc,                                  // validator id
                            $runtime::SessionKeys { aura }, // session keys
                        )
                    })
                    .collect(),
            },
            polkadot_xcm: $runtime::PolkadotXcmConfig {
                safe_xcm_version: Some(SAFE_XCM_VERSION),
                ..Default::default()
            },
            asset_registry: $runtime::AssetRegistryConfig {
                assets: $asset_metadata(),
                last_asset_id: CurrencyId::Native,
            },
            ..Default::default()
        };

        genesis_config.build_storage().unwrap()
	}};
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

pub(super) use crate::genesis_gen;