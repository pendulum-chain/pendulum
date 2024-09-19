// Based on https://github.com/open-web3-stack/open-runtime-module-library/blob/83a76c2bf66c0b1236c31077e6fb24bb760a3535/xtokens/src/mock/para.rs
#![cfg(test)]

use codec::{Decode, Encode, MaxEncodedLen};
use core::marker::PhantomData;
use cumulus_pallet_parachain_system::{self, RelayNumberStrictlyIncreases};
use frame_support::{
	match_types, parameter_types,
	traits::{ConstU32, ContainsPair, Everything, Nothing, ProcessMessageError},
};
use frame_system::EnsureRoot;
use orml_traits::{
	location::{RelativeReserveProvider, Reserve},
	parameter_type_with_key,
};
use pallet_xcm::XcmPassthrough;
use polkadot_parachain::primitives::Sibling;
use polkadot_runtime_common::MAXIMUM_BLOCK_WEIGHT;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::H256;
use sp_runtime::{
	generic, impl_opaque_keys,
	traits::{BlakeTwo256, Convert, ConvertInto, IdentityLookup, MaybeEquivalence, Zero},
	AccountId32, Permill, Perquintill, RuntimeDebug,
};
use xcm::v3::prelude::*;
use xcm_emulator::Weight;
use xcm_executor::{
	traits::{JustTry, ShouldExecute, WeightTrader},
	Assets, XcmExecutor,
};

use crate::{definitions::asset_hub, AMPLITUDE_ID, ASSETHUB_ID, PENDULUM_ID};
use pendulum_runtime::definitions::moonbeam::BRZ_location;
use runtime_common::AuraId;
use xcm::latest::Weight as XCMWeight;
use xcm_builder::{
	AccountId32Aliases, AllowUnpaidExecutionFrom, ConvertedConcreteId, EnsureXcmOrigin,
	FixedWeightBounds, FungiblesAdapter, NoChecking, ParentIsPreset, RelayChainAsNative,
	SiblingParachainAsNative, SiblingParachainConvertsVia, SignedAccountId32AsNative,
	SignedToAccountId32, SovereignSignedViaLocation,
};
use xcm_executor::traits::Properties;

#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

pub const UNIT: runtime_common::Balance = 1_000_000_000_000;
pub const MILLISECS_PER_BLOCK: u64 = 12000;

// NOTE: Currently it is not possible to change the slot duration after the chain has started.
//       Attempting to do so will brick block production.
pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

// Time is measured by number of blocks.
pub const MINUTES: runtime_common::BlockNumber =
	60_000 / (MILLISECS_PER_BLOCK as runtime_common::BlockNumber);
pub const HOURS: runtime_common::BlockNumber = MINUTES * 60;
pub const DAYS: runtime_common::BlockNumber = HOURS * 24;
pub const BLOCKS_PER_YEAR: runtime_common::BlockNumber = DAYS * 36525 / 100;

const XCM_ASSET_RELAY_DOT: u8 = 0;
const XCM_ASSET_ASSETHUB_USDT: u8 = 1;

pub type AccountId = AccountId32;

pub type Header = generic::Header<BlockNumber, BlakeTwo256>;

parameter_types! {
	pub const RelayLocation: MultiLocation = MultiLocation::parent();
	pub const RelayNetwork: NetworkId = NetworkId::Polkadot;
	pub RelayChainOrigin: RuntimeOrigin = cumulus_pallet_xcm::Origin::Relay.into();
	pub CheckingAccount: AccountId = PolkadotXcm::check_account();
	pub UniversalLocation: InteriorMultiLocation =
		X2(GlobalConsensus(RelayNetwork::get()), Parachain(ParachainInfo::parachain_id().into()));

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

#[allow(clippy::upper_case_acronyms)]
#[derive(
	Encode,
	Decode,
	Eq,
	PartialEq,
	Copy,
	Clone,
	RuntimeDebug,
	PartialOrd,
	Ord,
	MaxEncodedLen,
	TypeInfo,
)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum CurrencyId {
	Pendulum,
	Amplitude,
	Native,
	XCM(u8),
	Token,
}

// Convert from u32 parachain id to CurrencyId
// Needed for the test macro so it works regardless of the XCM sender parachain
impl From<u32> for CurrencyId {
	fn from(id: u32) -> Self {
		match id {
			PENDULUM_ID => CurrencyId::Pendulum,
			AMPLITUDE_ID => CurrencyId::Amplitude,
			ASSETHUB_ID => CurrencyId::XCM(XCM_ASSET_ASSETHUB_USDT),
			id if id == u32::from(ParachainInfo::parachain_id()) => CurrencyId::Native,
			// Relay
			_ => CurrencyId::XCM(XCM_ASSET_RELAY_DOT),
		}
	}
}

/// CurrencyIdConvert
/// This type implements conversions from our `CurrencyId` type into `MultiLocation` and vice-versa.
/// A currency locally is identified with a `CurrencyId` variant but in the network it is identified
/// in the form of a `MultiLocation`, in this case a pCfg (Para-Id, Currency-Id).
pub struct CurrencyIdConvert;

// Only supports native currency for now
impl Convert<CurrencyId, Option<MultiLocation>> for CurrencyIdConvert {
	fn convert(id: CurrencyId) -> Option<MultiLocation> {
		match id {
			CurrencyId::Native => Some(MultiLocation::new(
				1,
				X2(Parachain(ParachainInfo::parachain_id().into()), PalletInstance(10)),
			)),
			CurrencyId::Pendulum =>
				Some(MultiLocation::new(1, X2(Parachain(PENDULUM_ID), PalletInstance(10)))),
			CurrencyId::Amplitude =>
				Some(MultiLocation::new(1, X2(Parachain(AMPLITUDE_ID), PalletInstance(10)))),
			CurrencyId::Token => Some(BRZ_location()),
			CurrencyId::XCM(f) => match f {
				XCM_ASSET_RELAY_DOT => Some(MultiLocation::parent()),
				// Handles both Kusama and Polkadot asset hub
				XCM_ASSET_ASSETHUB_USDT => Some(asset_hub::USDT_location()),
				_ => None,
			},
		}
	}
}

impl Convert<MultiLocation, Option<CurrencyId>> for CurrencyIdConvert {
	fn convert(location: MultiLocation) -> Option<CurrencyId> {
		match location {
			MultiLocation {
				parents: 1,
				interior: X2(Parachain(PENDULUM_ID), PalletInstance(10)),
			} => Some(CurrencyId::Pendulum),
			MultiLocation {
				parents: 1,
				interior: X2(Parachain(AMPLITUDE_ID), PalletInstance(10)),
			} => Some(CurrencyId::Amplitude),
			MultiLocation { parents: 0, interior: X1(PalletInstance(10)) } =>
				Some(CurrencyId::Native),
			// Handles both Kusama and Polkadot asset hub
			loc if loc == asset_hub::USDT_location() =>
				Some(CurrencyId::XCM(XCM_ASSET_ASSETHUB_USDT)),
			MultiLocation { parents: 1, interior: Here } =>
				Some(CurrencyId::XCM(XCM_ASSET_RELAY_DOT)),
			loc if loc == BRZ_location() => Some(CurrencyId::Token),
			_ => None,
		}
	}
}

impl Convert<MultiAsset, Option<CurrencyId>> for CurrencyIdConvert {
	fn convert(a: MultiAsset) -> Option<CurrencyId> {
		if let MultiAsset { id: AssetId::Concrete(id), fun: _ } = a {
			<Self as Convert<MultiLocation, Option<CurrencyId>>>::convert(id)
		} else {
			None
		}
	}
}

// Required this now for FungiblesAdapter.
impl MaybeEquivalence<MultiLocation, CurrencyId> for CurrencyIdConvert {
	fn convert(id: &MultiLocation) -> Option<CurrencyId> {
		<CurrencyIdConvert as Convert<MultiLocation, Option<CurrencyId>>>::convert(*id)
	}
	fn convert_back(what: &CurrencyId) -> Option<MultiLocation> {
		<CurrencyIdConvert as Convert<CurrencyId, Option<MultiLocation>>>::convert(*what)
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

#[allow(dead_code)]
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
		max_weight: Weight,
		weight_credit: &mut Properties,
	) -> Result<(), ProcessMessageError> {
		Deny::should_execute(origin, instructions, max_weight, weight_credit)?;
		Allow::should_execute(origin, instructions, max_weight, weight_credit)
	}
}

// See issue #5233
#[allow(dead_code)]
pub struct DenyReserveTransferToRelayChain;
impl ShouldExecute for DenyReserveTransferToRelayChain {
	fn should_execute<RuntimeCall>(
		origin: &MultiLocation,
		instructions: &mut [Instruction<RuntimeCall>],
		_max_weight: Weight,
		_weight_credit: &mut Properties,
	) -> Result<(), ProcessMessageError> {
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
			return Err(ProcessMessageError::Unsupported) // Deny
		}

		// allow reserve transfers to arrive from relay chain
		if matches!(origin, MultiLocation { parents: 1, interior: Here }) &&
			instructions.iter().any(|inst| matches!(inst, ReserveAssetDeposited { .. }))
		{
			println! {"Unexpected ReserveAssetDeposited from the relay chain"};
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
	type Trader = AllTokensAreCreatedEqualToWeight;
	type ResponseHandler = PolkadotXcm;
	type AssetTrap = PolkadotXcm;
	type AssetLocker = ();
	type AssetExchanger = ();
	type AssetClaims = PolkadotXcm;
	type SubscriptionService = PolkadotXcm;
	type PalletInstancesInfo = AllPalletsWithSystem;
	type MaxAssetsIntoHolding = ConstU32<8>;
	type FeeManager = ();
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = ();
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
	type WeightInfo = pallet_xcm::TestWeightInfo;
	type AdminOrigin = EnsureRoot<AccountId>;
	type MaxRemoteLockConsumers = ConstU32<0>;
	type RemoteLockConsumerIdentifier = ();

	#[cfg(feature = "runtime-benchmarks")]
	type ReachableDest = ();
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

parameter_types! {
	pub const ProposalBond: Permill = Permill::from_percent(5);
	pub const ProposalBondMinimum: Balance = 10 * UNIT;
	pub const SpendPeriod: BlockNumber = 7 * DAYS;
	pub const Burn: Permill = Permill::from_percent(0);
	pub const TreasuryPalletId: frame_support::PalletId = frame_support::PalletId(*b"py/trsry");
	pub const MaxApprovals: u32 = 100;
}

type TreasuryApproveOrigin = EnsureRoot<runtime_common::AccountId>;

type TreasuryRejectOrigin = EnsureRoot<runtime_common::AccountId>;

impl pallet_treasury::Config for Runtime {
	type PalletId = TreasuryPalletId;
	type Currency = Balances;
	type ApproveOrigin = TreasuryApproveOrigin;
	type RejectOrigin = TreasuryRejectOrigin;
	type RuntimeEvent = RuntimeEvent;
	type OnSlash = Treasury;
	type ProposalBond = ProposalBond;
	type ProposalBondMinimum = ProposalBondMinimum;
	type ProposalBondMaximum = ();
	type SpendPeriod = SpendPeriod;
	type Burn = Burn;
	type BurnDestination = ();
	type SpendFunds = ();
	type WeightInfo = pallet_treasury::weights::SubstrateWeight<Runtime>;
	type MaxApprovals = MaxApprovals;
	type SpendOrigin = frame_support::traits::NeverEnsureOrigin<u128>;
}
pub struct AccountIdToMultiLocation;
impl Convert<AccountId, MultiLocation> for AccountIdToMultiLocation {
	fn convert(account: AccountId) -> MultiLocation {
		MultiLocation {
			parents: 0,
			interior: X1(xcm::v3::Junction::AccountId32 { network: None, id: account.into() }),
		}
	}
}

impl cumulus_pallet_xcm::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
}

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Runtime>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Runtime
	{
		System: frame_system,
		Tokens: orml_tokens,
		Timestamp: pallet_timestamp,
		XTokens: orml_xtokens,
		Balances: pallet_balances,
		PolkadotXcm: pallet_xcm,
		ParachainSystem: cumulus_pallet_parachain_system,
		ParachainInfo: parachain_info,
		XcmpQueue: cumulus_pallet_xcmp_queue,
		DmpQueue: cumulus_pallet_dmp_queue,
		CumulusXcm: cumulus_pallet_xcm,

		Treasury: pallet_treasury,

		Aura: pallet_aura = 33,
		Session: pallet_session = 32,
		ParachainStaking: parachain_staking = 35,
		Authorship: pallet_authorship = 30,
		AuraExt: cumulus_pallet_aura_ext = 34,


	}
);

pub type Balance = u128;
pub type BlockNumber = u32;
pub type Index = u32;
pub type Amount = i64;

parameter_types! {
	pub const BlockHashCount: u32 = 250;
	pub const SS58Prefix: u8 = 42;
}
impl frame_system::Config for Runtime {
	type Block = Block;
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = Index;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Self>;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
	pub const MaxLocks: u32 = 50;
}

parameter_type_with_key! {
	pub ExistentialDeposits: |_currency_id: CurrencyId| -> Balance {
		0
	};
}

pub struct CurrencyHooks<T>(sp_std::marker::PhantomData<T>);
impl<T: orml_tokens::Config>
	orml_traits::currency::MutationHooks<T::AccountId, T::CurrencyId, T::Balance> for CurrencyHooks<T>
{
	type OnDust = orml_tokens::BurnDust<T>;
	type OnSlash = ();
	type PreDeposit = ();
	type PostDeposit = ();
	type PreTransfer = ();
	type PostTransfer = ();
	type OnNewTokenAccount = ();
	type OnKilledTokenAccount = ();
}

impl orml_tokens::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type Amount = Amount;
	type CurrencyId = CurrencyId;
	type WeightInfo = ();
	type ExistentialDeposits = ExistentialDeposits;
	type CurrencyHooks = CurrencyHooks<Self>;
	type MaxLocks = MaxLocks;
	type MaxReserves = ConstU32<0>;
	type ReserveIdentifier = ();
	type DustRemovalWhitelist = Everything;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1000;
	pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Runtime {
	type MaxLocks = MaxLocks;
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
	type MaxReserves = MaxReserves;
	type ReserveIdentifier = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type MaxHolds = ConstU32<1>;
	type RuntimeHoldReason = RuntimeHoldReason;
}

parameter_types! {
	pub const ReservedXcmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
	pub const ReservedDmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
}

impl cumulus_pallet_parachain_system::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type SelfParaId = parachain_info::Pallet<Runtime>;
	type DmpMessageHandler = DmpQueue;
	type ReservedDmpWeight = ReservedDmpWeight;
	type OutboundXcmpMessageSource = XcmpQueue;
	type XcmpMessageHandler = XcmpQueue;
	type ReservedXcmpWeight = ReservedXcmpWeight;
	type CheckAssociatedRelayNumber = RelayNumberStrictlyIncreases;
	#[cfg(feature = "std")]
	type ConsensusHook = cumulus_pallet_parachain_system::consensus_hook::ExpectParentIncluded;
}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type ChannelInfo = ParachainSystem;
	type VersionWrapper = ();
	type ExecuteOverweightOrigin = EnsureRoot<AccountId>;
	type ControllerOrigin = EnsureRoot<AccountId>;
	type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
	type PriceForSiblingDelivery = ();
	type WeightInfo = cumulus_pallet_xcmp_queue::weights::SubstrateWeight<Runtime>;
}

impl cumulus_pallet_dmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type ExecuteOverweightOrigin = EnsureRoot<AccountId>;
}

parameter_types! {
	// as per documentation, typical value for this is false "unless this pallet is being augmented by another pallet"
	// https://github.com/paritytech/polkadot-sdk/blob/release-polkadot-v1.1.0/substrate/frame/aura/src/lib.rs#L111
	pub const AllowMultipleBlocksPerSlot: bool = false;
	pub const MaxAuthorities: u32 = 200;
}

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = MaxAuthorities;
	type AllowMultipleBlocksPerSlot = AllowMultipleBlocksPerSlot;
}

parameter_types! {
	pub const MinimumPeriod: u64 = SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Runtime {
	/// A timestamp: milliseconds since the unix epoch.
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = pallet_timestamp::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const Offset: u32 = 0;
}

impl pallet_authorship::Config for Runtime {
	type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
	type EventHandler = ParachainStaking;
}

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: Aura,
	}
}

parameter_types! {
	pub const MinBlocksPerRound: BlockNumber = HOURS;
	pub const DefaultBlocksPerRound: BlockNumber = 2 * HOURS;
	pub const StakeDuration: BlockNumber = 7 * DAYS;
	pub const ExitQueueDelay: u32 = 2;
	pub const MinCollators: u32 = 8;
	pub const MinRequiredCollators: u32 = 2;
	pub const MaxDelegationsPerRound: u32 = 1;
	#[derive(Debug, Eq, PartialEq)]
	pub const MaxDelegatorsPerCollator: u32 = 40;
	pub const MinCollatorStake: Balance = 5_000 * UNIT;
	pub const MinDelegatorStake: Balance = 10 * UNIT;
	#[derive(Debug, Eq, PartialEq)]
	pub const MaxCollatorCandidates: u32 = 40;
	pub const MaxUnstakeRequests: u32 = 10;
	pub const NetworkRewardStart: BlockNumber = BlockNumber::MAX;
	pub const NetworkRewardRate: Perquintill = Perquintill::from_percent(0);
	pub const CollatorRewardRateDecay: Perquintill = Perquintill::from_parts(936_879_853_200_000_000u64);
}

impl parachain_staking::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type CurrencyBalance = runtime_common::Balance;

	type MinBlocksPerRound = MinBlocksPerRound;
	type DefaultBlocksPerRound = DefaultBlocksPerRound;
	type StakeDuration = StakeDuration;
	type ExitQueueDelay = ExitQueueDelay;
	type MinCollators = MinCollators;
	type MinRequiredCollators = MinRequiredCollators;
	type MaxDelegationsPerRound = MaxDelegationsPerRound;
	type MaxDelegatorsPerCollator = MaxDelegatorsPerCollator;
	type MinCollatorStake = MinCollatorStake;
	type MinCollatorCandidateStake = MinCollatorStake;
	type MaxTopCandidates = MaxCollatorCandidates;
	type MinDelegatorStake = MinDelegatorStake;
	type MaxUnstakeRequests = MaxUnstakeRequests;
	type NetworkRewardRate = NetworkRewardRate;
	type NetworkRewardStart = NetworkRewardStart;
	type NetworkRewardBeneficiary = Treasury;
	type CollatorRewardRateDecay = CollatorRewardRateDecay;
	type WeightInfo = parachain_staking::default_weights::SubstrateWeight<Runtime>;

	const BLOCKS_PER_YEAR: runtime_common::BlockNumber = BLOCKS_PER_YEAR;
}

impl pallet_session::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type ValidatorIdOf = ConvertInto;
	type ShouldEndSession = ParachainStaking;
	type NextSessionRotation = ParachainStaking;
	type SessionManager = ParachainStaking;
	type SessionHandler = <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
	type Keys = SessionKeys;
	type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
}

impl parachain_info::Config for Runtime {}

impl cumulus_pallet_aura_ext::Config for Runtime {}

/// A trader who believes all tokens are created equal to "weight" of any chain,
/// which is not true, but good enough to mock the fee payment of XCM execution.
///
/// This mock will always trade `n` amount of weight to `n` amount of tokens.
pub struct AllTokensAreCreatedEqualToWeight(MultiLocation);
impl WeightTrader for AllTokensAreCreatedEqualToWeight {
	fn new() -> Self {
		Self(MultiLocation::parent())
	}

	fn buy_weight(
		&mut self,
		weight: Weight,
		payment: Assets,
		_context: &XcmContext,
	) -> Result<Assets, XcmError> {
		let asset_id = payment.fungible.iter().next().expect("Payment must be something; qed").0;
		let required = MultiAsset { id: *asset_id, fun: Fungible(weight.ref_time() as u128) };

		if let MultiAsset { fun: _, id: Concrete(ref id) } = &required {
			self.0 = *id;
		}

		let unused = payment.checked_sub(required).map_err(|_| XcmError::TooExpensive)?;
		Ok(unused)
	}

	fn refund_weight(&mut self, weight: Weight, _context: &XcmContext) -> Option<MultiAsset> {
		if weight.is_zero() {
			None
		} else {
			Some((self.0, weight.ref_time() as u128).into())
		}
	}
}
