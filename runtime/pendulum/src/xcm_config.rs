use core::marker::PhantomData;

use cumulus_primitives_utility::{
	ChargeWeightInFungibles, TakeFirstAssetTrader, XcmFeesTo32ByteAccount,
};
use frame_support::{
	log, match_types, parameter_types,
	traits::{ContainsPair, Everything, Nothing},
	weights::{Weight, WeightToFee as WeightToFeeTrait},
};
use orml_traits::{
	location::{RelativeReserveProvider, Reserve},
	parameter_type_with_key,
};
use orml_xcm_support::{DepositToAlternative, IsNativeConcrete, MultiCurrencyAdapter};
use pallet_xcm::XcmPassthrough;
use polkadot_parachain::primitives::Sibling;
use sp_runtime::traits::Convert;
use xcm::latest::{prelude::*, Weight as XCMWeight};
use xcm_builder::{
	AccountId32Aliases, AllowKnownQueryResponses, AllowSubscriptionsFrom,
	AllowTopLevelPaidExecutionFrom, AllowUnpaidExecutionFrom, ConvertedConcreteId, EnsureXcmOrigin,
	FixedWeightBounds, ParentIsPreset, RelayChainAsNative, SiblingParachainAsNative,
	SiblingParachainConvertsVia, SignedAccountId32AsNative, SignedToAccountId32,
	SovereignSignedViaLocation, TakeWeightCredit,
};
use xcm_executor::{
	traits::{JustTry, ShouldExecute},
	XcmExecutor,
};

use runtime_common::{
	custom_transactor::{AssetData, AutomationPalletConfig, CustomTransactorInterceptor},
	parachains::polkadot::{asset_hub, equilibrium, moonbeam, polkadex},
};

use crate::{
	assets::{
		self,
		native_locations::{
			native_location_external_pov, native_location_local_pov, EURC_location_external_pov,
			EURC_location_local_pov,
		},
		xcm_assets,
	},
	ConstU32,
};

use super::{
	AccountId, Balance, Balances, Currencies, CurrencyId, ParachainInfo, ParachainSystem,
	PendulumTreasuryAccount, PolkadotXcm, Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin,
	System, Tokens, WeightToFee, XcmpQueue,
};

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

/// CurrencyIdConvert
/// This type implements conversions from our `CurrencyId` type into `MultiLocation` and vice-versa.
/// A currency locally is identified with a `CurrencyId` variant but in the network it is identified
/// in the form of a `MultiLocation`, in this case a pCfg (Para-Id, Currency-Id).
pub struct CurrencyIdConvert;

impl Convert<CurrencyId, Option<MultiLocation>> for CurrencyIdConvert {
	fn convert(id: CurrencyId) -> Option<MultiLocation> {
		match id {
			CurrencyId::XCM(f) => match f {
				xcm_assets::RELAY_DOT => Some(MultiLocation::parent()),
				xcm_assets::ASSETHUB_USDT => Some(asset_hub::USDT_location()),
				xcm_assets::ASSETHUB_USDC => Some(asset_hub::USDC_location()),
				xcm_assets::EQUILIBRIUM_EQD => Some(equilibrium::EQD_location()),
				xcm_assets::MOONBEAM_BRZ => Some(moonbeam::BRZ_location()),
				xcm_assets::POLKADEX_PDEX => Some(polkadex::PDEX_location()),
				xcm_assets::MOONBEAM_GLMR => Some(moonbeam::GLMR_location()),
				_ => None,
			},

			CurrencyId::Native => Some(native_location_external_pov()),
			assets::tokens::EURC_ID => Some(EURC_location_external_pov()),
			_ => None,
		}
	}
}

pub struct RelativeValue {
	num: Balance,
	denominator: Balance,
}

impl RelativeValue {
	fn adjust_amount_by_relative_value(amount: Balance, relative_value: RelativeValue) -> Balance {
		if relative_value.denominator == 0 {
			// Or probably error
			return amount
		}
		// Calculate the adjusted amount
		let adjusted_amount = amount * relative_value.denominator / relative_value.num;
		adjusted_amount
	}
}

pub struct RelayRelativeValue;
impl RelayRelativeValue {
	fn get_relative_value(id: CurrencyId) -> Option<RelativeValue> {
		match id {
			CurrencyId::XCM(f) => match f {
				xcm_assets::RELAY_DOT => Some(RelativeValue { num: 1, denominator: 1 }),
				xcm_assets::ASSETHUB_USDT => Some(RelativeValue { num: 1, denominator: 4 }),
				xcm_assets::ASSETHUB_USDC => Some(RelativeValue { num: 1, denominator: 4 }),
				xcm_assets::EQUILIBRIUM_EQD => Some(RelativeValue { num: 1, denominator: 10 }),
				xcm_assets::MOONBEAM_BRZ => Some(RelativeValue { num: 1, denominator: 10 }),
				xcm_assets::POLKADEX_PDEX => Some(RelativeValue { num: 1, denominator: 2 }),
				xcm_assets::MOONBEAM_GLMR => Some(RelativeValue { num: 1, denominator: 10 }),
				_ => None,
			},

			CurrencyId::Native => Some(RelativeValue { num: 1, denominator: 2 }),
			assets::tokens::EURC_ID => Some(RelativeValue { num: 1, denominator: 10 }),
			_ => Some(RelativeValue { num: 1, denominator: 1 }),
		}
	}
}

impl Convert<MultiLocation, Option<CurrencyId>> for CurrencyIdConvert {
	fn convert(location: MultiLocation) -> Option<CurrencyId> {
		match location {
			loc if loc == MultiLocation::parent() => Some(xcm_assets::RELAY_DOT_id()),

			loc if loc == asset_hub::USDT_location() => Some(xcm_assets::ASSETHUB_USDT_id()),
			loc if loc == asset_hub::USDC_location() => Some(xcm_assets::ASSETHUB_USDC_id()),
			loc if loc == equilibrium::EQD_location() => Some(xcm_assets::EQUILIBRIUM_EQD_id()),
			loc if loc == moonbeam::BRZ_location() => Some(xcm_assets::MOONBEAM_BRZ_id()),
			loc if loc == polkadex::PDEX_location() => Some(xcm_assets::POLKADEX_PDEX_id()),
			loc if loc == moonbeam::GLMR_location() => Some(xcm_assets::MOONBEAM_GLMR_id()),

			// Our native currency location without re-anchoring
			loc if loc == native_location_external_pov() => Some(CurrencyId::Native),
			// Our native currency location with re-anchoring
			// The XCM pallet will try to re-anchor the location before it reaches here
			loc if loc == native_location_local_pov() => Some(CurrencyId::Native),
			loc if loc == EURC_location_external_pov() => Some(assets::tokens::EURC_ID),
			loc if loc == EURC_location_local_pov() => Some(assets::tokens::EURC_ID),
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
		<CurrencyIdConvert as Convert<MultiLocation, Option<CurrencyId>>>::convert(location)
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

match_types! {
	pub type ParentOrParentsPlurality: impl Contains<MultiLocation> = {
		MultiLocation { parents: 1, interior: Here } |
		MultiLocation { parents: 1, interior: X1(Plurality { .. }) }
	};
}

pub type Barrier = (
	TakeWeightCredit,
	AllowTopLevelPaidExecutionFrom<Everything>,
	// Parent and its plurality get free execution
	AllowUnpaidExecutionFrom<ParentOrParentsPlurality>,
	// Expected responses are OK.
	AllowKnownQueryResponses<PolkadotXcm>,
	// Subscriptions for version tracking are OK.
	AllowSubscriptionsFrom<Everything>,
);

pub struct ChargeWeightInFungiblesImplementation;
impl ChargeWeightInFungibles<AccountId, Tokens> for ChargeWeightInFungiblesImplementation {
	fn charge_weight_in_fungibles(
		asset_id: CurrencyId,
		weight: Weight,
	) -> Result<Balance, XcmError> {
		let amount = <WeightToFee as WeightToFeeTrait>::weight_to_fee(&weight);

		// since this is calibrated (in theory) for the native of the relay
		// we should just have a multiplier for relative "value" of that token
		// and adjust the amount inversily proportional to the value
		if let Some(relative_value) = RelayRelativeValue::get_relative_value(asset_id) {
			let adjusted_amount =
				RelativeValue::adjust_amount_by_relative_value(amount, relative_value);
			log::info!("amount to be charged: {:?} in asset: {:?}", adjusted_amount, asset_id);
			return Ok(adjusted_amount)
		} else {
			log::info!("amount to be charged: {:?} in asset: {:?}", amount, asset_id);
			return Ok(amount)
		}
	}
}

/// Means for transacting the currencies of this parachain
type Transactor = MultiCurrencyAdapter<
	Currencies,
	(), // We don't handle unknown assets.
	IsNativeConcrete<CurrencyId, CurrencyIdConvert>,
	AccountId,
	LocationToAccountId,
	CurrencyId,
	CurrencyIdConvert,
	DepositToAlternative<PendulumTreasuryAccount, Currencies, CurrencyId, AccountId, Balance>,
>;

pub type Traders = (
	TakeFirstAssetTrader<
		AccountId,
		ChargeWeightInFungiblesImplementation,
		ConvertedConcreteId<CurrencyId, Balance, CurrencyIdConvert, JustTry>,
		Tokens,
		XcmFeesTo32ByteAccount<Transactor, AccountId, PendulumTreasuryAccount>,
	>,
);

// We will allow for BRZ location from moonbeam
pub struct AutomationPalletConfigPendulum;

impl AutomationPalletConfig for AutomationPalletConfigPendulum {
	fn matches_asset(asset: &MultiAsset) -> Option<u128> {
		let expected_multiloc = moonbeam::BRZ_location();

		match asset {
			MultiAsset {
				id: AssetId::Concrete(loc),
				fun: Fungibility::Fungible(amount_deposited),
			} if loc == &expected_multiloc => return Some(*amount_deposited),
			_ => return None,
		}
	}

	// TODO modify with automation's pallet instance
	fn matches_beneficiary(beneficiary_location: &MultiLocation) -> Option<AssetData> {
		if let MultiLocation {
			parents: 0,
			interior: X2(PalletInstance(99), GeneralKey { length, data }),
		} = beneficiary_location
		{
			let asset_data = AssetData { length: *length, data: *data };
			Some(asset_data)
		} else {
			None
		}
	}

	fn callback(_length: u8, _data: [u8; 32], _amount: u128) -> Result<(), XcmError> {
		// TODO change to call the actual automation pallet, with data and length
		System::remark_with_event(RuntimeOrigin::signed(AccountId::from([0; 32])), [0; 1].to_vec());
		Ok(())
	}
}

pub type LocalAssetTransactor =
	CustomTransactorInterceptor<Transactor, AutomationPalletConfigPendulum>;

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = XcmRouter;
	// How to withdraw and deposit an asset.
	type AssetTransactor = LocalAssetTransactor;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	type IsReserve = MultiNativeAsset<RelativeReserveProvider>;
	// Teleporting is disabled.
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = Barrier;
	type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
	type Trader = Traders;
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
