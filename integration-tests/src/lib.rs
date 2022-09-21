use sp_runtime::traits::AccountIdConversion;
use xcm_simulator::{decl_test_network, decl_test_parachain, decl_test_relay_chain};
use amplitude_runtime;
pub const ALICE: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([0u8; 32]);
pub const BOB: sp_runtime::AccountId32 = sp_runtime::AccountId32::new([1u8; 32]);
pub const INITIAL_BALANCE: u128 = 1_000_000_000;
use xcm_simulator::TestExternalities;
use polkadot_parachain::primitives::Id as ParaId;
use frame_support::traits::GenesisBuild;
use polkadot_runtime_parachains::configuration::HostConfiguration;
use polkadot_primitives::v2::{BlockNumber, MAX_CODE_SIZE, MAX_POV_SIZE};
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

	<parachain_info::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
		&parachain_info::GenesisConfig {
			parachain_id: ParaId::from(para_id),
		},
		&mut t,
	)
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

    polkadot_runtime_parachains::configuration::GenesisConfig::<Runtime> {
        config: default_parachains_host_configuration(),
    }
    .assimilate_storage(&mut t)
    .unwrap();

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

fn default_parachains_host_configuration() -> HostConfiguration<BlockNumber> {
    HostConfiguration {
        validation_upgrade_cooldown: 2u32,
        validation_upgrade_delay: 2,
        code_retention_period: 1200,
        max_code_size: MAX_CODE_SIZE,
        max_pov_size: MAX_POV_SIZE,
        max_head_data_size: 32 * 1024,
        group_rotation_frequency: 20,
        chain_availability_period: 4,
        thread_availability_period: 4,
        max_upward_queue_count: 8,
        max_upward_queue_size: 1024 * 1024,
        max_downward_message_size: 1024 * 1024,
        ump_service_total_weight: 100_000_000_000,
        max_upward_message_size: 50 * 1024,
        max_upward_message_num_per_candidate: 5,
        hrmp_sender_deposit: 0,
        hrmp_recipient_deposit: 0,
        hrmp_channel_max_capacity: 8,
        hrmp_channel_max_total_size: 8 * 1024,
        hrmp_max_parachain_inbound_channels: 4,
        hrmp_max_parathread_inbound_channels: 4,
        hrmp_channel_max_message_size: 1024 * 1024,
        hrmp_max_parachain_outbound_channels: 4,
        hrmp_max_parathread_outbound_channels: 4,
        hrmp_max_message_num_per_candidate: 5,
        dispute_period: 6,
        no_show_slots: 2,
        n_delay_tranches: 25,
        needed_approvals: 2,
        relay_vrf_modulo_samples: 2,
        zeroth_delay_tranche_width: 0,
        minimum_validation_upgrade_delay: 5,
        ..Default::default()
    }
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
		TestNet::reset();

		let remark = amplitude_runtime::Call::System(
			frame_system::Call::<amplitude_runtime::Runtime>::remark_with_event { remark: vec![1, 2, 3] },
		);
		KusamaNet::execute_with(|| {
			assert_ok!(RelayChainPalletXcm::send_xcm(
				Here,
				Parachain(3333),
				Xcm(vec![Transact {
					origin_type: OriginKind::SovereignAccount,
					require_weight_at_most: INITIAL_BALANCE as u64,
					call: remark.encode().into(),
				}]),
			));
		});

		ParaA::execute_with(|| {
			use amplitude_runtime::{Event, System};
			println!("{}", System::events().len());
			assert!(System::events().iter().any(|r| matches!(
				r.event,
				Event::System(frame_system::Event::Remarked { .. })
			)));
		});
	}

	#[test]
	fn reserve_transfer() {
		TestNet::reset();

		let withdraw_amount = 123;

		KusamaNet::execute_with(|| {

			println!("kusama balances {}", kusama_runtime::Balances::free_balance(&para_account_id(3333)));
			println!("kusama balances {}", kusama_runtime::Balances::free_balance(&para_account_id(1)));
			println!("Kusama Alice : {}", pallet_balances::Pallet::<kusama_runtime::Runtime>::free_balance(&ALICE));
			println!("Kusama Bob : {}", pallet_balances::Pallet::<kusama_runtime::Runtime>::free_balance(&BOB));

			assert_ok!(RelayChainPalletXcm::reserve_transfer_assets(
				kusama_runtime::Origin::signed(ALICE),
				Box::new(X1(Parachain(3333)).into().into()),
				Box::new(X1(AccountId32 { network: Any, id: ALICE.into() }).into().into()),
				Box::new((Here, withdraw_amount).into()),
				0,
			));

			println!("parachain balances {}", amplitude_runtime::Balances::free_balance(&para_account_id(3333)));
			println!("Alice : {}", pallet_balances::Pallet::<amplitude_runtime::Runtime>::free_balance(&ALICE));

			println!("kusama balances {}", kusama_runtime::Balances::free_balance(&para_account_id(3333)));
			println!("kusama balances {}", kusama_runtime::Balances::free_balance(&para_account_id(1)));
			println!("Kusama Alice : {}", pallet_balances::Pallet::<kusama_runtime::Runtime>::free_balance(&ALICE));
			println!("Kusama Bob : {}", pallet_balances::Pallet::<kusama_runtime::Runtime>::free_balance(&BOB));

			use kusama_runtime::{System};
			println!("events {}", System::events().len());
			for e in System::events(){
				println!("{:?}", e);
			}
			// assert_eq!(
			// 	amplitude_runtime::Balances::free_balance(&para_account_id(3333)),
			// 	INITIAL_BALANCE + withdraw_amount
			// );
		});

		ParaA::execute_with(|| {
			// free execution, full amount received
			println!("kusama balances {}", amplitude_runtime::Balances::free_balance(&para_account_id(3333)));
			println!("kusama balances {}", amplitude_runtime::Balances::free_balance(&para_account_id(1)));
			println!("Kusama Alice : {}", pallet_balances::Pallet::<amplitude_runtime::Runtime>::free_balance(&ALICE));
			println!("Kusama Alice : {}", pallet_balances::Pallet::<amplitude_runtime::Runtime>::free_balance(&BOB));

			

			use amplitude_runtime::{System};
			println!("events {}", System::events().len());
			for e in System::events(){
				println!("{:?}", e);
			}

			assert_eq!(
				pallet_balances::Pallet::<amplitude_runtime::Runtime>::free_balance(&ALICE),
				INITIAL_BALANCE + withdraw_amount
			);
		});
	}
}