use frame_support::{
	assert_ok,
	traits::{fungible::Mutate, fungibles::Inspect, Currency, GenesisBuild},
};
use pendulum_runtime::{
	Balances, PendulumCurrencyId, Runtime, RuntimeOrigin, System, Tokens, XTokens,
};
use polkadot_core_primitives::{AccountId, Balance, BlockNumber};
use polkadot_parachain::primitives::{Id as ParaId, Sibling};
use polkadot_primitives::v2::{MAX_CODE_SIZE, MAX_POV_SIZE};
use polkadot_runtime_parachains::configuration::HostConfiguration;
use sp_runtime::{traits::AccountIdConversion, MultiAddress};
use xcm::{
	latest::{
		AssetId, Fungibility, Junction, Junction::*, Junctions::*, MultiAsset, MultiLocation,
		NetworkId, WeightLimit,
	},
	v2::{Instruction::WithdrawAsset, Xcm},
	VersionedMultiLocation,
};
const DOT_FEE: Balance = 3200000000;
const ASSET_ID: u32 = 1984; //Real USDT Asset ID from Statemint
const INCORRECT_ASSET_ID: u32 = 0;
pub const UNIT: Balance = 1_000_000_000_000;
pub const TEN: Balance = 10_000_000_000_000;
use xcm_emulator::{
	decl_test_network, decl_test_parachain, decl_test_relay_chain, Junctions, TestExt, Weight,
};

mod setup;
use setup::*;

pub const ALICE: [u8; 32] = [4u8; 32];
pub const BOB: [u8; 32] = [5u8; 32];
pub const INITIAL_BALANCE: u128 = 1_000_000_000;

pub fn dot(amount: Balance) -> Balance {
	amount * 10u128.saturating_pow(9)
}

decl_test_relay_chain! {
	pub struct Relay {
		Runtime = polkadot_runtime::Runtime,
		XcmConfig = polkadot_runtime::xcm_config::XcmConfig,
		new_ext = relay_ext(),
	}
}

decl_test_parachain! {
	pub struct PendulumParachain {
		Runtime = pendulum_runtime::Runtime,
		RuntimeOrigin = pendulum_runtime::RuntimeOrigin,
		XcmpMessageHandler = pendulum_runtime::XcmpQueue,
		DmpMessageHandler = pendulum_runtime::DmpQueue,
		new_ext = para_ext_pendulum(2094),
	}
}

decl_test_parachain! {
	pub struct Statemint {
		Runtime = statemint_runtime::Runtime,
		RuntimeOrigin = statemint_runtime::RuntimeOrigin,
		XcmpMessageHandler = statemint_runtime::XcmpQueue,
		DmpMessageHandler = statemint_runtime::DmpQueue,
		new_ext = para_ext_statemint(1000),
	}
}

decl_test_network! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(1000, Statemint),
			(2094, PendulumParachain),
		],
	}
}

pub fn para_account_id(id: u32) -> polkadot_core_primitives::AccountId {
	ParaId::from(id).into_account_truncating()
}

pub fn relay_ext() -> sp_io::TestExternalities {
	use polkadot_runtime::{Runtime, System};
	let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();
	pallet_balances::GenesisConfig::<Runtime> {
		balances: vec![
			(AccountId::from(ALICE), dot(100000)),
			(AccountId::from(BOB), dot(100)),
			(para_account_id(2094), 10 * dot(100000)),
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
		&pallet_xcm::GenesisConfig { safe_xcm_version: Some(2) },
		&mut t,
	)
	.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

pub fn para_ext_pendulum(parachain_id: u32) -> sp_io::TestExternalities {
	ExtBuilderPendulum::default()
		.balances(vec![])
		.parachain_id(parachain_id)
		.build()
}

pub fn para_ext_statemint(parachain_id: u32) -> sp_io::TestExternalities {
	ExtStatemintBuilder::default()
		.balances(vec![])
		.parachain_id(parachain_id)
		.build()
}

fn default_parachains_host_configuration() -> HostConfiguration<BlockNumber> {
	HostConfiguration {
		minimum_validation_upgrade_delay: 5,
		validation_upgrade_cooldown: 5u32,
		validation_upgrade_delay: 5,
		code_retention_period: 1200,
		max_code_size: MAX_CODE_SIZE,
		max_pov_size: MAX_POV_SIZE,
		max_head_data_size: 32 * 1024,
		group_rotation_frequency: 20,
		chain_availability_period: 4,
		thread_availability_period: 4,
		max_upward_queue_count: 8,
		max_upward_queue_size: 1024 * 1024,
		max_downward_message_size: 1024,
		ump_service_total_weight: Weight::from_ref_time(4 * 1_000_000_000),
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
		..Default::default()
	}
}

#[test]
fn transfer_polkadot_from_relay_chain_to_pendulum() {
	MockNet::reset();

	let transfer_amount: Balance = dot(20);
	let mut orml_tokens_before = 0;
	PendulumParachain::execute_with(|| {
		let orml_tokens_before = pendulum_runtime::Tokens::balance(
			pendulum_runtime::PendulumCurrencyId::XCM(0),
			&ALICE.into(),
		);
	});

	Relay::execute_with(|| {
		assert_ok!(polkadot_runtime::XcmPallet::reserve_transfer_assets(
			polkadot_runtime::RuntimeOrigin::signed(ALICE.into()),
			Box::new(Parachain(2094).into().into()),
			Box::new(Junction::AccountId32 { network: NetworkId::Any, id: ALICE }.into().into()),
			Box::new((Here, transfer_amount).into()),
			0
		));
	});

	PendulumParachain::execute_with(|| {
		use pendulum_runtime::{RuntimeEvent, System, Tokens};

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Tokens(orml_tokens::Event::Deposited { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::DmpQueue(cumulus_pallet_dmp_queue::Event::ExecutedDownward { .. })
		)));
	});

	PendulumParachain::execute_with(|| {
		assert_eq!(
			pendulum_runtime::Tokens::balance(
				pendulum_runtime::PendulumCurrencyId::XCM(0),
				&ALICE.into()
			),
			orml_tokens_before + transfer_amount - DOT_FEE
		);
	});
}

#[test]
fn transfer_polkadot_from_pendulum_to_relay_chain() {
	MockNet::reset();

	let transfer_dot_amount: Balance = dot(10);
	let FEE = 421434140;

	Relay::execute_with(|| {
		let after_bob_free_balance = polkadot_runtime::Balances::free_balance(&BOB.into());
		// println!("BOB DOT BEFORE balance on relay chain {} ", after_bob_free_balance);
		assert_eq!(after_bob_free_balance, dot(100));
	});

	PendulumParachain::execute_with(|| {
		assert_ok!(pendulum_runtime::XTokens::transfer(
			pendulum_runtime::RuntimeOrigin::signed(BOB.into()),
			pendulum_runtime::PendulumCurrencyId::XCM(0),
			transfer_dot_amount,
			Box::new(
				MultiLocation::new(
					1,
					Junctions::X1(Junction::AccountId32 { id: BOB, network: NetworkId::Any })
				)
				.into()
			),
			WeightLimit::Limited(4_000_000_000),
		));
	});

	PendulumParachain::execute_with(|| {
		use pendulum_runtime::{RuntimeEvent, System};
		// for i in System::events().iter() {
		// 	println!(" Pendulum_runtime {:?}", i);
		// }

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Tokens(orml_tokens::Event::Withdrawn { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XTokens(orml_xtokens::Event::TransferredMultiAssets { .. })
		)));
	});

	Relay::execute_with(|| {
		use polkadot_runtime::{RuntimeEvent, System};

		// for i in System::events().iter() {
		// 	println!("polkadot_runtime {:?}", i);
		// }

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Balances(pallet_balances::Event::Withdraw { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Balances(pallet_balances::Event::Deposit { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Ump(polkadot_runtime_parachains::ump::Event::ExecutedUpward { .. })
		)));
	});

	Relay::execute_with(|| {
		let after_bob_free_balance = polkadot_runtime::Balances::free_balance(&BOB.into());
		assert_eq!(after_bob_free_balance, dot(100) + transfer_dot_amount - FEE);
	});
}

#[test]
fn statemint_transfer_incorrect_asset_to_pendulum_fails() {
	let para_2094: AccountId = Sibling::from(2094).into_account_truncating();

	PendulumParachain::execute_with(|| {
		assert_eq!(
			pendulum_runtime::Tokens::balance(
				pendulum_runtime::PendulumCurrencyId::XCM(1),
				&BOB.into()
			),
			0
		);
	});

	Statemint::execute_with(|| {
		use statemint_runtime::*;

		let origin = RuntimeOrigin::signed(ALICE.into());
		Balances::make_free_balance_be(&ALICE.into(), TEN);
		Balances::make_free_balance_be(&BOB.into(), UNIT);

		// If using non root, create custom asset cost 0.1 Dot
		// We're using force_create here to make sure asset is sufficient.
		assert_ok!(Assets::force_create(
			RuntimeOrigin::root(),
			INCORRECT_ASSET_ID.into(),
			MultiAddress::Id(ALICE.into()),
			true,
			UNIT / 100
		));

		assert_ok!(Assets::mint(
			origin.clone(),
			INCORRECT_ASSET_ID.into(),
			MultiAddress::Id(ALICE.into()),
			1000 * UNIT
		));

		// need to have some DOT to be able to receive user assets
		Balances::make_free_balance_be(&para_2094, UNIT);

		assert_ok!(PolkadotXcm::limited_reserve_transfer_assets(
			origin.clone(),
			Box::new(MultiLocation::new(1, X1(Parachain(2094))).into()),
			Box::new(Junction::AccountId32 { id: BOB, network: NetworkId::Any }.into().into()),
			Box::new(
				(X2(PalletInstance(50), GeneralIndex(INCORRECT_ASSET_ID as u128)), TEN).into()
			),
			0,
			WeightLimit::Unlimited
		));

		assert_eq!(990 * UNIT, Assets::balance(INCORRECT_ASSET_ID, &AccountId::from(ALICE)));
		assert_eq!(0, Assets::balance(INCORRECT_ASSET_ID, &AccountId::from(BOB)));

		assert_eq!(TEN, Assets::balance(INCORRECT_ASSET_ID, &para_2094));
		// the DOT balance of sibling parachain sovereign account is not changed
		assert_eq!(UNIT, Balances::free_balance(&para_2094));
	});

	// Rerun the Statemint::execute to actually send the egress message via XCM
	Statemint::execute_with(|| {});

	PendulumParachain::execute_with(|| {
		use pendulum_runtime::{RuntimeEvent, System};

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Fail {
				message_hash: _,
				error: xcm::v2::Error::FailedToTransactAsset(..),
				weight: _
			})
		)));
	});
}

#[test]
fn statemint_transfer_asset_to_pendulum() {
	let para_2094: AccountId = Sibling::from(2094).into_account_truncating();

	PendulumParachain::execute_with(|| {
		assert_eq!(
			pendulum_runtime::Tokens::balance(
				pendulum_runtime::PendulumCurrencyId::XCM(1),
				&BOB.into()
			),
			0
		);
	});

	Statemint::execute_with(|| {
		use statemint_runtime::*;

		let origin = RuntimeOrigin::signed(ALICE.into());
		Balances::make_free_balance_be(&ALICE.into(), TEN);
		Balances::make_free_balance_be(&BOB.into(), UNIT);

		// If using non root, create custom asset cost 0.1 Dot
		// We're using force_create here to make sure asset is sufficient.
		assert_ok!(Assets::force_create(
			RuntimeOrigin::root(),
			ASSET_ID.into(),
			MultiAddress::Id(ALICE.into()),
			true,
			UNIT / 100
		));

		assert_ok!(Assets::mint(
			origin.clone(),
			ASSET_ID.into(),
			MultiAddress::Id(ALICE.into()),
			1000 * UNIT
		));

		// need to have some DOT to be able to receive user assets
		Balances::make_free_balance_be(&para_2094, UNIT);

		assert_ok!(PolkadotXcm::limited_reserve_transfer_assets(
			origin.clone(),
			Box::new(MultiLocation::new(1, X1(Parachain(2094))).into()),
			Box::new(Junction::AccountId32 { id: BOB, network: NetworkId::Any }.into().into()),
			Box::new((X2(PalletInstance(50), GeneralIndex(ASSET_ID as u128)), TEN).into()),
			0,
			WeightLimit::Unlimited
		));

		assert_eq!(990 * UNIT, Assets::balance(ASSET_ID, &AccountId::from(ALICE)));
		assert_eq!(0, Assets::balance(ASSET_ID, &AccountId::from(BOB)));

		assert_eq!(TEN, Assets::balance(ASSET_ID, &para_2094));
		// the DOT balance of sibling parachain sovereign account is not changed
		assert_eq!(UNIT, Balances::free_balance(&para_2094));
	});

	// Rerun the Statemint::execute to actually send the egress message via XCM
	Statemint::execute_with(|| {});

	PendulumParachain::execute_with(|| {
		use pendulum_runtime::{RuntimeEvent, System};
		for i in System::events().iter() {
			println!(" Pendulum_runtime {:?}", i);
		}

		assert!(System::events()
			.iter()
			.any(|r| matches!(r.event, RuntimeEvent::Tokens(orml_tokens::Event::Endowed { .. }))));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Tokens(orml_tokens::Event::Deposited { .. })
		)));

		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::Success { .. })
		)));

		assert_eq!(
			pendulum_runtime::Tokens::balance(
				pendulum_runtime::PendulumCurrencyId::XCM(1),
				&BOB.into()
			),
			TEN
		);
	});
}

#[test]
fn statemint_transfer_asset_to_statemint() {
	statemint_transfer_asset_to_pendulum();

	Statemint::execute_with(|| {});

	PendulumParachain::execute_with(|| {
		assert_eq!(TEN, Tokens::balance(PendulumCurrencyId::XCM(1), &AccountId::from(BOB)));
		// ensure sender has enough PEN balance to be charged as fee
		assert_ok!(Balances::mint_into(&AccountId::from(BOB), TEN));

		assert_ok!(XTokens::transfer(
			RuntimeOrigin::signed(BOB.into()),
			PendulumCurrencyId::XCM(1),
			UNIT * 1,
			Box::new(
				MultiLocation::new(
					1,
					X2(
						Parachain(1000),
						Junction::AccountId32 { network: NetworkId::Any, id: BOB.into() }
					)
				)
				.into()
			),
			WeightLimit::Limited(10_000_000_000),
		));

		assert_eq!(
			TEN - 1 * UNIT, //inital balance - one unit
			Tokens::balance(PendulumCurrencyId::XCM(1), &AccountId::from(BOB))
		);

		// for i in System::events().iter() {
		// 	println!(" Pendulum_runtime {:?}", i);
		// }

		use pendulum_runtime::{RuntimeEvent, System};
		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. })
		)));

		// assert_eq!(TEN - ksm_fee_amount, Tokens::free_balance(KSM, &AccountId::from(BOB)));
	});

	Statemint::execute_with(|| {
		use statemint_runtime::*;

		// https://github.com/paritytech/cumulus/pull/1278 support using self sufficient asset
		// for paying xcm execution fee on Statemint.
		assert_eq!(990_000_000_000, Assets::balance(ASSET_ID, &AccountId::from(BOB)));
	});
}
