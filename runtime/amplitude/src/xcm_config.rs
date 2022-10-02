use super::{
	AccountId, CurrencyId, Balance, Balances, Tokens, Currencies, Call, Event, Origin, ParachainInfo, ParachainSystem, PolkadotXcm, Runtime,
	WeightToFee, XcmpQueue,
};
use core::marker::PhantomData;
use frame_support::{
	log, match_types, parameter_types,
	traits::{Everything, Nothing},
	weights::Weight,
};
use orml_traits::{
    location::AbsoluteReserveProvider, parameter_type_with_key, DataFeeder, DataProvider,
    DataProviderExtended,
};
use sp_runtime::traits::{Convert};
use pallet_xcm::XcmPassthrough;
use polkadot_parachain::primitives::Sibling;
use polkadot_runtime_common::impls::ToAuthor;
use xcm::latest::prelude::*;
use xcm_builder::{
	ConvertedConcreteAssetId, FungiblesAdapter, AccountId32Aliases, AllowTopLevelPaidExecutionFrom, AllowUnpaidExecutionFrom, CurrencyAdapter,
	EnsureXcmOrigin, FixedWeightBounds, IsConcrete, LocationInverter, NativeAsset, ParentIsPreset,
	RelayChainAsNative, SiblingParachainAsNative, SiblingParachainConvertsVia,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit,
	UsingComponents,
};
use xcm_executor::{traits::ShouldExecute, XcmExecutor, traits::JustTry};

parameter_types! {
	pub const RelayLocation: MultiLocation = MultiLocation::parent();
	pub const RelayNetwork: NetworkId = NetworkId::Any;
	pub RelayChainOrigin: Origin = cumulus_pallet_xcm::Origin::Relay.into();
	pub Ancestry: MultiLocation = Parachain(ParachainInfo::parachain_id().into()).into();
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
}

/// Type for specifying how a `MultiLocation` can be converted into an `AccountId`. This is used
/// when determining ownership of accounts for asset transacting and when attempting to use XCM
/// `Transact` in order to determine the dispatch Origin.
pub type LocationToAccountId = (
	// The parent (Relay-chain) origin converts to the parent `AccountId`.
	ParentIsPreset<AccountId>,
	// Sibling parachain origins convert to AccountId via the `ParaId::into`.
	SiblingParachainConvertsVia<Sibling, AccountId>,
	// Straight up local `AccountId32` origins just alias directly to `AccountId`.
	AccountId32Aliases<RelayNetwork, AccountId>,
);

/// Means for transacting assets on this chain.
// pub type LocalAssetTransactor = CurrencyAdapter<
// 	// Use this currency:
// 	Tokens,
// 	// Use this currency when it is a fungible asset matching the given location or name:
// 	IsConcrete<RelayLocation>,
// 	// Do a simple punn to convert an AccountId32 MultiLocation into a native chain account ID:
// 	LocationToAccountId,
// 	// Our chain's account ID type (we can't get away without mentioning it explicitly):
// 	AccountId,
// 	// We don't track any teleports.
// 	(),
// >;


/// CurrencyIdConvert
/// This type implements conversions from our `CurrencyId` type into `MultiLocation` and vice-versa.
/// A currency locally is identified with a `CurrencyId` variant but in the network it is identified
/// in the form of a `MultiLocation`, in this case a pCfg (Para-Id, Currency-Id).
pub struct CurrencyIdConvert;

impl Convert<CurrencyId, Option<MultiLocation>> for CurrencyIdConvert {
    fn convert(id: CurrencyId) -> Option<MultiLocation> {
		match id{
			CurrencyId::KSM => Some(MultiLocation::parent()),
			_ => None,
		}
	}
}

impl Convert<MultiLocation, Option<CurrencyId>> for CurrencyIdConvert {
    fn convert(location: MultiLocation) -> Option<CurrencyId> {
        match location {
			MultiLocation {
                parents: 1,
                interior: Here,
            } => Some(CurrencyId::KSM),
			_ => None,
		}
	}
}

impl Convert<MultiAsset, Option<CurrencyId>> for CurrencyIdConvert {
    fn convert(a: MultiAsset) -> Option<CurrencyId> {
        if let MultiAsset {
            id: AssetId::Concrete(id),
            fun: _,
        } = a
        {
            Self::convert(id)
        } else {
            None
        }
    }
}

/// Convert an incoming `MultiLocation` into a `CurrencyId` if possible.
/// Here we need to know the canonical representation of all the tokens we handle in order to
/// correctly convert their `MultiLocation` representation into our internal `CurrencyId` type.
impl xcm_executor::traits::Convert<MultiLocation, CurrencyId> for CurrencyIdConvert {
	fn convert(location: MultiLocation) -> Result<CurrencyId, MultiLocation> {
		if location == MultiLocation::parent() {
			return Ok(CurrencyId::KSM);
		}
		Err(location.clone())
	}
}

/// Means for transacting the fungibles assets of ths parachain.
pub type FungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation
	Tokens,
	// This means that this adapter should handle any token that `CurrencyIdConvert` can convert
	// to `CurrencyId`, the `CurrencyId` type of `Tokens`, the fungibles implementation it uses.
	ConvertedConcreteAssetId<CurrencyId, Balance, CurrencyIdConvert, JustTry>,
	// Convert an XCM MultiLocation into a local account id
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly)
	AccountId,
	// We dont allow teleports.
	Nothing,
	// The account to use for tracking teleports.
	CheckingAccount,
>;

/// This is the type we use to convert an (incoming) XCM origin into a local `Origin` instance,
/// ready for dispatching a transaction with Xcm's `Transact`. There is an `OriginKind` which can
/// biases the kind of local `Origin` it will become.
pub type XcmOriginToTransactDispatchOrigin = (
	// Sovereign account converter; this attempts to derive an `AccountId` from the origin location
	// using `LocationToAccountId` and then turn that into the usual `Signed` origin. Useful for
	// foreign chains who want to have a local sovereign account on this chain which they control.
	SovereignSignedViaLocation<LocationToAccountId, Origin>,
	// Native converter for Relay-chain (Parent) location; will converts to a `Relay` origin when
	// recognized.
	RelayChainAsNative<RelayChainOrigin, Origin>,
	// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
	// recognized.
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, Origin>,
	// Native signed account converter; this just converts an `AccountId32` origin into a normal
	// `Origin::Signed` origin of the same 32-byte value.
	SignedAccountId32AsNative<RelayNetwork, Origin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<Origin>,
);

parameter_types! {
	// One XCM operation is 1_000_000_000 weight - almost certainly a conservative estimate.
	pub UnitWeightCost: Weight = 1_000_000_000;
	pub const MaxInstructions: u32 = 100;
	pub SelfLocation: MultiLocation = MultiLocation::new(1, X1(Parachain(ParachainInfo::parachain_id().into())));
	pub const BaseXcmWeight: Weight = 150_000_000;
	pub const MaxAssetsForTransfer: usize = 2;
}

match_types! {
	pub type ParentOrParentsExecutivePlurality: impl Contains<MultiLocation> = {
		MultiLocation { parents: 1, interior: Here } |
		MultiLocation { parents: 1, interior: X1(Plurality { id: BodyId::Executive, .. }) }
	};
}

//TODO: move DenyThenTry to polkadot's xcm module.
/// Deny executing the xcm message if it matches any of the Deny filter regardless of anything else.
/// If it passes the Deny, and matches one of the Allow cases then it is let through.
pub struct DenyThenTry<Deny, Allow>(PhantomData<Deny>, PhantomData<Allow>)
where
	Deny: ShouldExecute,
	Allow: ShouldExecute;

impl<Deny, Allow> ShouldExecute for DenyThenTry<Deny, Allow>
where
	Deny: ShouldExecute,
	Allow: ShouldExecute,
{
	fn should_execute<Call>(
		origin: &MultiLocation,
		message: &mut Xcm<Call>,
		max_weight: Weight,
		weight_credit: &mut Weight,
	) -> Result<(), ()> {
		Deny::should_execute(origin, message, max_weight, weight_credit)?;
		Allow::should_execute(origin, message, max_weight, weight_credit)
	}
}

// See issue #5233
pub struct DenyReserveTransferToRelayChain;
impl ShouldExecute for DenyReserveTransferToRelayChain {
	fn should_execute<Call>(
		origin: &MultiLocation,
		message: &mut Xcm<Call>,
		_max_weight: Weight,
		_weight_credit: &mut Weight,
	) -> Result<(), ()> {
		if message.0.iter().any(|inst| {
			matches!(
				inst,
				InitiateReserveWithdraw {
					reserve: MultiLocation { parents: 1, interior: Here },
					..
				} | DepositReserveAsset { dest: MultiLocation { parents: 1, interior: Here }, .. } |
					TransferReserveAsset {
						dest: MultiLocation { parents: 1, interior: Here },
						..
					}
			)
		}) {
			return Err(()) // Deny
		}

		// allow reserve transfers to arrive from relay chain
		if matches!(origin, MultiLocation { parents: 1, interior: Here }) &&
			message.0.iter().any(|inst| matches!(inst, ReserveAssetDeposited { .. }))
		{
			log::warn!(
				target: "xcm::barriers",
				"Unexpected ReserveAssetDeposited from the relay chain",
			);
		}
		// Permit everything else
		Ok(())
	}
}

// pub type Barrier = DenyThenTry<
// 	DenyReserveTransferToRelayChain,
// 	(
// 		TakeWeightCredit,
// 		AllowTopLevelPaidExecutionFrom<Everything>,
// 		AllowUnpaidExecutionFrom<ParentOrParentsExecutivePlurality>,
// 		// ^^^ Parent and its exec plurality get free execution
// 	),
// >;

pub type Barrier = AllowUnpaidExecutionFrom<Everything>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type Call = Call;
	type XcmSender = XcmRouter;
	// How to withdraw and deposit an asset.
	type AssetTransactor = FungiblesTransactor;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	type IsReserve = NativeAsset;
	type IsTeleporter = (); // Teleporting is disabled.
	type LocationInverter = LocationInverter<Ancestry>; //TODO check Ancestry???
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<UnitWeightCost, Call, MaxInstructions>;
	type Trader =
		UsingComponents<WeightToFee, RelayLocation, AccountId, Balances, ToAuthor<Runtime>>;
	type ResponseHandler = PolkadotXcm;
	type AssetTrap = PolkadotXcm;
	type AssetClaims = PolkadotXcm;
	type SubscriptionService = PolkadotXcm;
}

/// No local origins on this chain are allowed to dispatch XCM sends/executions.
pub type LocalOriginToLocation = SignedToAccountId32<Origin, AccountId, RelayNetwork>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = (
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
);

impl pallet_xcm::Config for Runtime {
	type Event = Event;
	type SendXcmOrigin = EnsureXcmOrigin<Origin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = EnsureXcmOrigin<Origin, LocalOriginToLocation>;
	type XcmExecuteFilter = Everything;
	// ^ Disable dispatchable execute on the XCM pallet.
	// Needs to be `Everything` for local testing.
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Nothing;
	type XcmReserveTransferFilter = Everything;
	type Weigher = FixedWeightBounds<UnitWeightCost, Call, MaxInstructions>;
	type LocationInverter = LocationInverter<Ancestry>;
	type Origin = Origin;
	type Call = Call;

	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	// ^ Override for AdvertisedXcmVersion default
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
}

parameter_type_with_key! {
	pub ParachainMinFee: |_location: MultiLocation| -> Option<u128> {
		None
	};
}

// Min fee required when transferring asset back to reserve sibling chain
// which use another asset(e.g Relaychain's asset) as fee
// parameter_type_with_key! {
//     pub ParachainMinFee: |location: MultiLocation| -> Option<u128> {
//         #[allow(clippy::match_ref_pats)] // false positive
//         match (location.parents, location.first_interior()) {
//             (1, Some(Parachain(paras::statemint::ID))) => Some(XcmHelper::get_xcm_weight_fee_to_sibling(location.clone()).fee),//default fee should be enough even if not configured
//             _ => None,
//         }
//     };
// }


impl orml_xtokens::Config for Runtime {
	type Event = Event;
	type Balance = Balance;
	type CurrencyId = CurrencyId;
	type CurrencyIdConvert = CurrencyIdConvert;
	type AccountIdToMultiLocation = AccountIdToMultiLocation;
	type SelfLocation = SelfLocation;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type Weigher = FixedWeightBounds<UnitWeightCost, Call, MaxInstructions>;
	type BaseXcmWeight = BaseXcmWeight;
	type LocationInverter = LocationInverter<Ancestry>;
	type MaxAssetsForTransfer = MaxAssetsForTransfer;
	type MinXcmFee = ParachainMinFee; //TODO to support hrmp transfer beetween parachain adjust this parameter
	type MultiLocationsFilter = Everything;
	type ReserveProvider = AbsoluteReserveProvider;
}

pub struct AccountIdToMultiLocation;
impl Convert<AccountId, MultiLocation> for AccountIdToMultiLocation {
	// fn convert(account: AccountId) -> MultiLocation {
	// 	X1(AccountId32 {
	// 		network: NetworkId::Any,
	// 		id: account.into(),
	// 	})
	// 	.into()
	// }
	fn convert(account: AccountId) -> MultiLocation {
        MultiLocation {
            parents: 0,
            interior: X1(AccountId32 {
                network: NetworkId::Any,
                id: account.into(),
            }),
        }
    }
}


// impl orml_xtokens::Config for Runtime {
//     type Event = Event;
//     type Balance = Balance;
//     type CurrencyId = CurrencyId;
//     type CurrencyIdConvert = CurrencyIdtoMultiLocation<
//         CurrencyIdConvert,
//         AsAssetType<CurrencyId, AssetType, AssetRegistry>,
//     >;
//     type AccountIdToMultiLocation = AccountIdToMultiLocation<AccountId>;
//     type SelfLocation = SelfLocation;
//     type XcmExecutor = XcmExecutor<XcmConfig>;
//     type Weigher = FixedWeightBounds<BaseXcmWeight, Call, MaxInstructions>;
//     type BaseXcmWeight = BaseXcmWeight;
//     type LocationInverter = LocationInverter<Ancestry>;
//     type MaxAssetsForTransfer = MaxAssetsForTransfer;
//     type MinXcmFee = ParachainMinFee;
//     type MultiLocationsFilter = Everything;
//     type ReserveProvider = AbsoluteReserveProvider;
// }



impl cumulus_pallet_xcm::Config for Runtime {
	type Event = Event;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}


/*
use crate::xcm::kusama::test_net::Amplitude;
#[test]
fn transfer_ksm_from_relay_chain_to_amplitude() {
	let transfer_amount: Balance = ksm(2);
	println!("transfer KSM amount : {} ", transfer_amount);
	let mut balance_before = 0;
	let mut orml_tokens_before = 0;
	Amplitude::execute_with(|| {
		balance_before = Balances::free_balance(&ALICE.into());
		println!("Alice balance_before {}", balance_before);
		let orml_tokens_before = amplitude_runtime::Tokens::free_balance(amplitude_runtime::CurrencyId::KSM, &ALICE.into());
		println!("Alice orml tokens KSM before {}", orml_tokens_before);
	});

	KusamaNet::execute_with(|| {
		assert_ok!(kusama_runtime::XcmPallet::reserve_transfer_assets(
			kusama_runtime::Origin::signed(ALICE.into()),
			Box::new(Parachain(1234).into().into()),
			Box::new(
				Junction::AccountId32 {
					network: NetworkId::Any,
					id: ALICE,
				}
				.into()
				.into()
			),
			Box::new((Here, transfer_amount).into()),
			0
		));
	});

	
	Amplitude::execute_with(|| {
		assert_eq!(
			amplitude_runtime::Tokens::free_balance(amplitude_runtime::CurrencyId::KSM, &ALICE.into()),
			orml_tokens_before + transfer_amount - KSM_FEE
		);
	});

	//old version. when transfer works to native currency. not to orml tokens
	// Amplitude::execute_with(|| {
	// 	assert_eq!(
	// 		Balances::free_balance(&ALICE.into()),
	// 		balance_before + transfer_amount - ksm_fee()
	// 	);
	// });

	KusamaNet::execute_with(|| {
		let before_bob_free_balance = kusama_runtime::Balances::free_balance(&BOB.into());
		println!("BOB KSM BEFORE balance on relay chain {} ", before_bob_free_balance);
		assert_eq!(
			before_bob_free_balance,
			0
		);
	});

	Amplitude::execute_with(|| {
		assert_ok!(amplitude_runtime::XTokens::transfer(
			amplitude_runtime::Origin::signed(ALICE.into()),
			amplitude_runtime::CurrencyId::KSM,
			ksm(1),
			Box::new(
				MultiLocation::new(
					1,
					X1(Junction::AccountId32 {
						id: BOB,
						network: NetworkId::Any,
					})
				)
				.into()
			),
			4_000_000_000
		));
	});

	KusamaNet::execute_with(|| {
		let after_bob_free_balance = kusama_runtime::Balances::free_balance(&BOB.into());
		println!("BOB KSM AFTER balance on relay chain {} ", after_bob_free_balance);
		assert_eq!(
			after_bob_free_balance,
			999988476752
		);
	});

}
*/