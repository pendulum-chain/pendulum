use sp_runtime::traits::AccountIdConversion;
use xcm_simulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain};
use amplitude_runtime;
pub const ALICE: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([0u8; 32]);
pub const INITIAL_BALANCE: u128 = 1_000_000_000;
use xcm_simulator::TestExternalities;
use polkadot_parachain::primitives::Id as ParaId;
use frame_support::traits::GenesisBuild;
// use primitives::AccountId;

decl_test_parachain! {
	pub struct ParaA {
		Runtime = amplitude_runtime::Runtime,
		XcmpMessageHandler = amplitude_runtime::XcmpQueue,
		DmpMessageHandler = amplitude_runtime::DmpQueue,
		new_ext = para_ext(3333),
	}
}

decl_test_relay_chain! {
    pub struct KusamaNet {
        Runtime = kusama_runtime::Runtime,
        XcmConfig = kusama_runtime::xcm_config::XcmConfig,
        new_ext = kusama_ext(),
    }
}

decl_test_network! {
    pub struct TestNet {
        relay_chain = KusamaNet,
        parachains = vec![
            (3333, ParaA),
        ],
    }
}



pub fn para_ext(para_id: u32) -> TestExternalities {
	use amplitude_runtime::{XcmpQueue, Runtime, System};
	

	let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();

	pallet_balances::GenesisConfig::<Runtime> { balances: vec![(ALICE, INITIAL_BALANCE)] }
		.assimilate_storage(&mut t)
		.unwrap();

	let mut ext = TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}


pub fn kusama_ext() -> TestExternalities {
    // let _ = env_logger::try_init();

    use kusama_runtime::{Runtime, System};

    let mut t = frame_system::GenesisConfig::default()
        .build_storage::<Runtime>()
        .unwrap();

    pallet_balances::GenesisConfig::<Runtime> {
        balances: vec![
			(ALICE, ksm(100f64)),
            // (AccountId::from(ALICE), ),
            (
                ParaId::from(3333 as u32).into_account_truncating(),
                ksm(100f64),
            ),
        ],
    }
    .assimilate_storage(&mut t)
    .unwrap();

	//TODO
    // polkadot_runtime_parachains::configuration::GenesisConfig::<Runtime> {
    //     config: default_parachains_host_configuration(),
    // }
    // .assimilate_storage(&mut t)
    // .unwrap();

    <pallet_xcm::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
        &pallet_xcm::GenesisConfig {
            safe_xcm_version: Some(2),
        },
        &mut t,
    )
    .unwrap();

    let mut ext = TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}

pub const KSM_DECIMAL: u32 = 12;
pub fn ksm(n: f64) -> u128 {
    (n as u128) * 10u128.pow(KSM_DECIMAL)
}

pub fn para_account_id(id: u32) -> sp_runtime::AccountId32 {
	ParaId::from(id).into_account_truncating()
}

pub type RelayChainPalletXcm = pallet_xcm::Pallet<kusama_runtime::Runtime>;
pub type ParachainPalletXcm = pallet_xcm::Pallet<amplitude_runtime::Runtime>;

#[cfg(test)]
mod tests {
	use super::*;

	use codec::Encode;
	use frame_support::assert_ok;
	use xcm::latest::prelude::*;
	use xcm_simulator::TestExt;

	// Helper function for forming buy execution message
	fn buy_execution<C>(fees: impl Into<MultiAsset>) -> Instruction<C> {
		BuyExecution { fees: fees.into(), weight_limit: Unlimited }
	}

	#[test]
	fn dmp() {
		MockNet::reset();

		let remark = amplitude_runtime::RuntimeCall::System(
			frame_system::Call::<amplitude_runtime::Runtime>::remark_with_event { remark: vec![1, 2, 3] },
		);
		KusamaNet::execute_with(|| {
			assert_ok!(RelayChainPalletXcm::send_xcm(
				Here,
				Parachain(1),
				Xcm(vec![Transact {
					origin_type: OriginKind::SovereignAccount,
					require_weight_at_most: INITIAL_BALANCE as u64,
					call: remark.encode().into(),
				}]),
			));
		});

		ParaA::execute_with(|| {
			use amplitude_runtime::{RuntimeEvent, System};
			assert!(System::events().iter().any(|r| matches!(
				r.event,
				RuntimeEvent::System(frame_system::Event::Remarked { .. })
			)));
		});
	}
}