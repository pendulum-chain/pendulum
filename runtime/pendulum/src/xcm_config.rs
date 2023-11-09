use super::{
	AccountId, Balance, Balances, CurrencyId, ParachainInfo, ParachainSystem, PolkadotXcm, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeOrigin, Tokens, WeightToFee, XcmpQueue,
};
use crate::ConstU32;
use core::marker::PhantomData;
use frame_support::{
	log, match_types, parameter_types,
	traits::{ContainsPair, Everything, Nothing},
};
use orml_traits::{
	location::{RelativeReserveProvider, Reserve},
	parameter_type_with_key,
};
use pallet_xcm::XcmPassthrough;
use polkadot_parachain::primitives::Sibling;
use polkadot_runtime_common::impls::ToAuthor;
use runtime_common::parachains::polkadot::{asset_hub, equilibrium, moonbeam};
use sp_runtime::traits::Convert;
use xcm::latest::{prelude::*, Weight as XCMWeight};
use xcm_builder::{
	AccountId32Aliases, AllowUnpaidExecutionFrom, ConvertedConcreteId, EnsureXcmOrigin,
	FixedWeightBounds, FungiblesAdapter, NoChecking, ParentIsPreset, RelayChainAsNative,
	SiblingParachainAsNative, SiblingParachainConvertsVia, SignedAccountId32AsNative,
	SignedToAccountId32, SovereignSignedViaLocation, UsingComponents,
};
use xcm_executor::{
	traits::{JustTry, ShouldExecute},
	XcmExecutor,
};

const XCM_ASSET_RELAY_DOT: u8 = 0;
const XCM_ASSET_ASSETHUB_USDT: u8 = 1;
const XCM_ASSET_ASSETHUB_USDC: u8 = 2;
const XCM_ASSET_EQUILIBRIUM_EQD: u8 = 3;
const XCM_ASSET_MOONBEAM_BRZ: u8 = 4;

parameter_types! {
	pub const RelayLocation: MultiLocation = MultiLocation::parent();
	pub const RelayNetwork: NetworkId = NetworkId::Polkadot;
	pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
	pub UniversalLocation: InteriorMultiLocation =
		X2(GlobalConsensus(RelayNetwork::get()), Parachain(ParachainInfo::parachain_id().into()));

}

// macro_rules! parachain_asset_loc {
// 	($module:ident, $asset_id:path) => {
// 		MultiLocation {
// 			parents: 1,
// 			interior: X3(
// 				Parachain($module::PARA_ID),
// 				PalletInstance($module::ASSET_PALLET_ID),
// 				GeneralIndex($asset_id),
// 			),
// 		}
// 	};
// }

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

/// CurrencyIdConvert
/// This type implements conversions from our `CurrencyId` type into `MultiLocation` and vice-versa.
/// A currency locally is identified with a `CurrencyId` variant but in the network it is identified
/// in the form of a `MultiLocation`, in this case a pCfg (Para-Id, Currency-Id).
pub struct CurrencyIdConvert;

impl Convert<CurrencyId, Option<MultiLocation>> for CurrencyIdConvert {
	fn convert(id: CurrencyId) -> Option<MultiLocation> {
		match id {
			// CurrencyId::KSM => Some(MultiLocation::parent()),
			CurrencyId::XCM(f) => match f {
				XCM_ASSET_RELAY_DOT => Some(MultiLocation::parent()),
				XCM_ASSET_ASSETHUB_USDT => Some(asset_hub::USDT_location()),
				XCM_ASSET_ASSETHUB_USDC => Some(asset_hub::USDC_location()),
				XCM_ASSET_EQUILIBRIUM_EQD => Some(equilibrium::EQD_location()),
				XCM_ASSET_MOONBEAM_BRZ => Some(moonbeam::BRZ_location()),
				_ => None,
			},
			CurrencyId::Native => Some(MultiLocation::new(
				1,
				X2(Parachain(ParachainInfo::parachain_id().into()), PalletInstance(10)),
			)),
			_ => None,
		}
	}
}

impl Convert<MultiLocation, Option<CurrencyId>> for CurrencyIdConvert {
	fn convert(location: MultiLocation) -> Option<CurrencyId> {
		match location {
			MultiLocation { parents: 1, interior: Here } =>
				Some(CurrencyId::XCM(XCM_ASSET_RELAY_DOT)),
			loc if loc == asset_hub::USDT_location() =>
				Some(CurrencyId::XCM(XCM_ASSET_ASSETHUB_USDT)),
			loc if loc == asset_hub::USDC_location() =>
				Some(CurrencyId::XCM(XCM_ASSET_ASSETHUB_USDC)),
			loc if loc == equilibrium::EQD_location() =>
				Some(CurrencyId::XCM(XCM_ASSET_EQUILIBRIUM_EQD)),
			loc if loc == moonbeam::BRZ_location() => Some(CurrencyId::XCM(XCM_ASSET_MOONBEAM_BRZ)),
			// Our native currency location without re-anchoring
			MultiLocation { parents: 1, interior: X2(Parachain(id), PalletInstance(10)) }
				if id == u32::from(ParachainInfo::parachain_id()) =>
				Some(CurrencyId::Native),
			// Our native currency location with re-anchoring
			// The XCM pallet will try to re-anchor the location before it reaches here
			MultiLocation { parents: 0, interior: X1(PalletInstance(10)) } =>
				Some(CurrencyId::Native),
			_ => None,
		}
	}
}

impl Convert<MultiAsset, Option<CurrencyId>> for CurrencyIdConvert {
	fn convert(a: MultiAsset) -> Option<CurrencyId> {
		if let MultiAsset { id: AssetId::Concrete(id), fun: _ } = a {
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
		<CurrencyIdConvert as Convert<MultiLocation, Option<CurrencyId>>>::convert(location.clone())
			.ok_or(location)
	}
}

/// A `FilterAssetLocation` implementation. Filters multi native assets whose
/// reserve is same with `origin`.
pub struct MultiNativeAsset<ReserveProvider>(PhantomData<ReserveProvider>);
impl<ReserveProvider> ContainsPair<MultiAsset, MultiLocation> for MultiNativeAsset<ReserveProvider>
where
	ReserveProvider: Reserve,
{
	fn contains(asset: &MultiAsset, origin: &MultiLocation) -> bool {
		if let Some(ref reserve) = ReserveProvider::reserve(asset) {
			if reserve == origin {
				return true
			}
		}
		false
	}
}

/// Means for transacting the fungibles assets of ths parachain.
pub type FungiblesTransactor = FungiblesAdapter<
	// Use this fungibles implementation
	Tokens,
	// This means that this adapter should handle any token that `CurrencyIdConvert` can convert
	// to `CurrencyId`, the `CurrencyId` type of `Tokens`, the fungibles implementation it uses.
	ConvertedConcreteId<CurrencyId, Balance, CurrencyIdConvert, JustTry>,
	// Convert an XCM MultiLocation into a local account id
	LocationToAccountId,
	// Our chain's account ID type (we can't get away without mentioning it explicitly)
	AccountId,
	// We dont allow teleports.
	NoChecking,
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
	SovereignSignedViaLocation<LocationToAccountId, RuntimeOrigin>,
	// Native converter for Relay-chain (Parent) location; will converts to a `Relay` origin when
	// recognized.
	RelayChainAsNative<RelayChainOrigin, RuntimeOrigin>,
	// Native converter for sibling Parachains; will convert to a `SiblingPara` origin when
	// recognized.
	SiblingParachainAsNative<cumulus_pallet_xcm::Origin, RuntimeOrigin>,
	// Native signed account converter; this just converts an `AccountId32` origin into a normal
	// `Origin::Signed` origin of the same 32-byte value.
	SignedAccountId32AsNative<RelayNetwork, RuntimeOrigin>,
	// Xcm origins can be represented natively under the Xcm pallet's Xcm origin.
	XcmPassthrough<RuntimeOrigin>,
);

parameter_types! {
	// One XCM operation is 1_000_000_000 weight - almost certainly a conservative estimate.
	pub UnitWeightCost: XCMWeight = XCMWeight::from_parts(1_000_000_000, 0);
	pub const MaxInstructions: u32 = 100;
	pub SelfLocation: MultiLocation = MultiLocation::new(1, X1(Parachain(ParachainInfo::parachain_id().into())));
	pub const BaseXcmWeight: XCMWeight = XCMWeight::from_parts(150_000_000, 0);
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
	fn should_execute<RuntimeCall>(
		origin: &MultiLocation,
		instructions: &mut [Instruction<RuntimeCall>],
		max_weight: XCMWeight,
		weight_credit: &mut XCMWeight,
	) -> Result<(), ()> {
		Deny::should_execute(origin, instructions, max_weight, weight_credit)?;
		Allow::should_execute(origin, instructions, max_weight, weight_credit)
	}
}

// See issue #5233
pub struct DenyReserveTransferToRelayChain;
impl ShouldExecute for DenyReserveTransferToRelayChain {
	fn should_execute<RuntimeCall>(
		origin: &MultiLocation,
		instructions: &mut [Instruction<RuntimeCall>],
		_max_weight: XCMWeight,
		_weight_credit: &mut XCMWeight,
	) -> Result<(), ()> {
		if instructions.iter().any(|inst| {
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
			instructions.iter().any(|inst| matches!(inst, ReserveAssetDeposited { .. }))
		{
			log::trace!(
				target: "xcm::barriers",
				"Unexpected ReserveAssetDeposited from the relay chain",
			);
		}
		// Permit everything else
		Ok(())
	}
}

pub type Barrier = AllowUnpaidExecutionFrom<Everything>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	// How to withdraw and deposit an asset.
	type AssetTransactor = FungiblesTransactor;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	type IsReserve = MultiNativeAsset<RelativeReserveProvider>;
	// Teleporting is disabled.
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type Trader =
		UsingComponents<WeightToFee, RelayLocation, AccountId, Balances, ToAuthor<Runtime>>;
	type ResponseHandler = PolkadotXcm;
	type AssetTrap = PolkadotXcm;
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = PolkadotXcm;
	type SubscriptionService = PolkadotXcm;
	type PalletInstancesInfo = crate::AllPalletsWithSystem;
	type MaxAssetsIntoHolding = ConstU32<8>;
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
}

/// No local origins on this chain are allowed to dispatch XCM sends/executions.
pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

/// The means for routing XCM messages which are not for local execution into the right message
/// queues.
pub type XcmRouter = (
	// Two routers - use UMP to communicate with the relay chain:
	cumulus_primitives_utility::ParentAsUmp<ParachainSystem, PolkadotXcm, ()>,
	// ..and XCMP to communicate with the sibling chains.
	XcmpQueue,
);

impl pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type CurrencyMatcher = ();
	type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmRouter = XcmRouter;
	type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
	type XcmExecuteFilter = Everything;
	// ^ Disable dispatchable execute on the XCM pallet.
	// Needs to be `Everything` for local testing.
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type XcmTeleportFilter = Nothing;
	type XcmReserveTransferFilter = Everything;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type UniversalLocation = UniversalLocation;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;

	const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
	// ^ Override for AdvertisedXcmVersion default
	type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
	type TrustedLockers = ();
	type SovereignAccountOf = LocationToAccountId;
	type MaxLockers = ConstU32<8>;
	type WeightInfo = crate::weights::pallet_xcm::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type ReachableDest = ReachableDest;
}

#[cfg(feature = "runtime-benchmarks")]
parameter_types! {
	pub ReachableDest: Option<MultiLocation> = Some(Parent.into());
}

parameter_type_with_key! {
	pub ParachainMinFee: |_location: MultiLocation| -> Option<u128> {
		None
	};
}

impl orml_xtokens::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type CurrencyId = CurrencyId;
	type CurrencyIdConvert = CurrencyIdConvert;
	type AccountIdToMultiLocation = AccountIdToMultiLocation;
	type SelfLocation = SelfLocation;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type BaseXcmWeight = BaseXcmWeight;
	type MaxAssetsForTransfer = MaxAssetsForTransfer;
	type MinXcmFee = ParachainMinFee; //TODO to support hrmp transfer beetween parachain adjust this parameter
	type MultiLocationsFilter = Everything;
	type ReserveProvider = RelativeReserveProvider;
	type UniversalLocation = UniversalLocation;
}

pub struct AccountIdToMultiLocation;
impl Convert<AccountId, MultiLocation> for AccountIdToMultiLocation {
	fn convert(account: AccountId) -> MultiLocation {
		MultiLocation {
			parents: 0,
			interior: X1(AccountId32 { network: None, id: account.into() }),
		}
	}
}

impl cumulus_pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}
