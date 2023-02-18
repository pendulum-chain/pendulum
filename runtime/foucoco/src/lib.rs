#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

use orml_traits::MultiCurrency;

mod weights;
pub mod xcm_config;
pub mod zenlink;
use crate::zenlink::*;
use xcm::v1::MultiLocation;
use zenlink_protocol::{AssetBalance, MultiAssetsHandler, PairInfo};

pub use parachain_staking::InflationInfo;

use codec::Encode;
use frame_system::Origin;

use smallvec::smallvec;
use sp_api::impl_runtime_apis;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata, H256};
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	traits::{
		AccountIdConversion, AccountIdLookup, BlakeTwo256, Block as BlockT, Convert, ConvertInto,
	},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, DispatchError, FixedPointNumber, SaturatedConversion,
};

use sp_std::{marker::PhantomData, prelude::*};
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

use frame_support::{
	construct_runtime,
	dispatch::DispatchClass,
	parameter_types,
	traits::{
		ConstBool, ConstU32, Contains, Currency as FrameCurrency, EitherOfDiverse,
		EqualPrivilegeOnly, Imbalance, OnUnbalanced, WithdrawReasons,
	},
	weights::{
		constants::WEIGHT_REF_TIME_PER_SECOND, ConstantMultiplier, Weight, WeightToFeeCoefficient,
		WeightToFeeCoefficients, WeightToFeePolynomial,
	},
	PalletId,
};
use frame_system::{
	limits::{BlockLength, BlockWeights},
	EnsureRoot,
};
pub use sp_runtime::{MultiAddress, Perbill, Permill, Perquintill};

use runtime_common::{
	opaque, AuraId, Index, ReserveIdentifier, EXISTENTIAL_DEPOSIT, MICROUNIT, MILLIUNIT, NANOUNIT,
	UNIT,
};

use cumulus_pallet_parachain_system::RelayNumberStrictlyIncreases;

use dia_oracle::DiaOracle;

use xcm_config::{XcmConfig, XcmOriginToTransactDispatchOrigin};

use currency::Amount;
use orml_currencies::BasicCurrencyAdapter;
use orml_traits::{currency::MutationHooks, parameter_type_with_key};

#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

pub use dia_oracle::dia::AssetId;
pub use issue::{Event as IssueEvent, IssueRequest};
pub use nomination::Event as NominationEvent;
pub use redeem::{Event as RedeemEvent, RedeemRequest};
pub use replace::{Event as ReplaceEvent, ReplaceRequest};
pub use security::StatusCode;
pub use stellar_relay::traits::{FieldLength, Organization, Validator};

pub use module_oracle_rpc_runtime_api::BalanceWrapper;
use oracle::dia::DiaOracleAdapter;

// Polkadot imports
use polkadot_runtime_common::{BlockHashCount, SlowAdjustingFeeUpdate};

use spacewalk_primitives::{
	self as primitives, AccountId, Balance, BlockNumber, CurrencyId, CurrencyId::XCM,
	ForeignCurrencyId, Hash, Moment, Signature, SignedFixedPoint, SignedInner, UnsignedFixedPoint,
	UnsignedInner,
};

use weights::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight};

// XCM Imports
use xcm_executor::XcmExecutor;

pub type VaultId = primitives::VaultId<AccountId, CurrencyId>;

/// The address format for describing accounts.
pub type Address = MultiAddress<AccountId, ()>;

/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;

/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;

/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;

/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
	pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, RuntimeCall, SignedExtra>;

pub type SignedPayload = generic::SignedPayload<RuntimeCall, SignedExtra>;

/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
>;

type DataProviderImpl = DiaOracleAdapter<
	DiaOracleModule,
	UnsignedFixedPoint,
	Moment,
	primitives::DiaOracleKeyConvertor,
	ConvertPrice,
	ConvertMoment,
>;

pub struct ConvertPrice;
impl Convert<u128, Option<UnsignedFixedPoint>> for ConvertPrice {
	fn convert(price: u128) -> Option<UnsignedFixedPoint> {
		Some(UnsignedFixedPoint::from_inner(price))
	}
}

pub struct ConvertMoment;
impl Convert<u64, Option<Moment>> for ConvertMoment {
	fn convert(moment: u64) -> Option<Moment> {
		Some(moment)
	}
}

/// Handles converting a weight scalar to a fee value, based on the scale and granularity of the
/// node's balance type.
///
/// This should typically create a mapping between the following ranges:
///   - `[0, MAXIMUM_BLOCK_WEIGHT]`
///   - `[Balance::min, Balance::max]`
///
/// Yet, it can be used for any other sort of change to weight-fee. Some examples being:
///   - Setting it to `0` will essentially disable the weight fee.
///   - Setting it to `1` will cause the literal `#[weight = x]` values to be charged.
pub struct WeightToFee;
impl WeightToFeePolynomial for WeightToFee {
	type Balance = Balance;
	fn polynomial() -> WeightToFeeCoefficients<Self::Balance> {
		let p = MILLIUNIT;
		let q = 10 * Balance::from(ExtrinsicBaseWeight::get().ref_time());
		smallvec![WeightToFeeCoefficient {
			degree: 1,
			negative: false,
			coeff_frac: Perbill::from_rational(p % q, q),
			coeff_integer: p / q,
		}]
	}
}

impl_opaque_keys! {
	pub struct SessionKeys {
		pub aura: Aura,
	}
}

#[sp_version::runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("amplitude"),
	impl_name: create_runtime_str!("amplitude"),
	authoring_version: 1,
	spec_version: 8,
	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 8,
	state_version: 1,
};

/// This determines the average expected block time that we are targeting.
/// Blocks will be produced at a minimum duration defined by `SLOT_DURATION`.
/// `SLOT_DURATION` is picked up by `pallet_timestamp` which is in turn picked
/// up by `pallet_aura` to implement `fn slot_duration()`.
///
/// Change this to adjust the block time.
pub const MILLISECS_PER_BLOCK: u64 = 12000;

// NOTE: Currently it is not possible to change the slot duration after the chain has started.
//       Attempting to do so will brick block production.
pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

// Prints debug output of the `contracts` pallet to stdout if the node is
// started with `-lruntime::contracts=debug`.
const CONTRACTS_DEBUG_OUTPUT: bool = true;

// Time is measured by number of blocks.
pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
pub const HOURS: BlockNumber = MINUTES * 60;
pub const DAYS: BlockNumber = HOURS * 24;
pub const BLOCKS_PER_YEAR: BlockNumber = DAYS * 36525 / 100;

/// We assume that ~5% of the block weight is consumed by `on_initialize` handlers. This is
/// used to limit the maximal weight of a single extrinsic.
const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(5);

/// We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used by
/// `Operational` extrinsics.
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

/// We allow for 0.5 of a second of compute with a 12 second average block time.
const MAXIMUM_BLOCK_WEIGHT: Weight =
	Weight::from_ref_time(WEIGHT_REF_TIME_PER_SECOND.saturating_div(2))
		.set_proof_size(cumulus_primitives_core::relay_chain::v2::MAX_POV_SIZE as u64);

// For mainnet USDC issued by centre.io
pub const WRAPPED_USDC_CURRENCY: CurrencyId = CurrencyId::AlphaNum4 {
	code: *b"USDC",
	issuer: [
		59, 153, 17, 56, 14, 254, 152, 139, 160, 168, 144, 14, 177, 207, 228, 79, 54, 111, 125,
		190, 148, 107, 237, 7, 114, 64, 247, 246, 36, 223, 21, 197,
	],
};

/// The version information used to identify this runtime when compiled natively.
#[cfg(feature = "std")]
pub fn native_version() -> NativeVersion {
	NativeVersion { runtime_version: VERSION, can_author_with: Default::default() }
}

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;

	// This part is copied from Substrate's `bin/node/runtime/src/lib.rs`.
	//  The `RuntimeBlockLength` and `RuntimeBlockWeights` exist here because the
	// `DeletionWeightLimit` and `DeletionQueueDepth` depend on those to parameterize
	// the lazy contract deletion.
	pub RuntimeBlockLength: BlockLength =
		BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
		.base_block(BlockExecutionWeight::get())
		.for_class(DispatchClass::all(), |weights| {
			weights.base_extrinsic = ExtrinsicBaseWeight::get();
		})
		.for_class(DispatchClass::Normal, |weights| {
			weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
		})
		.for_class(DispatchClass::Operational, |weights| {
			weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
			// Operational transactions have some extra reserved space, so that they
			// are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
			weights.reserved = Some(
				MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT
			);
		})
		.avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
		.build_or_panic();
	pub const SS58Prefix: u16 = 57;
}

pub struct BaseFilter;
impl Contains<RuntimeCall> for BaseFilter {
	fn contains(call: &RuntimeCall) -> bool {
		match call {
			// These modules are all allowed to be called by transactions:
			RuntimeCall::Bounties(_) |
			RuntimeCall::ChildBounties(_) |
			RuntimeCall::Treasury(_) |
			RuntimeCall::Tokens(_) |
			RuntimeCall::Currencies(_) |
			RuntimeCall::ParachainStaking(_) |
			RuntimeCall::Democracy(_) |
			RuntimeCall::Council(_) |
			RuntimeCall::TechnicalCommittee(_) |
			RuntimeCall::System(_) |
			RuntimeCall::Scheduler(_) |
			RuntimeCall::Preimage(_) |
			RuntimeCall::Timestamp(_) |
			RuntimeCall::Balances(_) |
			RuntimeCall::Authorship(_) |
			RuntimeCall::Session(_) |
			RuntimeCall::ParachainSystem(_) |
			RuntimeCall::Sudo(_) |
			RuntimeCall::XcmpQueue(_) |
			RuntimeCall::PolkadotXcm(_) |
			RuntimeCall::DmpQueue(_) |
			RuntimeCall::Utility(_) |
			RuntimeCall::Vesting(_) |
			RuntimeCall::XTokens(_) |
			RuntimeCall::Multisig(_) |
			RuntimeCall::Identity(_) |
			RuntimeCall::Contracts(_) |
			RuntimeCall::ZenlinkProtocol(_) |
			RuntimeCall::DiaOracleModule(_) |
			RuntimeCall::Fee(_) |
			RuntimeCall::Issue(_) |
			RuntimeCall::Nomination(_) |
			RuntimeCall::Oracle(_) |
			RuntimeCall::Redeem(_) |
			RuntimeCall::Replace(_) |
			RuntimeCall::Security(_) |
			RuntimeCall::StellarRelay(_) |
			RuntimeCall::VaultRegistry(_) |
			RuntimeCall::VaultRewards(_) |
			RuntimeCall::TokenAllowance(_) => true,
			// All pallets are allowed, but exhaustive match is defensive
			// in the case of adding new pallets.
		}
	}
}

// Configure FRAME pallets to include in runtime.

impl frame_system::Config for Runtime {
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The aggregated dispatch type that is available for extrinsics.
	type RuntimeCall = RuntimeCall;
	/// The lookup mechanism to get account ID from whatever is passed in dispatchers.
	type Lookup = AccountIdLookup<AccountId, ()>;
	/// The index type for storing how many extrinsics an account has signed.
	type Index = Index;
	/// The index type for blocks.
	type BlockNumber = BlockNumber;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The hashing algorithm used.
	type Hashing = BlakeTwo256;
	/// The header type.
	type Header = generic::Header<BlockNumber, BlakeTwo256>;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	/// The ubiquitous origin type.
	type RuntimeOrigin = RuntimeOrigin;
	/// Maximum number of block number to block hash mappings to keep (oldest pruned first).
	type BlockHashCount = BlockHashCount;
	/// Runtime version.
	type Version = Version;
	/// Converts a module to an index of this module in the runtime.
	type PalletInfo = PalletInfo;
	/// The data to be stored in an account.
	type AccountData = pallet_balances::AccountData<Balance>;
	/// What to do if a new account is created.
	type OnNewAccount = ();
	/// What to do if an account is fully reaped from the system.
	type OnKilledAccount = ();
	/// The weight of database operations that the runtime can invoke.
	type DbWeight = RocksDbWeight;
	/// The basic call filter to use in dispatchable.
	type BaseCallFilter = BaseFilter;
	/// Weight information for the extrinsics of this pallet.
	type SystemWeightInfo = ();
	/// Block & extrinsics weights: base values and limits.
	type BlockWeights = RuntimeBlockWeights;
	/// The maximum length of a block (in bytes).
	type BlockLength = RuntimeBlockLength;
	/// This is used as an identifier of the chain. 57 is the prefix for Foucoco
	type SS58Prefix = SS58Prefix;
	/// The action to take on a Runtime Upgrade
	type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Self>;
	type MaxConsumers = frame_support::traits::ConstU32<16>;
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
	pub const UncleGenerations: u32 = 2;
}

impl pallet_authorship::Config for Runtime {
	type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
	type UncleGenerations = UncleGenerations;
	type FilterUncle = ();
	type EventHandler = ParachainStaking;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
	pub const MaxLocks: u32 = 50;
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
	type ReserveIdentifier = ReserveIdentifier;
}

parameter_types! {
	/// Relay Chain `TransactionByteFee` / 10
	pub const TransactionByteFee: Balance = 10 * MICROUNIT;
	pub const OperationalFeeMultiplier: u8 = 5;
}

type NegativeImbalance = <Balances as FrameCurrency<AccountId>>::NegativeImbalance;

pub struct DealWithFees;
impl OnUnbalanced<NegativeImbalance> for DealWithFees {
	fn on_unbalanceds<B>(mut fees_then_tips: impl Iterator<Item = NegativeImbalance>) {
		if let Some(mut fees) = fees_then_tips.next() {
			if let Some(tips) = fees_then_tips.next() {
				tips.merge_into(&mut fees);
			}
			// for fees and tips, 100% to treasury
			Treasury::on_unbalanced(fees);
		}
	}
}

impl pallet_transaction_payment::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = pallet_transaction_payment::CurrencyAdapter<Balances, DealWithFees>;
	type WeightToFee = WeightToFee;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
	type OperationalFeeMultiplier = OperationalFeeMultiplier;
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
}

impl parachain_info::Config for Runtime {}

impl cumulus_pallet_aura_ext::Config for Runtime {}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type ChannelInfo = ParachainSystem;
	type VersionWrapper = ();
	type ExecuteOverweightOrigin = EnsureRoot<AccountId>;
	type ControllerOrigin = EnsureRoot<AccountId>;
	type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
	type WeightInfo = cumulus_pallet_xcmp_queue::weights::SubstrateWeight<Runtime>;
}

impl cumulus_pallet_dmp_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type ExecuteOverweightOrigin = EnsureRoot<AccountId>;
}

parameter_types! {
	pub const Offset: u32 = 0;
	pub const MaxAuthorities: u32 = 200;
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

impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = MaxAuthorities;
}

parameter_types! {
	pub const LaunchPeriod: BlockNumber = 4 * DAYS;
	pub const VotingPeriod: BlockNumber = 4 * DAYS;
	pub const FastTrackVotingPeriod: BlockNumber = 3 * HOURS;
	pub const MinimumDeposit: Balance = 1 * UNIT;
	pub const EnactmentPeriod: BlockNumber = 4 * DAYS;
	pub const CooloffPeriod: BlockNumber = 4 * DAYS;
	pub const MaxProposals: u32 = 100;
}

impl pallet_democracy::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type EnactmentPeriod = EnactmentPeriod;
	type LaunchPeriod = LaunchPeriod;
	type VotingPeriod = VotingPeriod;
	type VoteLockingPeriod = EnactmentPeriod; // Same as EnactmentPeriod
	type MinimumDeposit = MinimumDeposit;
	/// A straight majority of the council can decide what their next motion is.
	type ExternalOrigin =
		pallet_collective::EnsureProportionAtLeast<AccountId, CouncilCollective, 1, 2>;
	/// A super-majority can have the next scheduled referendum be a straight majority-carries vote.
	type ExternalMajorityOrigin =
		pallet_collective::EnsureProportionAtLeast<AccountId, CouncilCollective, 1, 2>;
	/// A unanimous council can have the next scheduled referendum be a straight default-carries
	/// (NTB) vote.
	type ExternalDefaultOrigin =
		pallet_collective::EnsureProportionAtLeast<AccountId, CouncilCollective, 1, 1>;
	/// Two thirds of the technical committee can have an ExternalMajority/ExternalDefault vote
	/// be tabled immediately and with a shorter voting/enactment period.
	type FastTrackOrigin =
		pallet_collective::EnsureProportionAtLeast<AccountId, TechnicalCollective, 2, 3>;
	type InstantOrigin =
		pallet_collective::EnsureProportionAtLeast<AccountId, TechnicalCollective, 1, 1>;
	type InstantAllowed = frame_support::traits::ConstBool<true>;
	type FastTrackVotingPeriod = FastTrackVotingPeriod;
	// To cancel a proposal which has been passed, 2/3 of the council must agree to it.
	type CancellationOrigin =
		pallet_collective::EnsureProportionAtLeast<AccountId, CouncilCollective, 2, 3>;
	// To cancel a proposal before it has been passed, the technical committee must be unanimous or
	// Root must agree.
	type CancelProposalOrigin = EitherOfDiverse<
		EnsureRoot<AccountId>,
		pallet_collective::EnsureProportionAtLeast<AccountId, TechnicalCollective, 1, 1>,
	>;
	type BlacklistOrigin = EnsureRoot<AccountId>;
	// Any single technical committee member may veto a coming council proposal, however they can
	// only do it once and it lasts only for the cool-off period.
	type VetoOrigin = pallet_collective::EnsureMember<AccountId, TechnicalCollective>;
	type CooloffPeriod = CooloffPeriod;
	type Slash = ();
	type Scheduler = Scheduler;
	type PalletsOrigin = OriginCaller;
	type MaxVotes = ConstU32<100>;
	type WeightInfo = pallet_democracy::weights::SubstrateWeight<Runtime>;
	type MaxProposals = MaxProposals;
	type Preimages = Preimage;
	type MaxDeposits = ConstU32<100>;
	type MaxBlacklisted = ConstU32<100>;
}

parameter_types! {
	pub const CouncilMotionDuration: BlockNumber = 3 * DAYS;
	pub const CouncilMaxProposals: u32 = 100;
	pub const CouncilMaxMembers: u32 = 100;
}

type CouncilCollective = pallet_collective::Instance1;
impl pallet_collective::Config<CouncilCollective> for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type Proposal = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type MotionDuration = CouncilMotionDuration;
	type MaxProposals = CouncilMaxProposals;
	type MaxMembers = CouncilMaxMembers;
	type DefaultVote = pallet_collective::PrimeDefaultVote;
	type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const TechnicalMotionDuration: BlockNumber = 3 * DAYS;
	pub const TechnicalMaxProposals: u32 = 100;
	pub const TechnicalMaxMembers: u32 = 100;
}

type TechnicalCollective = pallet_collective::Instance2;
impl pallet_collective::Config<TechnicalCollective> for Runtime {
	type RuntimeOrigin = RuntimeOrigin;
	type Proposal = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type MotionDuration = TechnicalMotionDuration;
	type MaxProposals = TechnicalMaxProposals;
	type MaxMembers = TechnicalMaxMembers;
	type DefaultVote = pallet_collective::PrimeDefaultVote;
	type WeightInfo = pallet_collective::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub MaximumSchedulerWeight: Weight = Perbill::from_percent(80) *
		RuntimeBlockWeights::get().max_block;
}

impl pallet_scheduler::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeOrigin = RuntimeOrigin;
	type PalletsOrigin = OriginCaller;
	type RuntimeCall = RuntimeCall;
	type MaximumWeight = MaximumSchedulerWeight;
	type ScheduleOrigin = EnsureRoot<AccountId>;
	type MaxScheduledPerBlock = ConstU32<50>;
	type WeightInfo = pallet_scheduler::weights::SubstrateWeight<Runtime>;
	type OriginPrivilegeCmp = EqualPrivilegeOnly;
	type Preimages = Preimage;
}

parameter_types! {
	pub const PreimageMaxSize: u32 = 4096 * 1024;
	pub const PreimageBaseDeposit: Balance = 1 * UNIT;
	// One cent: $10,000 / MB
	pub const PreimageByteDeposit: Balance = 10 * MILLIUNIT;
}

impl pallet_preimage::Config for Runtime {
	type WeightInfo = pallet_preimage::weights::SubstrateWeight<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type ManagerOrigin = EnsureRoot<AccountId>;
	type BaseDeposit = PreimageBaseDeposit;
	type ByteDeposit = PreimageByteDeposit;
}

parameter_types! {
	pub const ProposalBond: Permill = Permill::from_percent(5);
	pub const ProposalBondMinimum: Balance = 10 * UNIT;
	pub const SpendPeriod: BlockNumber = 7 * DAYS;
	pub const Burn: Permill = Permill::from_percent(0);
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub const MaxApprovals: u32 = 100;
}

type TreasuryApproveOrigin = EitherOfDiverse<
	EnsureRoot<AccountId>,
	pallet_collective::EnsureProportionAtLeast<AccountId, CouncilCollective, 3, 5>,
>;

type TreasuryRejectOrigin = EitherOfDiverse<
	EnsureRoot<AccountId>,
	pallet_collective::EnsureProportionMoreThan<AccountId, CouncilCollective, 1, 2>,
>;

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
	type SpendFunds = Bounties;
	type WeightInfo = pallet_treasury::weights::SubstrateWeight<Runtime>;
	type MaxApprovals = MaxApprovals;
	type SpendOrigin = frame_support::traits::NeverEnsureOrigin<u128>;
}

parameter_types! {
	pub const BountyCuratorDeposit: Permill = Permill::from_percent(50);
	pub const BountyValueMinimum: Balance = 5 * UNIT;
	pub const BountyDepositBase: Balance = 1 * UNIT;
	pub const CuratorDepositMultiplier: Permill = Permill::from_percent(50);
	pub const CuratorDepositMin: Balance = 1 * UNIT;
	pub const CuratorDepositMax: Balance = 100 * UNIT;
	pub const DataDepositPerByte: Balance = 30 * MILLIUNIT;
	pub const BountyDepositPayoutDelay: BlockNumber = 4 * DAYS;
	pub const BountyUpdatePeriod: BlockNumber = 20 * DAYS;
	pub const MaximumReasonLength: u32 = 5000;
}

impl pallet_bounties::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BountyDepositBase = BountyDepositBase;
	type BountyDepositPayoutDelay = BountyDepositPayoutDelay;
	type BountyUpdatePeriod = BountyUpdatePeriod;
	type CuratorDepositMultiplier = CuratorDepositMultiplier;
	type CuratorDepositMin = CuratorDepositMin;
	type CuratorDepositMax = CuratorDepositMax;
	type BountyValueMinimum = BountyValueMinimum;
	type DataDepositPerByte = DataDepositPerByte;
	type MaximumReasonLength = MaximumReasonLength;
	type WeightInfo = pallet_bounties::weights::SubstrateWeight<Runtime>;
	type ChildBountyManager = ChildBounties;
}

parameter_types! {
	pub const ChildBountyValueMinimum: Balance = 1 * UNIT;
}

impl pallet_child_bounties::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MaxActiveChildBountyCount = ConstU32<10>;
	type ChildBountyValueMinimum = ChildBountyValueMinimum;
	type WeightInfo = pallet_child_bounties::weights::SubstrateWeight<Runtime>;
}

parameter_type_with_key! {
	pub ExistentialDeposits: |_currency_id: CurrencyId| -> Balance {
		NANOUNIT
	};
}

pub fn get_all_module_accounts() -> Vec<AccountId> {
	vec![Treasury::account_id()]
}

pub struct DustRemovalWhitelist;
impl Contains<AccountId> for DustRemovalWhitelist {
	fn contains(a: &AccountId) -> bool {
		get_all_module_accounts().contains(a)
	}
}

pub struct CurrencyHooks<T>(PhantomData<T>);
impl<T: orml_tokens::Config> MutationHooks<T::AccountId, T::CurrencyId, T::Balance>
	for CurrencyHooks<T>
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
	type Amount = primitives::Amount;
	type CurrencyId = CurrencyId;
	type WeightInfo = ();
	type ExistentialDeposits = ExistentialDeposits;
	type CurrencyHooks = CurrencyHooks<Runtime>;
	type MaxLocks = ConstU32<50>;
	type DustRemovalWhitelist = DustRemovalWhitelist;
	type MaxReserves = ConstU32<0>;
	type ReserveIdentifier = ReserveIdentifier;
}

parameter_types! {
	pub const NativeCurrencyId: CurrencyId = CurrencyId::Native;
	pub const DefaultWrappedCurrencyId: CurrencyId = WRAPPED_USDC_CURRENCY;
}

impl orml_currencies::Config for Runtime {
	type MultiCurrency = Tokens;
	type NativeCurrency = BasicCurrencyAdapter<Runtime, Balances, primitives::Amount, BlockNumber>;
	type GetNativeCurrencyId = NativeCurrencyId;
	type WeightInfo = ();
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
	type CurrencyBalance = Balance;

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

	const BLOCKS_PER_YEAR: BlockNumber = BLOCKS_PER_YEAR;
}

impl pallet_sudo::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
}

parameter_types! {
	pub const DepositBase: Balance = 300 * MILLIUNIT;
	pub const DepositFactor: Balance = 50 * MILLIUNIT;
}

impl pallet_multisig::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type DepositBase = DepositBase;
	type DepositFactor = DepositFactor;
	type MaxSignatories = ConstU32<20>;
	type WeightInfo = pallet_multisig::weights::SubstrateWeight<Runtime>;
}

impl pallet_utility::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = pallet_utility::weights::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const MinVestedTransfer: Balance = 0;
	pub UnvestedFundsAllowedWithdrawReasons: WithdrawReasons =
		WithdrawReasons::except(WithdrawReasons::TRANSFER | WithdrawReasons::RESERVE);
}

impl pallet_vesting::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type BlockNumberToBalance = ConvertInto;
	type MinVestedTransfer = MinVestedTransfer;
	type UnvestedFundsAllowedWithdrawReasons = UnvestedFundsAllowedWithdrawReasons;
	type WeightInfo = pallet_vesting::weights::SubstrateWeight<Runtime>;
	const MAX_VESTING_SCHEDULES: u32 = 10;
}

const fn deposit(items: u32, bytes: u32) -> Balance {
	(items as Balance * UNIT + (bytes as Balance) * (5 * MILLIUNIT / 100)) / 10
}

parameter_types! {
	pub const DepositPerItem: Balance = deposit(1, 0);
	pub const DepositPerByte: Balance = deposit(0, 1);
	pub const DeletionQueueDepth: u32 = 128;
	pub DeletionWeightLimit: Weight = RuntimeBlockWeights::get()
		.per_class
		.get(DispatchClass::Normal)
		.max_total
		.unwrap_or(RuntimeBlockWeights::get().max_block);
	pub Schedule: pallet_contracts::Schedule<Runtime> = Default::default();
}

use frame_support::{
	log::{error, trace},
	pallet_prelude::*,
	traits::fungibles::{
		approvals::{Inspect as AllowanceInspect, Mutate as AllowanceMutate},
		metadata::{Inspect as OtherInspectMetadata, Mutate as MetadataMutate},
		Create, Inspect, InspectMetadata, Mutate, MutateHold, Transfer,
	},
};
use sp_std::vec::Vec;

use pallet_contracts::chain_extension::{
	ChainExtension,
	Environment,
	Ext,
	InitState,
	RetVal,
	SysConfig,
	// UncheckedFrom,
};
use sp_core::crypto::UncheckedFrom;

// use sp_runtime::DispatchError;
use sp_runtime::{ArithmeticError, TokenError};
#[derive(Default)]
pub struct Psp22Extension;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
enum OriginType {
	Caller,
	Address,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
struct PalletAssetRequest {
	origin_type: OriginType,
	asset_id: u32,
	target_address: [u8; 32],
	amount: u128,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
struct PalletAssetBalanceRequest {
	asset_id: u32,
	address: [u8; 32],
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
pub enum ChainExnensionErr {
	/// Some error occurred.
	Other,
	/// Failed to lookup some data.
	CannotLookup,
	/// A bad origin.
	BadOrigin,
	/// A custom error in a module.
	Module,
	/// At least one consumer is remaining so the account cannot be destroyed.
	ConsumerRemaining,
	/// There are no providers so the account cannot be created.
	NoProviders,
	/// There are too many consumers so the account cannot be created.
	TooManyConsumers,
	/// An error to do with tokens.
	Token(PalletAssetTokenErr),
	/// An arithmetic error.
	Arithmetic(PalletAssetArithmeticErr),
	//unknown error
	Unknown,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
pub enum PalletAssetArithmeticErr {
	/// Underflow.
	Underflow,
	/// Overflow.
	Overflow,
	/// Division by zero.
	DivisionByZero,
	//unknown error
	Unknown,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
pub enum PalletAssetTokenErr {
	/// Funds are unavailable.
	NoFunds,
	/// Account that must exist would die.
	WouldDie,
	/// Account cannot exist with the funds that would be given.
	BelowMinimum,
	/// Account cannot be created.
	CannotCreate,
	/// The asset in question is unknown.
	UnknownAsset,
	/// Funds exist but are frozen.
	Frozen,
	/// Operation is not supported by the asset.
	Unsupported,
	//unknown error
	Unknown,
}

impl From<DispatchError> for ChainExnensionErr {
	fn from(e: DispatchError) -> Self {
		match e {
			DispatchError::Other(_) => ChainExnensionErr::Other,
			DispatchError::CannotLookup => ChainExnensionErr::CannotLookup,
			DispatchError::BadOrigin => ChainExnensionErr::BadOrigin,
			DispatchError::Module(_) => ChainExnensionErr::Module,
			DispatchError::ConsumerRemaining => ChainExnensionErr::ConsumerRemaining,
			DispatchError::NoProviders => ChainExnensionErr::NoProviders,
			DispatchError::TooManyConsumers => ChainExnensionErr::TooManyConsumers,
			DispatchError::Token(token_err) =>
				ChainExnensionErr::Token(PalletAssetTokenErr::from(token_err)),
			DispatchError::Arithmetic(arithmetic_error) =>
				ChainExnensionErr::Arithmetic(PalletAssetArithmeticErr::from(arithmetic_error)),
			_ => ChainExnensionErr::Unknown,
		}
	}
}

impl From<ArithmeticError> for PalletAssetArithmeticErr {
	fn from(e: ArithmeticError) -> Self {
		match e {
			ArithmeticError::Underflow => PalletAssetArithmeticErr::Underflow,
			ArithmeticError::Overflow => PalletAssetArithmeticErr::Overflow,
			ArithmeticError::DivisionByZero => PalletAssetArithmeticErr::DivisionByZero,
			_ => PalletAssetArithmeticErr::Unknown,
		}
	}
}

impl From<TokenError> for PalletAssetTokenErr {
	fn from(e: TokenError) -> Self {
		match e {
			TokenError::NoFunds => PalletAssetTokenErr::NoFunds,
			TokenError::WouldDie => PalletAssetTokenErr::WouldDie,
			TokenError::BelowMinimum => PalletAssetTokenErr::BelowMinimum,
			TokenError::CannotCreate => PalletAssetTokenErr::CannotCreate,
			TokenError::UnknownAsset => PalletAssetTokenErr::UnknownAsset,
			TokenError::Frozen => PalletAssetTokenErr::Frozen,
			TokenError::Unsupported => PalletAssetTokenErr::Unsupported,
			_ => PalletAssetTokenErr::Unknown,
		}
	}
}

fn try_from(type_id: u8, code: [u8; 12], issuer: [u8; 32]) -> Result<CurrencyId, ()> {
	match type_id {
		0 => {
			let foreign_currency_id = code[0];
			match foreign_currency_id {
				0 => Ok(CurrencyId::XCM(ForeignCurrencyId::KSM)),
				1 => Ok(CurrencyId::XCM(ForeignCurrencyId::KAR)),
				2 => Ok(CurrencyId::XCM(ForeignCurrencyId::AUSD)),
				3 => Ok(CurrencyId::XCM(ForeignCurrencyId::BNC)),
				4 => Ok(CurrencyId::XCM(ForeignCurrencyId::VsKSM)),
				5 => Ok(CurrencyId::XCM(ForeignCurrencyId::HKO)),
				6 => Ok(CurrencyId::XCM(ForeignCurrencyId::MOVR)),
				7 => Ok(CurrencyId::XCM(ForeignCurrencyId::SDN)),
				8 => Ok(CurrencyId::XCM(ForeignCurrencyId::KINT)),
				9 => Ok(CurrencyId::XCM(ForeignCurrencyId::KBTC)),
				10 => Ok(CurrencyId::XCM(ForeignCurrencyId::GENS)),
				11 => Ok(CurrencyId::XCM(ForeignCurrencyId::XOR)),
				12 => Ok(CurrencyId::XCM(ForeignCurrencyId::TEER)),
				13 => Ok(CurrencyId::XCM(ForeignCurrencyId::KILT)),
				14 => Ok(CurrencyId::XCM(ForeignCurrencyId::PHA)),
				15 => Ok(CurrencyId::XCM(ForeignCurrencyId::ZTG)),
				16 => Ok(CurrencyId::XCM(ForeignCurrencyId::USD)),
				_ => Err(()),
			}
		},
		1 => Ok(CurrencyId::Native),
		2 => Ok(CurrencyId::StellarNative),
		3 => {
			let code = [code[0], code[1], code[2], code[3]];
			Ok(CurrencyId::AlphaNum4 { code, issuer })
		},
		4 => Ok(CurrencyId::AlphaNum12 { code, issuer }),
		_ => Err(()),
	}
}

pub(crate) type BalanceOfForChainExt<T> =
	<<T as orml_currencies::Config>::MultiCurrency as orml_traits::MultiCurrency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

impl<T> ChainExtension<T> for Psp22Extension
where
	T: SysConfig
		+ orml_tokens::Config<CurrencyId = CurrencyId>
		+ pallet_contracts::Config
		+ orml_currencies::Config<MultiCurrency = Tokens, AccountId = AccountId>
		+ orml_tokens_allowance::Config,
	<T as SysConfig>::AccountId: UncheckedFrom<<T as SysConfig>::Hash> + AsRef<[u8]>,
{
	fn call<E: Ext>(&mut self, mut env: Environment<E, InitState>) -> Result<RetVal, DispatchError>
	where
		E: Ext<T = T>,
		<E::T as SysConfig>::AccountId: UncheckedFrom<<E::T as SysConfig>::Hash> + AsRef<[u8]>,
	{
		let func_id = env.func_id();

		error!("func_id : {}", func_id);

		match func_id {
			//transfer
			1105 => {
				let ext = env.ext();
				let address = ext.address().clone();
				let caller = ext.caller().clone();
				let mut env = env.buf_in_buf_out();
				let create_asset: (
					OriginType,
					u8,
					[u8; 12],
					[u8; 32],
					T::AccountId,
					BalanceOfForChainExt<T>,
				) = env.read_as()?;
				let (origin_id, type_id, code, issuer, account_id, balance) = create_asset;

				let address_account;
				if origin_id == OriginType::Caller {
					address_account = caller;
				} else {
					address_account = address;
				}

				error!("asset_id : {:#?}", type_id);
				error!("address_account : {:#?}", address_account);
				error!("account_id : {:#?}", account_id);
				error!("balance : {:#?}", balance);

				let currency_id = try_from(type_id, code, issuer).unwrap_or(CurrencyId::Native);
				let result = <orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::transfer(
					currency_id,
					&address_account,
					&account_id,
					balance,
				);

				error!("result : {:#?}", result);
			},

			//balance
			1106 => {
				let ext = env.ext();
				let mut env = env.buf_in_buf_out();
				let create_asset: (u8, [u8; 12], [u8; 32], T::AccountId) = env.read_as()?;
				let (type_id, code, issuer, account_id) = create_asset;

				let currency_id = try_from(type_id, code, issuer).unwrap_or(CurrencyId::Native);
				let balance =
					<orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::free_balance(
						currency_id,
						&account_id,
					);

				error!("asset_id : {:#?}", type_id);
				error!("account_id : {:#?}", account_id);
				error!("balance : {:#?}", balance);

				env.write(&balance.encode(), false, None)
					.map_err(|_| DispatchError::Other("ChainExtension failed to call balance"))?;
			},

			//total_supply
			1107 => {
				let mut env = env.buf_in_buf_out();
				let create_asset: (u8, [u8; 12], [u8; 32], T::AccountId) = env.read_as()?;
				let (type_id, code, issuer, account_id) = create_asset;

				let currency_id = try_from(type_id, code, issuer).unwrap_or(CurrencyId::Native);

				let total_supply =
					<orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::total_issuance(
						currency_id,
					);

				env.write(&total_supply.encode(), false, None).map_err(|_| {
					DispatchError::Other("ChainExtension failed to call total_supply")
				})?;
			},

			//approve_transfer
			1108 => {
				let ext = env.ext();
				let address = ext.address().clone();
				let caller = ext.caller().clone();
				let mut env = env.buf_in_buf_out();
				let create_asset: (
					OriginType,
					u8,
					[u8; 12],
					[u8; 32],
					T::AccountId,
					BalanceOfForChainExt<T>,
				) = env.read_as()?;
				let (origin_type, type_id, code, issuer, to, amount) = create_asset;

				let from;
				if origin_type == OriginType::Caller {
					error!("OriginType::Caller");
					from = caller;
				} else {
					error!("OriginType::Address");
					from = address;
				}

				error!("from : {:#?}", from);
				error!("origin_type : {:#?}", origin_type);
				error!("to : {:#?}", to);
				error!("amount : {:#?}", amount);

				let currency_id = try_from(type_id, code, issuer).unwrap_or(CurrencyId::Native);

				let result = orml_tokens_allowance::Pallet::<T>::do_approve_transfer(
					currency_id,
					&from,
					&to,
					amount,
				);

				error!("result : {:#?}", result);

				match result {
					DispatchResult::Ok(_) => {},
					DispatchResult::Err(e) => {
						let err = Result::<(), ChainExnensionErr>::Err(ChainExnensionErr::from(e));
						env.write(&err.encode(), false, None).map_err(|_| {
							error!("ChainExtension failed to call 'approve'");
							DispatchError::Other("ChainExtension failed to call 'approve'")
						})?;
					},
				}
			},

			// //transfer_approved
			1109 => {
				let ext = env.ext();
				let address = ext.address().clone();
				let caller = ext.caller().clone();
				let mut env = env.buf_in_buf_out();
				let create_asset: (
					T::AccountId,
					(OriginType, u8, [u8; 12], [u8; 32], T::AccountId, BalanceOfForChainExt<T>),
				) = env.read_as()?;
				let owner = create_asset.0;
				let (origin_type, type_id, code, issuer, to, amount) = create_asset.1;

				let from;
				if origin_type == OriginType::Caller {
					from = caller;
				} else {
					from = address;
				}

				error!("from : {:#?}", from);
				error!("owner : {:#?}", owner);
				error!("origin_type : {:#?}", origin_type);
				error!("to : {:#?}", to);
				error!("amount : {:#?}", amount);

				let currency_id = try_from(type_id, code, issuer).unwrap_or(CurrencyId::Native);

				let result = orml_tokens_allowance::Pallet::<T>::do_transfer_approved(
					currency_id,
					&owner,
					&from,
					&to,
					amount,
				);

				error!("transfer_from : {:#?}", result);

				match result {
					DispatchResult::Ok(_) => {},
					DispatchResult::Err(e) => {
						let err = Result::<(), ChainExnensionErr>::Err(ChainExnensionErr::from(e));
						env.write(&err.encode(), false, None).map_err(|_| {
							DispatchError::Other(
								"ChainExtension failed to call 'approved transfer'",
							)
						})?;
					},
				}
			},

			//allowance
			1110 => {
				let mut env = env.buf_in_buf_out();
				let allowance_request: (u8, [u8; 12], [u8; 32], T::AccountId, T::AccountId) =
					env.read_as()?;

				let currency_id =
					try_from(allowance_request.0, allowance_request.1, allowance_request.2)
						.unwrap_or(CurrencyId::Native);

				let allowance = orml_tokens_allowance::Pallet::<T>::allowance(
					currency_id,
					&allowance_request.3,
					&allowance_request.4,
				);
				error!("allowance_request : {:#?}", allowance_request);
				error!("allowance : {:#?}", allowance);

				env.write(&allowance.encode(), false, None)
					.map_err(|_| DispatchError::Other("ChainExtension failed to call balance"))?;
			},

			//TODO perhaps we need this functionality. if not. will remove it.
			//increase_allowance/decrease_allowance
			// 1111 => {
			// 	use frame_support::dispatch::DispatchResult;
			//     let mut env = env.buf_in_buf_out();
			//     let request: (u32, [u8; 32], [u8; 32], u128, bool) = env.read_as()?;
			// 	let (asset_id, owner, delegate, amount, is_increase) = request;
			// 	let mut vec = &owner.to_vec()[..];
			// 	let owner_address = AccountId::decode(&mut vec).unwrap();
			// 	let mut vec = &delegate.to_vec()[..];
			// 	let delegate_address = AccountId::decode(&mut vec).unwrap();

			// 	use crate::sp_api_hidden_includes_construct_runtime::hidden_include::traits::fungibles::approvals::Inspect;
			//     let allowance :u128 = Assets::allowance(asset_id, &owner_address, &delegate_address);
			// 	let new_allowance =
			// 	if is_increase {allowance + amount}
			// 	else {
			// 		if allowance < amount  { 0 }
			// 		else {allowance - amount}
			// 	};
			// 	let cancel_approval_result = pallet_assets::Pallet::<Runtime>::
			// 	cancel_approval(Origin::signed(owner_address.clone()),
			// 	asset_id,
			// 	MultiAddress::Id(delegate_address.clone()));
			// 	match cancel_approval_result {
			// 		DispatchResult::Ok(_) => {
			// 			error!("OK cancel_approval")
			// 		}
			// 		DispatchResult::Err(e) => {
			// 			error!("ERROR cancel_approval");
			// 			error!("{:#?}", e);
			// 			let err = Result::<(),PalletAssetErr>::Err(PalletAssetErr::from(e));
			// 			env.write(&err.encode(), false, None).map_err(|_| {
			// 				DispatchError::Other("ChainExtension failed to call 'approve transfer'")
			// 			})?;
			// 		}
			// 	}
			// 	if cancel_approval_result.is_ok(){
			// 		let approve_transfer_result = pallet_assets::Pallet::<Runtime>::
			// 		approve_transfer(Origin::signed(owner_address),
			// 		asset_id,
			// 		MultiAddress::Id(delegate_address),
			// 		new_allowance);
			// 		error!("old allowance {}", allowance);
			// 		error!("new allowance {}", new_allowance);
			// 		error!("increase_allowance input {:#?}", request);
			// 		error!("increase_allowance output {:#?}", approve_transfer_result);
			// 		match approve_transfer_result {
			// 			DispatchResult::Ok(_) => {
			// 				error!("OK increase_allowance")
			// 			}
			// 			DispatchResult::Err(e) => {
			// 				error!("ERROR increase_allowance");
			// 				error!("{:#?}", e);
			// 				let err = Result::<(),PalletAssetErr>::Err(PalletAssetErr::from(e));
			// 				env.write(&err.encode(), false, None).map_err(|_| {
			// 					DispatchError::Other("ChainExtension failed to call 'approve transfer'")
			// 				})?;
			// 			}
			// 		}
			// 	}
			// }

			//TODO
			7777 => {
				error!("Called an dia oracle `func_id`: {:}", func_id);
				return Err(DispatchError::Other("Unimplemented dia oracle func_id"))
			},
			_ => {
				error!("Called an unregistered `func_id`: {:}", func_id);
				return Err(DispatchError::Other("Unimplemented func_id"))
			},
		}

		Ok(RetVal::Converging(0))
	}

	fn enabled() -> bool {
		true
	}
}

/*
		#[ink(message,selector = 0x70a08231)]
		pub fn balance(&self, account : AccountId) -> [u128; 2] {
			let b = self.balance_of(account);
			use ethnum::U256;
			let balance_u256: U256 = U256::try_from(b).unwrap();
			balance_u256.0
		}
		#[ink(message,selector = 0x23b872dd)]
		pub fn transfertransferfrom(&mut self, from : AccountId, to : AccountId, amount : [u128; 2]) {
			use ethnum::U256;
			let amount : u128 = U256(amount).try_into().unwrap();
			self.transfer_from(from, to, amount, Vec::<u8>::new()).expect("should transfer from");
		}
		#[ink(message,selector = 0xa9059cbb)]
		pub fn transfertransfer(&mut self, to : AccountId, amount : [u128; 2]) {
			use ethnum::U256;
			let amount : u128 = U256(amount).try_into().unwrap();
			self.transfer(to, amount, Vec::<u8>::new()).expect("should transfer");
		}
*/

/*____________________________________________________________________________________________________*/

impl pallet_contracts::Config for Runtime {
	type Time = Timestamp;
	type Randomness = RandomnessCollectiveFlip;
	type Currency = Balances;
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type CallFilter = frame_support::traits::Nothing;
	type DepositPerItem = DepositPerItem;
	type DepositPerByte = DepositPerByte;
	type CallStack = [pallet_contracts::Frame<Self>; 31];
	type WeightPrice = pallet_transaction_payment::Pallet<Self>;
	type WeightInfo = pallet_contracts::weights::SubstrateWeight<Self>;
	// type ChainExtension = ();
	type ChainExtension = Psp22Extension;
	type DeletionQueueDepth = DeletionQueueDepth;
	type DeletionWeightLimit = DeletionWeightLimit;
	type Schedule = Schedule;
	type AddressGenerator = pallet_contracts::DefaultAddressGenerator;
	type MaxCodeLen = ConstU32<{ 128 * 1024 }>;
	type MaxStorageKeyLen = ConstU32<128>;
	type UnsafeUnstableInterface = ConstBool<true>;
	type MaxDebugBufferLen = ConstU32<{ 2 * 1024 * 1024 }>;
}

impl pallet_randomness_collective_flip::Config for Runtime {}

impl orml_tokens_allowance::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
}

parameter_types! {
	pub const BasicDeposit: Balance = 10 * UNIT;       // 258 bytes on-chain
	pub const FieldDeposit: Balance = 25 * MILLIUNIT;  // 66 bytes on-chain
	pub const SubAccountDeposit: Balance = 2 * UNIT;   // 53 bytes on-chain
	pub const MaxSubAccounts: u32 = 100;
	pub const MaxAdditionalFields: u32 = 100;
	pub const MaxRegistrars: u32 = 20;
}

type EnsureRootOrHalfCouncil = EitherOfDiverse<
	EnsureRoot<AccountId>,
	pallet_collective::EnsureProportionMoreThan<AccountId, CouncilCollective, 1, 2>,
>;

impl pallet_identity::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type BasicDeposit = BasicDeposit;
	type FieldDeposit = FieldDeposit;
	type SubAccountDeposit = SubAccountDeposit;
	type MaxSubAccounts = MaxSubAccounts;
	type MaxAdditionalFields = MaxAdditionalFields;
	type MaxRegistrars = MaxRegistrars;
	type Slashed = Treasury;
	type ForceOrigin = EnsureRootOrHalfCouncil;
	type RegistrarOrigin = EnsureRootOrHalfCouncil;
	type WeightInfo = pallet_identity::weights::SubstrateWeight<Runtime>;
}

impl dia_oracle::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type AuthorityId = dia_oracle::crypto::DiaAuthId;
	type WeightInfo = dia_oracle::weights::DiaWeightInfo<Runtime>;
}

impl frame_system::offchain::SigningTypes for Runtime {
	type Public = <Signature as sp_runtime::traits::Verify>::Signer;
	type Signature = Signature;
}

impl<LocalCall> frame_system::offchain::CreateSignedTransaction<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_transaction<C: frame_system::offchain::AppCrypto<Self::Public, Self::Signature>>(
		call: RuntimeCall,
		public: <Signature as sp_runtime::traits::Verify>::Signer,
		account: AccountId,
		index: Index,
	) -> Option<(
		RuntimeCall,
		<UncheckedExtrinsic as sp_runtime::traits::Extrinsic>::SignaturePayload,
	)> {
		let period = BlockHashCount::get() as u64;
		let current_block = System::block_number().saturated_into::<u64>().saturating_sub(1);
		let tip = 0;
		let extra: SignedExtra = (
			frame_system::CheckNonZeroSender::<Runtime>::new(),
			frame_system::CheckSpecVersion::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckEra::<Runtime>::from(generic::Era::mortal(period, current_block)),
			frame_system::CheckNonce::<Runtime>::from(index),
			frame_system::CheckWeight::<Runtime>::new(),
			pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
		);

		let raw_payload = SignedPayload::new(call, extra).ok()?;
		let signature = raw_payload.using_encoded(|payload| C::sign(payload, public))?;
		let address = account;
		let (call, extra, _) = raw_payload.deconstruct();
		Some((call, (sp_runtime::MultiAddress::Id(address), signature.into(), extra)))
	}
}

pub struct CurrencyConvert;
impl currency::CurrencyConversion<currency::Amount<Runtime>, CurrencyId> for CurrencyConvert {
	fn convert(
		amount: &currency::Amount<Runtime>,
		to: CurrencyId,
	) -> Result<currency::Amount<Runtime>, DispatchError> {
		Oracle::convert(amount, to)
	}
}

parameter_types! {
	pub const RelayChainCurrencyId: CurrencyId = XCM(ForeignCurrencyId::KSM);
}
impl currency::Config for Runtime {
	type UnsignedFixedPoint = UnsignedFixedPoint;
	type SignedInner = SignedInner;
	type SignedFixedPoint = SignedFixedPoint;
	type Balance = Balance;
	type GetNativeCurrencyId = NativeCurrencyId;
	type GetRelayChainCurrencyId = RelayChainCurrencyId;
	type AssetConversion = primitives::AssetConversion;
	type BalanceConversion = primitives::BalanceConversion;
	type CurrencyConversion = CurrencyConvert;
}

impl security::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = security::SubstrateWeight<Runtime>;
}

impl staking::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SignedInner = SignedInner;
	type SignedFixedPoint = SignedFixedPoint;
	type GetNativeCurrencyId = NativeCurrencyId;
	type CurrencyId = CurrencyId;
}

impl oracle::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = oracle::SubstrateWeight<Runtime>;
	type DataProvider = DataProviderImpl;
}

parameter_types! {
	pub const OrganizationLimit: u32 = 255;
	pub const ValidatorLimit: u32 = 255;
}

impl stellar_relay::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OrganizationId = u128;
	type OrganizationLimit = OrganizationLimit;
	type ValidatorLimit = ValidatorLimit;
	type WeightInfo = ();
}

impl reward::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SignedFixedPoint = SignedFixedPoint;
	type RewardId = VaultId;
	type CurrencyId = CurrencyId;
	type GetNativeCurrencyId = NativeCurrencyId;
}

parameter_types! {
	pub const FeePalletId: PalletId = PalletId(*b"mod/fees");
	pub const VaultRegistryPalletId: PalletId = PalletId(*b"mod/vreg");

	pub const MaxExpectedValue: UnsignedFixedPoint = UnsignedFixedPoint::from_inner(<UnsignedFixedPoint as FixedPointNumber>::DIV);
	pub FeeAccount: AccountId = FeePalletId::get().into_account_truncating();
}

impl fee::Config for Runtime {
	type FeePalletId = FeePalletId;
	type WeightInfo = fee::SubstrateWeight<Runtime>;
	type SignedFixedPoint = SignedFixedPoint;
	type SignedInner = SignedInner;
	type UnsignedFixedPoint = UnsignedFixedPoint;
	type UnsignedInner = UnsignedInner;
	type VaultRewards = VaultRewards;
	type VaultStaking = VaultStaking;
	type OnSweep = currency::SweepFunds<Runtime, FeeAccount>;
	type MaxExpectedValue = MaxExpectedValue;
}

impl vault_registry::Config for Runtime {
	type PalletId = VaultRegistryPalletId;
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type WeightInfo = vault_registry::SubstrateWeight<Runtime>;
	type GetGriefingCollateralCurrencyId = NativeCurrencyId;
}

impl redeem::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = redeem::SubstrateWeight<Runtime>;
}

pub struct BlockNumberToBalance;

impl sp_runtime::traits::Convert<BlockNumber, Balance> for BlockNumberToBalance {
	fn convert(a: BlockNumber) -> Balance {
		a.into()
	}
}

impl issue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type BlockNumberToBalance = BlockNumberToBalance;
	type WeightInfo = issue::SubstrateWeight<Runtime>;
}

impl nomination::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = nomination::SubstrateWeight<Runtime>;
}

impl replace::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = replace::SubstrateWeight<Runtime>;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
where
	RuntimeCall: From<C>,
{
	type Extrinsic = UncheckedExtrinsic;
	type OverarchingCall = RuntimeCall;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = opaque::Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		// System support stuff.
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>} = 0,
		ParachainSystem: cumulus_pallet_parachain_system::{
			Pallet, Call, Config, Storage, Inherent, Event<T>, ValidateUnsigned,
		} = 1,
		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent} = 2,
		ParachainInfo: parachain_info::{Pallet, Storage, Config} = 3,

		// Monetary stuff.
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>} = 10,
		TransactionPayment: pallet_transaction_payment::{Pallet, Storage, Event<T>} = 11,

		// Governance
		Sudo: pallet_sudo::{Pallet, Call, Storage, Config<T>, Event<T>} = 12,
		Democracy: pallet_democracy::{Pallet, Call, Storage, Config<T>, Event<T>} = 13,
		Council: pallet_collective::<Instance1>::{Pallet, Call, Storage, Config<T>, Origin<T>, Event<T>} = 14,
		TechnicalCommittee: pallet_collective::<Instance2>::{Pallet, Call, Storage, Config<T>, Origin<T>,  Event<T>} = 15,
		Scheduler: pallet_scheduler::{Pallet, Call, Storage, Event<T>} = 16,
		Preimage: pallet_preimage::{Pallet, Call, Storage, Event<T>} = 17,
		Multisig: pallet_multisig::{Pallet, Call, Storage, Event<T>} = 18,
		Treasury: pallet_treasury::{Pallet, Call, Storage, Event<T>} = 19,
		Bounties: pallet_bounties::{Pallet, Call, Storage, Event<T>} = 20,
		ChildBounties: pallet_child_bounties::{Pallet, Call, Storage, Event<T>} = 21,

		// Collator support. The order of these 4 are important and shall not change.
		Authorship: pallet_authorship::{Pallet, Call, Storage} = 30,
		Session: pallet_session::{Pallet, Call, Storage, Event, Config<T>} = 32,
		Aura: pallet_aura::{Pallet, Storage, Config<T>} = 33,
		AuraExt: cumulus_pallet_aura_ext::{Pallet, Storage, Config} = 34,
		ParachainStaking: parachain_staking::{Pallet, Call, Storage, Event<T>, Config<T>} = 35,

		// XCM helpers.
		XcmpQueue: cumulus_pallet_xcmp_queue::{Pallet, Call, Storage, Event<T>} = 40,
		PolkadotXcm: pallet_xcm::{Pallet, Call, Event<T>, Origin, Config} = 41,
		CumulusXcm: cumulus_pallet_xcm::{Pallet, Event<T>, Origin} = 42,
		DmpQueue: cumulus_pallet_dmp_queue::{Pallet, Call, Storage, Event<T>} = 43,

		// Amendments
		Vesting: pallet_vesting::{Pallet, Call, Storage, Event<T>} = 50,
		Utility: pallet_utility::{Pallet, Call, Event} = 51,
		Currencies: orml_currencies::{Pallet, Call, Storage} = 52,
		Tokens: orml_tokens::{Pallet, Call, Storage, Config<T>, Event<T>} = 53,
		XTokens: orml_xtokens::{Pallet, Storage, Call, Event<T>} = 54,
		Identity: pallet_identity::{Pallet, Storage, Call, Event<T>} = 55,
		Contracts: pallet_contracts::{Pallet, Storage, Call, Event<T>} = 56,
		RandomnessCollectiveFlip: pallet_randomness_collective_flip::{Pallet, Storage} = 57,
		DiaOracleModule: dia_oracle::{Pallet, Storage, Call, Config<T>, Event<T>} = 58,

		ZenlinkProtocol: zenlink_protocol::{Pallet, Call, Storage, Event<T>}  = 59,

		// Spacewalk pallets
		Currency: currency::{Pallet} = 60,
		Fee: fee::{Pallet, Call, Config<T>, Storage} = 61,
		Issue: issue::{Pallet, Call, Config<T>, Storage, Event<T>} = 62,
		Nomination: nomination::{Pallet, Call, Config, Storage, Event<T>} = 63,
		Oracle: oracle::{Pallet, Call, Config, Storage, Event<T>} = 64,
		Redeem: redeem::{Pallet, Call, Config<T>, Storage, Event<T>} = 65,
		Replace: replace::{Pallet, Call, Config<T>, Storage, Event<T>} = 66,
		Security: security::{Pallet, Call, Config, Storage, Event<T>} = 67,
		StellarRelay: stellar_relay::{Pallet, Call, Config<T>, Storage, Event<T>} = 68,
		VaultRegistry: vault_registry::{Pallet, Call, Config<T>, Storage, Event<T>, ValidateUnsigned} = 69,
		VaultRewards: reward::{Pallet, Call, Storage, Event<T>} = 70,
		VaultStaking: staking::{Pallet, Storage, Event<T>} = 71,
		TokenAllowance: orml_tokens_allowance::{Pallet, Storage, Call, Event<T>} = 72,
	}
);

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
extern crate frame_benchmarking;

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	define_benchmarks!(
		[frame_system, SystemBench::<Runtime>]
		[pallet_balances, Balances]
		[pallet_session, SessionBench::<Runtime>]
		[pallet_timestamp, Timestamp]
		[cumulus_pallet_xcmp_queue, XcmpQueue]

		[fee, Fee]
		[issue, Issue]
		[nomination, Nomination]
		[oracle, Oracle]
		[redeem, Redeem]
		[replace, Replace]
		[stellar_relay, StellarRelay]
		[vault_registry, VaultRegistry]
	);
}

impl_runtime_apis! {
	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
		}

		fn authorities() -> Vec<AuraId> {
			Aura::authorities().into_inner()
		}
	}

	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			Executive::execute_block(block)
		}

		fn initialize_block(header: &<Block as BlockT>::Header) {
			Executive::initialize_block(header)
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			OpaqueMetadata::new(Runtime::metadata().into())
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
			Executive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> <Block as BlockT>::Header {
			Executive::finalize_block()
		}

		fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
			block_hash: <Block as BlockT>::Hash,
		) -> TransactionValidity {
			Executive::validate_transaction(source, tx, block_hash)
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	impl sp_session::SessionKeys<Block> for Runtime {
		fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
			SessionKeys::generate(seed)
		}

		fn decode_session_keys(
			encoded: Vec<u8>,
		) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
			SessionKeys::decode_into_raw_public_keys(&encoded)
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Index> for Runtime {
		fn account_nonce(account: AccountId) -> Index {
			System::account_nonce(account)
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
		fn query_info(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_info(uxt, len)
		}
		fn query_fee_details(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			TransactionPayment::query_fee_details(uxt, len)
		}
	}

	impl cumulus_primitives_core::CollectCollationInfo<Block> for Runtime {
		fn collect_collation_info(header: &<Block as BlockT>::Header) -> cumulus_primitives_core::CollationInfo {
			ParachainSystem::collect_collation_info(header)
		}
	}

	impl parachain_staking::runtime_api::ParachainStakingApi<Block, AccountId, Balance> for Runtime {
		fn get_unclaimed_staking_rewards(account: &AccountId) -> Balance {
			ParachainStaking::get_unclaimed_staking_rewards(account)
		}

		fn get_staking_rates() -> parachain_staking::runtime_api::StakingRates {
			ParachainStaking::get_staking_rates()
		}
	}

	impl dia_oracle_runtime_api::DiaOracleApi<Block> for Runtime{
		fn get_value(blockchain: frame_support::sp_std::vec::Vec<u8>, symbol: frame_support::sp_std::vec::Vec<u8>)-> Result<dia_oracle_runtime_api::PriceInfo, sp_runtime::DispatchError>{
			DiaOracleModule::get_value(blockchain, symbol)
		}

		fn get_coin_info(blockchain: frame_support::sp_std::vec::Vec<u8>, symbol: frame_support::sp_std::vec::Vec<u8>)-> Result<dia_oracle_runtime_api::CoinInfo,sp_runtime::DispatchError>{
			DiaOracleModule::get_coin_info(blockchain, symbol)
		}
	}

	// zenlink runtime outer apis
	impl zenlink_protocol_runtime_api::ZenlinkProtocolApi<Block, AccountId, ZenlinkAssetId> for Runtime {

		fn get_balance(
			asset_id: ZenlinkAssetId,
			owner: AccountId
		) -> AssetBalance {
			<Runtime as zenlink_protocol::Config>::MultiAssetsHandler::balance_of(asset_id, &owner)
		}

		fn get_sovereigns_info(
			asset_id: ZenlinkAssetId
		) -> Vec<(u32, AccountId, AssetBalance)> {
			ZenlinkProtocol::get_sovereigns_info(&asset_id)
		}

		fn get_pair_by_asset_id(
			asset_0: ZenlinkAssetId,
			asset_1: ZenlinkAssetId
		) -> Option<PairInfo<AccountId, AssetBalance, ZenlinkAssetId>> {
			ZenlinkProtocol::get_pair_by_asset_id(asset_0, asset_1)
		}

		fn get_amount_in_price(
			supply: AssetBalance,
			path: Vec<ZenlinkAssetId>
		) -> AssetBalance {
			ZenlinkProtocol::desired_in_amount(supply, path)
		}

		fn get_amount_out_price(
			supply: AssetBalance,
			path: Vec<ZenlinkAssetId>
		) -> AssetBalance {
			ZenlinkProtocol::supply_out_amount(supply, path)
		}

		fn get_estimate_lptoken(
			token_0: ZenlinkAssetId,
			token_1: ZenlinkAssetId,
			amount_0_desired: AssetBalance,
			amount_1_desired: AssetBalance,
			amount_0_min: AssetBalance,
			amount_1_min: AssetBalance,
		) -> AssetBalance{
			ZenlinkProtocol::get_estimate_lptoken(
				token_0,
				token_1,
				amount_0_desired,
				amount_1_desired,
				amount_0_min,
				amount_1_min
			)
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade() -> (Weight, Weight) {
			log::info!("try-runtime::on_runtime_upgrade foucoco.");
			let weight = Executive::try_runtime_upgrade().unwrap();
			(weight, RuntimeBlockWeights::get().max_block)
		}

		fn execute_block_no_check(block: Block) -> Weight {
			Executive::execute_block_no_check(block)
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(extra: bool) -> (
			Vec<frame_benchmarking::BenchmarkList>,
			Vec<frame_support::traits::StorageInfo>,
		) {
			use frame_benchmarking::{Benchmarking, BenchmarkList};
			use frame_support::traits::StorageInfoTrait;
			use frame_system_benchmarking::Pallet as SystemBench;
			use cumulus_pallet_session_benchmarking::Pallet as SessionBench;

			let mut list = Vec::<BenchmarkList>::new();
			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();
			return (list, storage_info)
		}

		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{Benchmarking, BenchmarkBatch, TrackedStorageKey};

			use frame_system_benchmarking::Pallet as SystemBench;
			impl frame_system_benchmarking::Config for Runtime {}

			use cumulus_pallet_session_benchmarking::Pallet as SessionBench;
			impl cumulus_pallet_session_benchmarking::Config for Runtime {}

			let whitelist: Vec<TrackedStorageKey> = vec![
				// Block Number
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac").to_vec().into(),
				// Total Issuance
				hex_literal::hex!("c2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80").to_vec().into(),
				// Execution Phase
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a").to_vec().into(),
				// Event Count
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850").to_vec().into(),
				// System Events
				hex_literal::hex!("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7").to_vec().into(),
			];

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);
			add_benchmarks!(params, batches);

			if batches.is_empty() { return Err("Benchmark not found for this pallet.".into()) }
			Ok(batches)
		}
	}

	impl module_issue_rpc_runtime_api::IssueApi<
		Block,
		AccountId,
		H256,
		IssueRequest<AccountId, BlockNumber, Balance, CurrencyId>
	> for Runtime {
		fn get_issue_requests(account_id: AccountId) -> Vec<H256> {
			Issue::get_issue_requests_for_account(account_id)
		}

		fn get_vault_issue_requests(vault_id: AccountId) -> Vec<H256> {
			Issue::get_issue_requests_for_vault(vault_id)
		}
	}

	impl module_vault_registry_rpc_runtime_api::VaultRegistryApi<
		Block,
		VaultId,
		Balance,
		UnsignedFixedPoint,
		CurrencyId,
		AccountId,
	> for Runtime {
		fn get_vault_collateral(vault_id: VaultId) -> Result<BalanceWrapper<Balance>, DispatchError> {
			let result = VaultRegistry::compute_collateral(&vault_id)?;
			Ok(BalanceWrapper{amount:result.amount()})
		}

		fn get_vaults_by_account_id(account_id: AccountId) -> Result<Vec<VaultId>, DispatchError> {
			VaultRegistry::get_vaults_by_account_id(account_id)
		}

		fn get_vault_total_collateral(vault_id: VaultId) -> Result<BalanceWrapper<Balance>, DispatchError> {
			let result = VaultRegistry::get_backing_collateral(&vault_id)?;
			Ok(BalanceWrapper{amount:result.amount()})
		}

		fn get_premium_redeem_vaults() -> Result<Vec<(VaultId, BalanceWrapper<Balance>)>, DispatchError> {
			let result = VaultRegistry::get_premium_redeem_vaults()?;
			Ok(result.iter().map(|v| (v.0.clone(), BalanceWrapper{amount:v.1.amount()})).collect())
		}

		fn get_vaults_with_issuable_tokens() -> Result<Vec<(VaultId, BalanceWrapper<Balance>)>, DispatchError> {
			let result = VaultRegistry::get_vaults_with_issuable_tokens()?;
			Ok(result.into_iter().map(|v| (v.0, BalanceWrapper{amount:v.1.amount()})).collect())
		}

		fn get_vaults_with_redeemable_tokens() -> Result<Vec<(VaultId, BalanceWrapper<Balance>)>, DispatchError> {
			let result = VaultRegistry::get_vaults_with_redeemable_tokens()?;
			Ok(result.into_iter().map(|v| (v.0, BalanceWrapper{amount:v.1.amount()})).collect())
		}

		fn get_issuable_tokens_from_vault(vault: VaultId) -> Result<BalanceWrapper<Balance>, DispatchError> {
			let result = VaultRegistry::get_issuable_tokens_from_vault(&vault)?;
			Ok(BalanceWrapper{amount:result.amount()})
		}

		fn get_collateralization_from_vault(vault: VaultId, only_issued: bool) -> Result<UnsignedFixedPoint, DispatchError> {
			VaultRegistry::get_collateralization_from_vault(vault, only_issued)
		}

		fn get_collateralization_from_vault_and_collateral(vault: VaultId, collateral: BalanceWrapper<Balance>, only_issued: bool) -> Result<UnsignedFixedPoint, DispatchError> {
			let amount = Amount::new(collateral.amount, vault.collateral_currency());
			VaultRegistry::get_collateralization_from_vault_and_collateral(vault, &amount, only_issued)
		}

		fn get_required_collateral_for_wrapped(amount_wrapped: BalanceWrapper<Balance>, currency_id: CurrencyId) -> Result<BalanceWrapper<Balance>, DispatchError> {
			let amount_wrapped = Amount::new(amount_wrapped.amount, DefaultWrappedCurrencyId::get());
			let result = VaultRegistry::get_required_collateral_for_wrapped(&amount_wrapped, currency_id)?;
			Ok(BalanceWrapper{amount:result.amount()})
		}

		fn get_required_collateral_for_vault(vault_id: VaultId) -> Result<BalanceWrapper<Balance>, DispatchError> {
			let result = VaultRegistry::get_required_collateral_for_vault(vault_id)?;
			Ok(BalanceWrapper{amount:result.amount()})
		}
	}

	impl module_redeem_rpc_runtime_api::RedeemApi<
		Block,
		AccountId,
		H256,
		RedeemRequest<AccountId, BlockNumber, Balance, CurrencyId>
	> for Runtime {
		fn get_redeem_requests(account_id: AccountId) -> Vec<H256> {
			Redeem::get_redeem_requests_for_account(account_id)
		}

		fn get_vault_redeem_requests(vault_account_id: AccountId) -> Vec<H256> {
			Redeem::get_redeem_requests_for_vault(vault_account_id)
		}
	}

	impl module_replace_rpc_runtime_api::ReplaceApi<
		Block,
		AccountId,
		H256,
		ReplaceRequest<AccountId, BlockNumber, Balance, CurrencyId>
	> for Runtime {
		fn get_old_vault_replace_requests(vault_id: AccountId) -> Vec<H256> {
			Replace::get_replace_requests_for_old_vault(vault_id)
		}

		fn get_new_vault_replace_requests(vault_id: AccountId) -> Vec<H256> {
			Replace::get_replace_requests_for_new_vault(vault_id)
		}
	}

	impl pallet_contracts::ContractsApi<Block, AccountId, Balance, BlockNumber, Hash>
		for Runtime
	{
		fn call(
			origin: AccountId,
			dest: AccountId,
			value: Balance,
			gas_limit: Option<Weight>,
			storage_deposit_limit: Option<Balance>,
			input_data: Vec<u8>,
		) -> pallet_contracts_primitives::ContractExecResult<Balance> {
			let gas_limit = gas_limit.unwrap_or(RuntimeBlockWeights::get().max_block);
			Contracts::bare_call(
				origin,
				dest,
				value,
				gas_limit,
				storage_deposit_limit,
				input_data,
				CONTRACTS_DEBUG_OUTPUT,
				pallet_contracts::Determinism::Deterministic,
			)
		}

		fn instantiate(
			origin: AccountId,
			value: Balance,
			gas_limit: Option<Weight>,
			storage_deposit_limit: Option<Balance>,
			code: pallet_contracts_primitives::Code<Hash>,
			data: Vec<u8>,
			salt: Vec<u8>,
		) -> pallet_contracts_primitives::ContractInstantiateResult<AccountId, Balance>
		{
			let gas_limit = gas_limit.unwrap_or(RuntimeBlockWeights::get().max_block);
			Contracts::bare_instantiate(
				origin,
				value,
				gas_limit,
				storage_deposit_limit,
				code,
				data,
				salt,
				CONTRACTS_DEBUG_OUTPUT
			)
		}

		fn upload_code(
			origin: AccountId,
			code: Vec<u8>,
			storage_deposit_limit: Option<Balance>,
			determinism: pallet_contracts::Determinism,
		) -> pallet_contracts_primitives::CodeUploadResult<Hash, Balance>
		{
			Contracts::bare_upload_code(origin, code, storage_deposit_limit, determinism)
		}

		fn get_storage(
			address: AccountId,
			key: Vec<u8>,
		) -> pallet_contracts_primitives::GetStorageResult {
			Contracts::get_storage(address, key)
		}
	}

}

struct CheckInherents;

impl cumulus_pallet_parachain_system::CheckInherents<Block> for CheckInherents {
	fn check_inherents(
		block: &Block,
		relay_state_proof: &cumulus_pallet_parachain_system::RelayChainStateProof,
	) -> sp_inherents::CheckInherentsResult {
		let relay_chain_slot = relay_state_proof
			.read_slot()
			.expect("Could not read the relay chain slot from the proof");

		let inherent_data =
			cumulus_primitives_timestamp::InherentDataProvider::from_relay_chain_slot_and_duration(
				relay_chain_slot,
				sp_std::time::Duration::from_secs(6),
			)
			.create_inherent_data()
			.expect("Could not create the timestamp inherent data");

		inherent_data.check_extrinsics(block)
	}
}

cumulus_pallet_parachain_system::register_validate_block! {
	Runtime = Runtime,
	BlockExecutor = cumulus_pallet_aura_ext::BlockExecutor::<Runtime, Executive>,
	CheckInherents = CheckInherents,
}
