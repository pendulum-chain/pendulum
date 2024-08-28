#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 512.
#![recursion_limit = "512"]

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

mod chain_ext;
pub mod definitions;
mod weights;
pub mod xcm_config;
pub mod zenlink;

use crate::zenlink::*;
use xcm::v3::MultiLocation;
use zenlink_protocol::{AssetBalance, MultiAssetsHandler, PairInfo};

pub use parachain_staking::InflationInfo;

use codec::Encode;

use smallvec::smallvec;
use sp_api::impl_runtime_apis;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata, H256};
use sp_runtime::{
	create_runtime_str, generic, impl_opaque_keys,
	traits::{
		AccountIdConversion, AccountIdLookup, BlakeTwo256, Block as BlockT, Convert, ConvertInto,
	},
	transaction_validity::{TransactionSource, TransactionValidity},
	ApplyExtrinsicResult, DispatchError, FixedPointNumber, MultiAddress, Perbill, Permill,
	Perquintill, SaturatedConversion,
};

use bifrost_farming as farming;
use bifrost_farming_rpc_runtime_api as farming_rpc_runtime_api;

pub use spacewalk_primitives::CurrencyId;
use spacewalk_primitives::{
	self as primitives, Asset, CurrencyId::XCM, Moment, SignedFixedPoint, SignedInner,
	UnsignedFixedPoint, UnsignedInner,
};

#[cfg(any(feature = "runtime-benchmarks"))]
use oracle::testing_utils::MockDataFeeder;

use sp_std::{fmt::Debug, marker::PhantomData, prelude::*};
#[cfg(feature = "std")]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

use frame_support::{
	construct_runtime,
	dispatch::DispatchClass,
	parameter_types,
	traits::{
		fungible::Credit, ConstBool, ConstU32, Contains, Currency as FrameCurrency,
		EitherOfDiverse, EqualPrivilegeOnly, Imbalance, InstanceFilter, OnUnbalanced,
		WithdrawReasons,
	},
	weights::{
		constants::WEIGHT_REF_TIME_PER_SECOND, ConstantMultiplier, Weight, WeightToFeeCoefficient,
		WeightToFeeCoefficients, WeightToFeePolynomial,
	},
	PalletId,
};
use frame_system::{
	limits::{BlockLength, BlockWeights},
	EnsureRoot, EnsureSigned,
};

use runtime_common::{
	asset_registry, AccountId, Amount, AuraId, Balance, BlockNumber, Hash, Index, PoolId,
	ProxyType, ReserveIdentifier, Signature, EXISTENTIAL_DEPOSIT, MILLIUNIT, NANOUNIT, UNIT,
};

use cumulus_pallet_parachain_system::RelayNumberStrictlyIncreases;

use dia_oracle::DiaOracle;
pub use dia_oracle::dia::AssetId;
pub use issue::{Event as IssueEvent, IssueRequest};
pub use nomination::Event as NominationEvent;
use oracle::dia::DiaOracleAdapter;
pub use redeem::{Event as RedeemEvent, RedeemRequest};
pub use replace::{Event as ReplaceEvent, ReplaceRequest};
pub use security::StatusCode;
pub use stellar_relay::traits::{FieldLength, Organization, Validator};

use xcm_config::{XcmConfig, XcmOriginToTransactDispatchOrigin};

use module_oracle_rpc_runtime_api::BalanceWrapper;
use orml_currencies::BasicCurrencyAdapter;
use orml_traits::{currency::MutationHooks, parameter_type_with_key};

#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;

// Polkadot imports
use polkadot_runtime_common::{BlockHashCount, SlowAdjustingFeeUpdate};

use weights::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight};

use runtime_common::asset_registry::StringLimit;
// XCM Imports
use xcm_executor::XcmExecutor;

// Chain Extension
use crate::chain_ext::{PriceChainExtension, TokensChainExtension};

/// Spacewalk vault id type
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
	treasury_buyout_extension::CheckBuyout<Runtime>,
);

type EventRecord = frame_system::EventRecord<
	<Runtime as frame_system::Config>::RuntimeEvent,
	<Runtime as frame_system::Config>::Hash,
>;

/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic =
	generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;

pub type SignedPayload = generic::SignedPayload<RuntimeCall, SignedExtra>;

/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, RuntimeCall, SignedExtra>;

parameter_types! {
	pub const InactiveAccounts: Vec<AccountId> = Vec::new();
}

// To be removed after contracts migrations executes.
use pallet_contracts::migration::{v11, v12, v13, v14, v15};

// Custom storage version bump
use frame_support::traits::{GetStorageVersion, OnRuntimeUpgrade};
use frame_support::pallet_prelude::StorageVersion;

pub struct CustomOnRuntimeUpgrade;
impl OnRuntimeUpgrade for CustomOnRuntimeUpgrade {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		log::info!("Custom on-runtime-upgrade function");

		let mut writes = 0;
		// WARNING: manually setting the storage version
		if ParachainStaking::on_chain_storage_version() == 0 {
			log::info!("Upgrading parachain staking storage version to 7");
			StorageVersion::new(7).put::<ParachainStaking>();
			writes += 1;
		}

		if Bounties::on_chain_storage_version() == 0 {
			log::info!("Upgrading bounties storage version to 4");
			StorageVersion::new(4).put::<Bounties>();
			writes += 1;
		}
		// not really a heavy operation
		<Runtime as frame_system::Config>::DbWeight::get().reads_writes(4, writes)
	}
}


/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPalletsWithSystem,
	(
		CustomOnRuntimeUpgrade,
		pallet_contracts::migration::Migration<Runtime>,
	),
>;

pub struct ConvertPrice;

impl Convert<u128, Option<UnsignedFixedPoint>> for ConvertPrice {
	fn convert(price: u128) -> Option<UnsignedFixedPoint> {
		// The DIA batching server returns the price in 1e12 format, see [here](https://github.com/pendulum-chain/oracle-pallet/blob/716073885de01f923a0fe44a05bd2a0bd45db555/dia-batching-server/src/price_updater.rs#L141)
		// but our UnsignedFixedPoint implementation expects the price in 1e18 format.
		Some(UnsignedFixedPoint::from_rational(price, 1_000_000_000_000))
	}
}

pub struct ConvertMoment;

impl Convert<u64, Option<Moment>> for ConvertMoment {
	fn convert(moment: u64) -> Option<Moment> {
		// The provided moment is in seconds, but we need milliseconds
		Some(moment.saturating_mul(1000))
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
	spec_name: create_runtime_str!("pendulum"),
	impl_name: create_runtime_str!("pendulum"),
	authoring_version: 1,
	spec_version: 18,
	impl_version: 0,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 10,
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
	Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND.saturating_div(2), 0)
		.set_proof_size(cumulus_primitives_core::relay_chain::MAX_POV_SIZE as u64);

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
	pub const SS58Prefix: u16 = 56;
}

pub struct BaseFilter;

impl Contains<RuntimeCall> for BaseFilter {
	fn contains(call: &RuntimeCall) -> bool {
		match call {
			// These modules are all allowed to be called by transactions:
			RuntimeCall::Bounties(_)
			| RuntimeCall::ChildBounties(_)
			| RuntimeCall::ClientsInfo(_)
			| RuntimeCall::Treasury(_)
			| RuntimeCall::Tokens(_)
			| RuntimeCall::Currencies(_)
			| RuntimeCall::ParachainStaking(_)
			| RuntimeCall::Democracy(_)
			| RuntimeCall::Council(_)
			| RuntimeCall::TechnicalCommittee(_)
			| RuntimeCall::System(_)
			| RuntimeCall::Scheduler(_)
			| RuntimeCall::Preimage(_)
			| RuntimeCall::Timestamp(_)
			| RuntimeCall::Balances(_)
			| RuntimeCall::Session(_)
			| RuntimeCall::ParachainSystem(_)
			| RuntimeCall::XcmpQueue(_)
			| RuntimeCall::PolkadotXcm(_)
			| RuntimeCall::DmpQueue(_)
			| RuntimeCall::Utility(_)
			| RuntimeCall::Vesting(_)
			| RuntimeCall::XTokens(_)
			| RuntimeCall::Multisig(_)
			| RuntimeCall::Identity(_)
			| RuntimeCall::Contracts(_)
			| RuntimeCall::ZenlinkProtocol(_)
			| RuntimeCall::DiaOracleModule(_)
			| RuntimeCall::VestingManager(_)
			| RuntimeCall::TokenAllowance(_)
			| RuntimeCall::AssetRegistry(_)
			| RuntimeCall::Fee(_)
			| RuntimeCall::Issue(_)
			| RuntimeCall::Nomination(_)
			| RuntimeCall::Oracle(_)
			| RuntimeCall::Redeem(_)
			| RuntimeCall::Replace(_)
			| RuntimeCall::Security(_)
			| RuntimeCall::StellarRelay(_)
			| RuntimeCall::VaultRegistry(_)
			| RuntimeCall::PooledVaultRewards(_)
			| RuntimeCall::RewardDistribution(_)
			| RuntimeCall::Farming(_)
			| RuntimeCall::Proxy(_)
			| RuntimeCall::TreasuryBuyoutExtension(_)
			| RuntimeCall::ParachainInfo(_)
			| RuntimeCall::CumulusXcm(_)
			| RuntimeCall::VaultStaking(_) => true,
			// All pallets are allowed, but exhaustive match is defensive
			// in the case of adding new pallets.
		}
	}
}

// Configure FRAME pallets to include in runtime.

impl frame_system::Config for Runtime {
    /// The Block type used by the runtime. This is used by construct_runtime to retrieve the extrinsics or other block specific data as needed.
    type Block = Block;
	/// The identifier used to distinguish between accounts.
	type AccountId = AccountId;
	/// The aggregated dispatch type that is available for extrinsics.
	type RuntimeCall = RuntimeCall;
	/// The lookup mechanism to get account ID from whatever is passed in dispatchers.
	type Lookup = AccountIdLookup<AccountId, ()>;
	/// This stores the number of previous transactions associated with a sender account.
	type Nonce = Index;
	/// The type for hashing blocks and tries.
	type Hash = Hash;
	/// The hashing algorithm used.
	type Hashing = BlakeTwo256;
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
	/// This is used as an identifier of the chain. 56 is the prefix for Pendulum
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
	type EventHandler = ParachainStaking;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
	pub const MaxLocks: u32 = 50;
	pub const MaxReserves: u32 = 50;
}

pub struct MoveDustToTreasury;

impl
	OnUnbalanced<
		Credit<<Runtime as frame_system::Config>::AccountId, pallet_balances::Pallet<Runtime>>,
	> for MoveDustToTreasury
{
	fn on_nonzero_unbalanced(
		amount: Credit<
			<Runtime as frame_system::Config>::AccountId,
			pallet_balances::Pallet<Runtime>,
		>,
	) {
		let _ = <Balances as FrameCurrency<AccountId>>::deposit_creating(
			&TreasuryPalletId::get().into_account_truncating(),
			amount.peek(),
		);
	}
}

impl pallet_balances::Config for Runtime {
	type MaxLocks = MaxLocks;
	/// The type for recording an account's balance.
	type Balance = Balance;
	/// The ubiquitous event type.
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = MoveDustToTreasury;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
	type MaxReserves = MaxReserves;
	type ReserveIdentifier = ReserveIdentifier;
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type MaxHolds = ConstU32<1>;
	type RuntimeHoldReason = RuntimeHoldReason;
}

parameter_types! {
	pub const TransactionByteFee: Balance = 100 * NANOUNIT;
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
	#[cfg(feature = "std")]
	type ConsensusHook = cumulus_pallet_parachain_system::consensus_hook::ExpectParentIncluded;
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
	type PriceForSiblingDelivery = ();
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

parameter_types! {
	// as per documentation, typical value for this is false "unless this pallet is being augmented by another pallet"
	// https://github.com/paritytech/polkadot-sdk/blob/release-polkadot-v1.1.0/substrate/frame/aura/src/lib.rs#L111
	pub const AllowMultipleBlocksPerSlot: bool = false;
}
impl pallet_aura::Config for Runtime {
	type AuthorityId = AuraId;
	type DisabledValidators = ();
	type MaxAuthorities = MaxAuthorities;
	type AllowMultipleBlocksPerSlot = AllowMultipleBlocksPerSlot;
}

parameter_types! {
	pub const LaunchPeriod: BlockNumber = 5 * DAYS;
	pub const VotingPeriod: BlockNumber = 5 * DAYS;
	pub const FastTrackVotingPeriod: BlockNumber = 3 * HOURS;
	pub const MinimumDeposit: Balance = UNIT;
	pub const EnactmentPeriod: BlockNumber = 2 * DAYS;
	pub const CooloffPeriod: BlockNumber = 7 * DAYS;
	pub const MaxProposals: u32 = 100;
	pub const VoteLockingPeriod: u32 = 7 * DAYS;
}

impl pallet_democracy::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type EnactmentPeriod = EnactmentPeriod;
	type LaunchPeriod = LaunchPeriod;
	type VotingPeriod = VotingPeriod;
	type VoteLockingPeriod = VoteLockingPeriod;
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
	type SubmitOrigin = EnsureSigned<AccountId>;
}

parameter_types! {
	pub const CouncilMotionDuration: BlockNumber = 3 * DAYS;
	pub const CouncilMaxProposals: u32 = 100;
	pub const CouncilMaxMembers: u32 = 100;
	pub MaxProposalWeight: Weight = Perbill::from_percent(50) * RuntimeBlockWeights::get().max_block;
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
	type SetMembersOrigin = EnsureRoot<Self::AccountId>;
	type MaxProposalWeight = MaxProposalWeight;
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
	type SetMembersOrigin = EnsureRoot<Self::AccountId>;
	type MaxProposalWeight = MaxProposalWeight;
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
	pub const PreimageBaseDeposit: Balance = UNIT;
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
	pub const BountyDepositBase: Balance = UNIT;
	pub const CuratorDepositMultiplier: Permill = Permill::from_percent(50);
	pub const CuratorDepositMin: Balance = UNIT;
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
	pub const ChildBountyValueMinimum: Balance = UNIT;
}

impl pallet_child_bounties::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MaxActiveChildBountyCount = ConstU32<10>;
	type ChildBountyValueMinimum = ChildBountyValueMinimum;
	type WeightInfo = pallet_child_bounties::weights::SubstrateWeight<Runtime>;
}

parameter_type_with_key! {
	pub ExistentialDeposits: |currency_id: CurrencyId| -> Balance {
		// Since the xcm trader uses Tokens to get the minimum
		// balance of both it's assets and native, we need to
		// handle native here
		match currency_id{
			CurrencyId::Native => EXISTENTIAL_DEPOSIT,
			_ => NANOUNIT
		}
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
	type Amount = Amount;
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
}

impl orml_currencies::Config for Runtime {
	type MultiCurrency = Tokens;
	type NativeCurrency = BasicCurrencyAdapter<Runtime, Balances, Amount, BlockNumber>;
	type GetNativeCurrencyId = NativeCurrencyId;
	type WeightInfo = ();
}

impl orml_asset_registry::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type CustomMetadata = asset_registry::CustomMetadata;
	type AssetId = CurrencyId;
	type AuthorityOrigin = asset_registry::AssetAuthority;
	type AssetProcessor = asset_registry::CustomAssetProcessor;
	type Balance = Balance;
	type WeightInfo = weights::orml_asset_registry::WeightInfo<Runtime>;
	type StringLimit = StringLimit;
}

parameter_types! {
	pub const MinBlocksPerRound: BlockNumber = HOURS;
	pub const DefaultBlocksPerRound: BlockNumber = 2 * HOURS;
	pub const StakeDuration: BlockNumber = 7 * DAYS;
	pub const ExitQueueDelay: u32 = 2;
	pub const MinCollators: u32 = 8;
	pub const MinRequiredCollators: u32 = 4;
	pub const MaxDelegationsPerRound: u32 = 1;
	#[derive(Debug, Eq, PartialEq)]
	pub const MaxDelegatorsPerCollator: u32 = 40;
	pub const MinCollatorStake: Balance = 5_000 * UNIT;
	pub const MinDelegatorStake: Balance = 10 * UNIT;
	#[derive(Debug, Eq, PartialEq)]
	pub const MaxTopCandidates: u32 = 50;
	pub const MaxUnstakeRequests: u32 = 10;
	pub const NetworkRewardStart: BlockNumber = BlockNumber::MAX;
	pub const NetworkRewardRate: Perquintill = Perquintill::from_percent(0);
	pub const CollatorRewardRateDecay: Perquintill = Perquintill::from_parts(938_252_045_000_000_000u64);
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
	type MaxTopCandidates = MaxTopCandidates;
	type MinDelegatorStake = MinDelegatorStake;
	type MaxUnstakeRequests = MaxUnstakeRequests;
	type NetworkRewardRate = NetworkRewardRate;
	type NetworkRewardStart = NetworkRewardStart;
	type NetworkRewardBeneficiary = Treasury;
	type CollatorRewardRateDecay = CollatorRewardRateDecay;
	type WeightInfo = weights::parachain_staking::SubstrateWeight<Runtime>;

	const BLOCKS_PER_YEAR: BlockNumber = BLOCKS_PER_YEAR;
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

impl vesting_manager::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type VestingSchedule = Vesting;
}

const fn deposit(items: u32, bytes: u32) -> Balance {
	(items as Balance * UNIT + (bytes as Balance) * (5 * MILLIUNIT / 100)) / 10
}

parameter_types! {
	pub const DepositPerItem: Balance = deposit(1, 0);
	pub const DepositPerByte: Balance = deposit(0, 1);
	// Fallback value if storage deposit limit not set by the user
	pub const DefaultDepositLimit: Balance = deposit(1024, 1024 * 1024);
	pub Schedule: pallet_contracts::Schedule<Runtime> = Default::default();
	pub const CodeHashLockupDepositPercent: Perbill = Perbill::from_percent(10);
	pub const MaxDelegateDependencies: u32 = 32;
}

impl pallet_contracts::Config for Runtime {
	type Time = Timestamp;
	type Randomness = RandomnessCollectiveFlip;
	type Currency = Balances;
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type CallFilter = frame_support::traits::Nothing;
	type DepositPerItem = DepositPerItem;
	type DepositPerByte = DepositPerByte;
	type CallStack = [pallet_contracts::Frame<Self>; 5];
	type WeightPrice = pallet_transaction_payment::Pallet<Self>;
	type WeightInfo = pallet_contracts::weights::SubstrateWeight<Self>;
	type ChainExtension =
		(TokensChainExtension<Self, Tokens, AccountId>, PriceChainExtension<Self>);
	type Schedule = Schedule;
	type AddressGenerator = pallet_contracts::DefaultAddressGenerator;
	type MaxCodeLen = ConstU32<{ 123 * 1024 }>;
	type MaxStorageKeyLen = ConstU32<128>;
	type UnsafeUnstableInterface = ConstBool<true>;
	type MaxDebugBufferLen = ConstU32<{ 2 * 1024 * 1024 }>;
	type DefaultDepositLimit = DefaultDepositLimit;
	type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
	type MaxDelegateDependencies = MaxDelegateDependencies;
	type RuntimeHoldReason = RuntimeHoldReason;
	type Migrations = (v11::Migration<Self>, v12::Migration<Runtime, Balances>,  v13::Migration<Self>, v14::Migration<Self, Balances>,  v15::Migration<Self> );
	type Debug = ();
	type Environment = ();
}

impl pallet_insecure_randomness_collective_flip::Config for Runtime {}

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
	type WeightInfo = weights::dia_oracle::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const FarmingKeeperPalletId: PalletId = PalletId(*b"pe/fmkpr");
	pub const FarmingRewardIssuerPalletId: PalletId = PalletId(*b"pe/fmrir");
	pub const FarmingBoostPalletId: PalletId = PalletId(*b"pe/fmbst");
	pub PendulumTreasuryAccount: AccountId = TreasuryPalletId::get().into_account_truncating();
	pub const WhitelistMaximumLimit: u32 = 10;
}

impl farming::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type CurrencyId = CurrencyId;
	type MultiCurrency = Currencies;
	type ControlOrigin = EnsureRoot<AccountId>;
	type WeightInfo = ();
	type TreasuryAccount = PendulumTreasuryAccount;
	type Keeper = FarmingKeeperPalletId;
	type RewardIssuer = FarmingRewardIssuerPalletId;
	type FarmingBoost = FarmingBoostPalletId;
	type VeMinting = ();
	type BlockNumberToBalance = ConvertInto;
	type WhitelistMaximumLimit = WhitelistMaximumLimit;
}

impl frame_system::offchain::SigningTypes for Runtime {
	type Public = <Signature as sp_runtime::traits::Verify>::Signer;
	type Signature = Signature;
}

impl<C> frame_system::offchain::SendTransactionTypes<C> for Runtime
where
	RuntimeCall: From<C>,
{
	type OverarchingCall = RuntimeCall;
	type Extrinsic = UncheckedExtrinsic;
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
			treasury_buyout_extension::CheckBuyout::<Runtime>::new(),
		);

		let raw_payload = SignedPayload::new(call, extra).ok()?;
		let signature = raw_payload.using_encoded(|payload| C::sign(payload, public))?;
		let address = account;
		let (call, extra, _) = raw_payload.deconstruct();
		Some((call, (sp_runtime::MultiAddress::Id(address), signature, extra)))
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
	pub const RelayChainCurrencyId: CurrencyId = XCM(0); // 0 is the index of the relay chain in our XCM mapping
	// This specific asset is used for benchmarking Spacewalk pallets as it's already used as the wrapped currency in the genesis config
	pub const GetWrappedCurrencyId: CurrencyId = CurrencyId::Stellar(Asset::AlphaNum4 {
		code: *b"USDC",
		issuer: [
			59, 153, 17, 56, 14, 254, 152, 139, 160, 168, 144, 14, 177, 207, 228, 79, 54, 111, 125,
			190, 148, 107, 237, 7, 114, 64, 247, 246, 36, 223, 21, 197,
		],
	});
}
impl currency::Config for Runtime {
	type UnsignedFixedPoint = UnsignedFixedPoint;
	type SignedInner = SignedInner;
	type SignedFixedPoint = SignedFixedPoint;
	type Balance = Balance;
	type GetRelayChainCurrencyId = RelayChainCurrencyId;
	#[cfg(feature = "runtime-benchmarks")]
	type GetWrappedCurrencyId = GetWrappedCurrencyId;
	type AssetConversion = primitives::AssetConversion;
	type BalanceConversion = primitives::BalanceConversion;
	type CurrencyConversion = CurrencyConvert;
	type AmountCompatibility = primitives::StellarCompatibility;
}

impl security::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = security::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const MaxRewardCurrencies: u32= 10;
}

impl staking::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SignedInner = SignedInner;
	type SignedFixedPoint = SignedFixedPoint;
	type GetNativeCurrencyId = NativeCurrencyId;
	type CurrencyId = CurrencyId;
	type MaxRewardCurrencies = MaxRewardCurrencies;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct DataFeederBenchmark<K, V, A>(PhantomData<(K, V, A)>);

#[cfg(feature = "runtime-benchmarks")]
impl<K, V, A> orml_traits::DataFeeder<K, V, A> for DataFeederBenchmark<K, V, A> {
	fn feed_value(_who: Option<A>, _key: K, _value: V) -> sp_runtime::DispatchResult {
		Ok(())
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl<K, V, A> orml_traits::DataProvider<K, V> for DataFeederBenchmark<K, V, A> {
	fn get(_key: &K) -> Option<V> {
		None
	}
}

cfg_if::cfg_if! {
	if #[cfg(feature = "runtime-benchmarks")] {
		use oracle::testing_utils::{
			MockConvertMoment, MockConvertPrice, MockDiaOracle, MockOracleKeyConvertor,
		};
		type DataProviderImpl = DiaOracleAdapter<
			MockDiaOracle,
			UnsignedFixedPoint,
			Moment,
			MockOracleKeyConvertor,
			MockConvertPrice,
			MockConvertMoment<Moment>,
		>;
	} else {
		type DataProviderImpl = DiaOracleAdapter<
			DiaOracleModule,
			UnsignedFixedPoint,
			Moment,
			asset_registry::AssetRegistryToDiaOracleKeyConvertor<Runtime>,
			ConvertPrice,
			ConvertMoment,
		>;
	}
}

impl oracle::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::oracle::SubstrateWeight<Runtime>;
	type DecimalsLookup = DecimalsLookupImpl;
	type DataProvider = DataProviderImpl;
	#[cfg(feature = "runtime-benchmarks")]
	type DataFeeder = MockDataFeeder<Self::AccountId, Moment>;
}

parameter_types! {
	pub const OrganizationLimit: u32 = 255;
	pub const ValidatorLimit: u32 = 255;
	pub const IsPublicNetwork: bool = true;
}

impl stellar_relay::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OrganizationId = u128;
	type OrganizationLimit = OrganizationLimit;
	type ValidatorLimit = ValidatorLimit;
	type IsPublicNetwork = IsPublicNetwork;
	type WeightInfo = weights::stellar_relay::SubstrateWeight<Runtime>;
}

parameter_types! {
	pub const FeePalletId: PalletId = PalletId(*b"mod/fees");
	pub const VaultRegistryPalletId: PalletId = PalletId(*b"mod/vreg");
	pub const MaxExpectedValue: UnsignedFixedPoint = UnsignedFixedPoint::from_inner(<UnsignedFixedPoint as FixedPointNumber>::DIV);
	pub FeeAccount: AccountId = FeePalletId::get().into_account_truncating();
}
impl fee::Config for Runtime {
	type FeePalletId = FeePalletId;
	type WeightInfo = weights::fee::SubstrateWeight<Runtime>;
	type SignedFixedPoint = SignedFixedPoint;
	type SignedInner = SignedInner;
	type UnsignedFixedPoint = UnsignedFixedPoint;
	type UnsignedInner = UnsignedInner;
	type VaultRewards = PooledVaultRewards;
	type VaultStaking = VaultStaking;
	type OnSweep = currency::SweepFunds<Runtime, FeeAccount>;
	type MaxExpectedValue = MaxExpectedValue;
	type RewardDistribution = RewardDistribution;
}

impl vault_registry::Config for Runtime {
	type PalletId = VaultRegistryPalletId;
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type WeightInfo = weights::vault_registry::SubstrateWeight<Runtime>;
	type GetGriefingCollateralCurrencyId = NativeCurrencyId;
}

impl redeem::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::redeem::SubstrateWeight<Runtime>;
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
	type WeightInfo = weights::issue::SubstrateWeight<Runtime>;
}

impl nomination::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::nomination::SubstrateWeight<Runtime>;
}

impl replace::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::replace::SubstrateWeight<Runtime>;
}

impl clients_info::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = clients_info::SubstrateWeight<Runtime>;
	type MaxNameLength = ConstU32<255>;
	type MaxUriLength = ConstU32<255>;
}
// Choice of parameters: Perquintill::from_parts(36600800000000000u64) represents a value of
// 0.0366008 = 36600800000000000 / 1×10¹⁸
// The decay interval 216000 equates to a month when considering 1 block every 12 seconds
parameter_types! {
	pub const DecayRate: Perquintill = Perquintill::from_parts(36600800000000000);
	pub const MaxCurrencies: u32 = 10;
}

impl reward_distribution::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = reward_distribution::SubstrateWeight<Runtime>;
	type Balance = Balance;
	type DecayInterval = ConstU32<216_000>;
	type DecayRate = DecayRate;
	type VaultRewards = PooledVaultRewards;
	type MaxCurrencies = MaxCurrencies;
	type OracleApi = Oracle;
	type Balances = Balances;
	type VaultStaking = VaultStaking;
	type FeePalletId = FeePalletId;
}

impl pooled_rewards::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SignedFixedPoint = SignedFixedPoint;
	type PoolId = CurrencyId;
	type PoolRewardsCurrencyId = CurrencyId;
	type StakeId = VaultId;
	type MaxRewardCurrencies = MaxRewardCurrencies;
}

impl InstanceFilter<RuntimeCall> for ProxyType {
	fn filter(&self, c: &RuntimeCall) -> bool {
		match self {
			// Always allowed RuntimeCall::Utility no matter type.
			// Only transactions allowed by Proxy.filter can be executed
			_ if matches!(c, RuntimeCall::Utility(..)) => true,
			ProxyType::Any => true,
		}
	}

	// Determines whether self matches at least everything that o does.
	fn is_superset(&self, o: &Self) -> bool {
		match (self, o) {
			(x, y) if x == y => true,
			(ProxyType::Any, _) => true,
			#[allow(unreachable_patterns)]
			_ => false,
		}
	}
}

parameter_types! {
	// One storage item; key size 32, value size 8; .
	pub const ProxyDepositBase: Balance = deposit(1, 8);
	// Additional storage item size of 33 bytes.
	pub const ProxyDepositFactor: Balance = deposit(0, 33);
	pub const MaxProxies: u16 = 32;
	pub const MaxPending: u16 = 32;
	pub const AnnouncementDepositBase: Balance = deposit(1, 8);
	pub const AnnouncementDepositFactor: Balance = deposit(0, 66);
}

impl pallet_proxy::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Currency = Balances;
	type ProxyType = ProxyType;
	type ProxyDepositBase = ProxyDepositBase;
	type ProxyDepositFactor = ProxyDepositFactor;
	type MaxProxies = MaxProxies;
	type WeightInfo = pallet_proxy::weights::SubstrateWeight<Runtime>;
	type MaxPending = MaxPending;
	type CallHasher = BlakeTwo256;
	type AnnouncementDepositBase = AnnouncementDepositBase;
	type AnnouncementDepositFactor = AnnouncementDepositFactor;
}

impl orml_currencies_allowance_extension::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = weights::orml_currencies_allowance_extension::SubstrateWeight<Runtime>;
	type MaxAllowedCurrencies = ConstU32<256>;
}

pub struct DecimalsLookupImpl;
impl spacewalk_primitives::DecimalsLookup for DecimalsLookupImpl {
	type CurrencyId = CurrencyId;

	fn decimals(currency_id: Self::CurrencyId) -> u32 {
		// Fallback to hard-coded implementation in case no decimals are found in asset registry
		match AssetRegistry::metadata(currency_id) {
			Some(metadata) => metadata.decimals,
			None => spacewalk_primitives::PendulumDecimalsLookup::decimals(currency_id),
		}
	}
}

parameter_types! {
	pub const SellFee: Permill = Permill::from_percent(5);
	pub const MinAmountToBuyout: Balance = 100 * MILLIUNIT; // 0.1 PEN or 100_000_000_000
	// 24 hours in blocks (where average block time is 12 seconds)
	pub const BuyoutPeriod: u32 = 7200;
	// Maximum number of allowed currencies for buyout
	pub const MaxAllowedBuyoutCurrencies: u32 = 20;
}

impl treasury_buyout_extension::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Currencies;
	type TreasuryAccount = PendulumTreasuryAccount;
	type BuyoutPeriod = BuyoutPeriod;
	type SellFee = SellFee;
	type PriceGetter = runtime_common::OraclePriceGetter<Runtime>;
	type DecimalsLookup = DecimalsLookupImpl;
	type MinAmountToBuyout = MinAmountToBuyout;
	type MaxAllowedBuyoutCurrencies = MaxAllowedBuyoutCurrencies;
	type WeightInfo = weights::treasury_buyout_extension::SubstrateWeight<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type RelayChainCurrencyId = RelayChainCurrencyId;
}

// Create the runtime by composing the FRAME pallets that were previously configured.
construct_runtime!(
	pub enum Runtime
	{
		// System support stuff.
		System: frame_system = 0,
		ParachainSystem: cumulus_pallet_parachain_system = 1,
		Timestamp: pallet_timestamp = 2,
		ParachainInfo: parachain_info = 3,

		// Monetary stuff.
		Balances: pallet_balances = 10,
		TransactionPayment: pallet_transaction_payment = 11,

		// Governance
		Democracy: pallet_democracy= 13,
		Council: pallet_collective::<Instance1> = 14,
		TechnicalCommittee: pallet_collective::<Instance2> = 15,
		Scheduler: pallet_scheduler = 16,
		Preimage: pallet_preimage = 17,
		Multisig: pallet_multisig = 18,
		Treasury: pallet_treasury = 19,
		Bounties: pallet_bounties = 20,
		ChildBounties: pallet_child_bounties = 21,
		Proxy: pallet_proxy = 22,

		// Consensus support.
		// The following order MUST NOT be changed: Aura -> Session -> Staking -> Authorship -> AuraExt
		// Dependencies: AuraExt on Aura, Authorship and Session on ParachainStaking
		Aura: pallet_aura = 33,
		Session: pallet_session = 32,
		ParachainStaking: parachain_staking = 35,
		Authorship: pallet_authorship = 30,
		AuraExt: cumulus_pallet_aura_ext = 34,

		// XCM helpers.
		XcmpQueue: cumulus_pallet_xcmp_queue = 40,
		PolkadotXcm: pallet_xcm = 41,
		CumulusXcm: cumulus_pallet_xcm = 42,
		DmpQueue: cumulus_pallet_dmp_queue = 43,

		// Amendments
		Vesting: pallet_vesting = 50,
		Utility: pallet_utility = 51,
		Currencies: orml_currencies = 52,
		Tokens: orml_tokens = 53,
		XTokens: orml_xtokens = 54,
		Identity: pallet_identity = 55,
		Contracts: pallet_contracts = 56,
		RandomnessCollectiveFlip: pallet_insecure_randomness_collective_flip = 57,
		DiaOracleModule: dia_oracle = 58,

		// Zenlink
		ZenlinkProtocol: zenlink_protocol::{Pallet, Call, Storage, Event<T>}  = 59,

		// Spacewalk pallets
		Currency: currency = 60,
		Fee: fee = 61,
		Issue: issue = 62,
		Nomination: nomination = 63,
		Oracle: oracle = 64,
		Redeem: redeem = 65,
		Replace: replace = 66,
		Security: security = 67,
		StellarRelay: stellar_relay = 68,
		VaultRegistry: vault_registry = 69,
		PooledVaultRewards: pooled_rewards = 70,
		VaultStaking: staking = 71,
		ClientsInfo: clients_info = 72,
		RewardDistribution: reward_distribution = 73,

		TokenAllowance: orml_currencies_allowance_extension = 80,
		TreasuryBuyoutExtension: treasury_buyout_extension = 82,

		//Farming
		Farming: farming = 90,
		// Asset Metadata
		AssetRegistry: orml_asset_registry = 91,

		VestingManager: vesting_manager = 100
	}
);

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
extern crate frame_benchmarking;

#[cfg(feature = "runtime-benchmarks")]
mod benches {
	define_benchmarks!(
		[frame_benchmarking, BaselineBench::<Runtime>]
		[frame_system, SystemBench::<Runtime>]
		[pallet_balances, Balances]
		[pallet_session, SessionBench::<Runtime>]
		[pallet_timestamp, Timestamp]
		[cumulus_pallet_xcmp_queue, XcmpQueue]
		[parachain_staking, ParachainStaking]

		[fee, Fee]
		[issue, Issue]
		[nomination, Nomination]
		[oracle, Oracle]
		[redeem, Redeem]
		[replace, Replace]
		[stellar_relay, StellarRelay]
		[vault_registry, VaultRegistry]

		// Other
		[orml_asset_registry, runtime_common::benchmarking::orml_asset_registry::Pallet::<Runtime>]
		[pallet_xcm, PolkadotXcm]

		[orml_currencies_allowance_extension, TokenAllowance]
		[treasury_buyout_extension, TreasuryBuyoutExtension]

		[dia_oracle, DiaOracleModule]
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

		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
			Runtime::metadata_at_version(version)
		}

		fn metadata_versions() -> sp_std::vec::Vec<u32> {
			Runtime::metadata_versions()
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
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	impl cumulus_primitives_core::CollectCollationInfo<Block> for Runtime {
		fn collect_collation_info(header: &<Block as BlockT>::Header) -> cumulus_primitives_core::CollationInfo {
			ParachainSystem::collect_collation_info(header)
		}
	}

	impl module_pallet_staking_rpc_runtime_api::ParachainStakingApi<Block, AccountId, Balance> for Runtime {
		fn get_unclaimed_staking_rewards(account: AccountId) -> BalanceWrapper<Balance> {
			let result = ParachainStaking::get_unclaimed_staking_rewards(&account);
			BalanceWrapper{amount:result}
		}

		fn get_staking_rates() -> module_pallet_staking_rpc_runtime_api::StakingRates {
			ParachainStaking::get_staking_rates()
		}
	}


	impl dia_oracle_runtime_api::DiaOracleApi<Block> for Runtime{
		fn get_value(blockchain: sp_std::vec::Vec<u8>, symbol: sp_std::vec::Vec<u8>)-> Result<dia_oracle_runtime_api::PriceInfo, sp_runtime::DispatchError>{
			DiaOracleModule::get_value(blockchain, symbol)
		}

		fn get_coin_info(blockchain: sp_std::vec::Vec<u8>, symbol: sp_std::vec::Vec<u8>)-> Result<dia_oracle_runtime_api::CoinInfo,sp_runtime::DispatchError>{
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

		fn calculate_remove_liquidity(
			asset_0: ZenlinkAssetId,
			asset_1: ZenlinkAssetId,
			amount: AssetBalance,
		) -> Option<(AssetBalance, AssetBalance)>{
			ZenlinkProtocol::calculate_remove_liquidity(
				asset_0,
				asset_1,
				amount,
			)
		}
	}

	impl farming_rpc_runtime_api::FarmingRuntimeApi<Block, AccountId, PoolId, CurrencyId> for Runtime {
		fn get_farming_rewards(who: AccountId, pid: PoolId) -> Vec<(CurrencyId, Balance)> {
			Farming::get_farming_rewards(&who, pid).unwrap_or(Vec::new())
		}

		fn get_gauge_rewards(who: AccountId, pid: PoolId) -> Vec<(CurrencyId, Balance)> {
			Farming::get_gauge_rewards(&who, pid).unwrap_or(Vec::new())
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade(checks: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
			let weight = Executive::try_runtime_upgrade(checks).unwrap();
			(weight, RuntimeBlockWeights::get().max_block)
		}

		fn execute_block(
			block: Block,
			state_root_check: bool,
			signature_check: bool,
			select: frame_try_runtime::TryStateSelect
		) -> Weight {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here.
			Executive::try_execute_block(block, state_root_check, signature_check, select).expect("execute-block failed")
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(extra: bool) -> (
			Vec<frame_benchmarking::BenchmarkList>,
			Vec<frame_support::traits::StorageInfo>,
		) {
			use frame_benchmarking::{baseline, Benchmarking, BenchmarkList};
			use frame_support::traits::StorageInfoTrait;
			use frame_system_benchmarking::Pallet as SystemBench;
			use cumulus_pallet_session_benchmarking::Pallet as SessionBench;
			use baseline::Pallet as BaselineBench;

			let mut list = Vec::<BenchmarkList>::new();
			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();
			return (list, storage_info)
		}

		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{baseline, Benchmarking, BenchmarkBatch};
			use sp_storage::TrackedStorageKey;
			use frame_system_benchmarking::Pallet as SystemBench;
			use baseline::Pallet as BaselineBench;

			impl frame_system_benchmarking::Config for Runtime {}
			impl baseline::Config for Runtime {}
			impl runtime_common::benchmarking::orml_asset_registry::Config for Runtime {}

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
			let amount = currency::Amount::new(collateral.amount, vault.collateral_currency());
			VaultRegistry::get_collateralization_from_vault_and_collateral(vault, &amount, only_issued)
		}
		fn get_required_collateral_for_wrapped(amount_wrapped: BalanceWrapper<Balance>, wrapped_currency_id: CurrencyId, collateral_currency_id: CurrencyId) -> Result<BalanceWrapper<Balance>, DispatchError> {
			let amount_wrapped = currency::Amount::new(amount_wrapped.amount, wrapped_currency_id);
			let result = VaultRegistry::get_required_collateral_for_wrapped(&amount_wrapped, collateral_currency_id)?;
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

	impl pallet_contracts::ContractsApi<Block, AccountId, Balance, BlockNumber, Hash, EventRecord>
		for Runtime
	{
		fn call(
			origin: AccountId,
			dest: AccountId,
			value: Balance,
			gas_limit: Option<Weight>,
			storage_deposit_limit: Option<Balance>,
			input_data: Vec<u8>,
		) -> pallet_contracts_primitives::ContractExecResult<Balance, EventRecord> {
			let gas_limit = gas_limit.unwrap_or(RuntimeBlockWeights::get().max_block);
			Contracts::bare_call(
				origin,
                dest,
                value,
                gas_limit,
                storage_deposit_limit,
                input_data,
                pallet_contracts::DebugInfo::UnsafeDebug,
                pallet_contracts::CollectEvents::UnsafeCollect,
                pallet_contracts::Determinism::Enforced,
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
		) -> pallet_contracts_primitives::ContractInstantiateResult<AccountId, Balance, EventRecord>
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
                pallet_contracts::DebugInfo::UnsafeDebug,
                pallet_contracts::CollectEvents::UnsafeCollect,
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

	impl module_oracle_rpc_runtime_api::OracleApi<Block, Balance, CurrencyId> for Runtime {
		fn currency_to_usd(amount:BalanceWrapper<Balance>, currency_id: CurrencyId) -> Result<BalanceWrapper<Balance>, DispatchError> {
			let result = Oracle::currency_to_usd(amount.amount, currency_id)?;
			Ok(BalanceWrapper{amount:result})
		}

		fn usd_to_currency(amount:BalanceWrapper<Balance>, currency_id: CurrencyId) -> Result<BalanceWrapper<Balance>, DispatchError> {
			let result = Oracle::usd_to_currency(amount.amount, currency_id)?;
			Ok(BalanceWrapper{amount:result})
		}

		fn get_exchange_rate(currency_id: CurrencyId) -> Result<UnsignedFixedPoint, DispatchError> {
			let result = Oracle::get_exchange_rate(currency_id)?;
			Ok(result)
		}
	}

}

#[allow(dead_code)]
struct CheckInherents;

#[allow(deprecated)]
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
