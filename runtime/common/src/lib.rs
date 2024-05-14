#![cfg_attr(not(feature = "std"), no_std)]
#![allow(non_snake_case)]

use sp_runtime::{
	traits::{IdentifyAccount, Verify,Convert},
	DispatchError, MultiSignature,
};
use spacewalk_primitives::CurrencyId;
use xcm::v3::{MultiAsset, AssetId, MultiLocation};
use orml_traits::asset_registry::Inspect;
use asset_registry::CustomMetadata;

pub mod asset_registry;
pub mod custom_transactor;
mod proxy_type;
pub mod stellar;
pub mod zenlink;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Type for IDs of farming pools
pub type PoolId = u32;

pub use proxy_type::*;
pub use sp_consensus_aura::sr25519::AuthorityId as AuraId;

/// Balance of an account.
pub type Balance = u128;
pub type Amount = i128;

pub type ReserveIdentifier = [u8; 8];

// Unit = the base number of indivisible units for balances
pub const UNIT: Balance = 1_000_000_000_000;
pub const MILLIUNIT: Balance = 1_000_000_000;
pub const MICROUNIT: Balance = 1_000_000;
pub const NANOUNIT: Balance = 1_000;

/// The existential deposit. Set to 1/10 of the Connected Relay Chain.
pub const EXISTENTIAL_DEPOSIT: Balance = MILLIUNIT;

/// An index to a block.
pub type BlockNumber = u32;

/// Index of a transaction in the chain.
pub type Index = u32;

/// A hash of some data used by the chain.
pub type Hash = sp_core::H256;



/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
	use super::*;
	use sp_runtime::{generic, traits::BlakeTwo256};

	pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;
	/// Opaque block header type.
	pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
	/// Opaque block type.
	pub type Block = generic::Block<Header, UncheckedExtrinsic>;
	/// Opaque block identifier type.
	pub type BlockId = generic::BlockId<Block>;
}

#[macro_use]
pub mod parachains {

	/// Creates a function and a const u8 representation of the value.
	/// # Examples
	/// `create_xcm_id!(PARACHAIN_ASSET,100);`
	///
	/// will look like:
	/// ```
	/// use spacewalk_primitives::CurrencyId;
	///
	/// pub const PARACHAIN_ASSET : u8 = 100;
	/// pub fn PARACHAIN_ASSET_id() -> CurrencyId {
	///    CurrencyId::XCM(PARACHAIN_ASSET)
	/// }
	/// ```
	#[macro_export]
	macro_rules! create_xcm_id {
		($asset_name:ident, $u8_repr:literal) => {
			paste::item! {
				pub const $asset_name :u8 = $u8_repr;

				pub fn [< $asset_name _id >]() -> spacewalk_primitives::CurrencyId {
					spacewalk_primitives::CurrencyId::XCM($asset_name)
				}
			}
		};
	}

	/// Creates a location for the given asset in this format: `fn <asset_name>_location() -> MultiLocation`
	#[macro_export]
	macro_rules! parachain_asset_location {
		// Also declares a constant variable <asset_name>_ASSET_ID with <asset_value>.
		// This assumes that the following constant variables exist:
		// * `PARA_ID` - the parachain id
		// * `ASSET_PALLET_INDEX` - the index of the Assets Pallet
		($asset_name:ident, $asset_index: literal) => {
			paste::item! {
				pub const [< $asset_name _ASSET_ID >] : u128 = $asset_index;

				pub fn [< $asset_name _location >] () -> xcm::latest::MultiLocation {
					xcm::latest::MultiLocation {
						parents: 1,
						interior: xcm::latest::Junctions::X3(
							xcm::latest::Junction::Parachain(PARA_ID),
							xcm::latest::Junction::PalletInstance(ASSET_PALLET_INDEX),
							xcm::latest::Junction::GeneralIndex($asset_index)
						),
					}
				}
			}
		};

		// Accepts the asset name AND the interior of the location
		// mostly for locations that do not use a `GeneralIndex`
		($asset_name:ident, $interiors: expr) => {
			paste::item! {
				pub fn [< $asset_name _location >] () -> xcm::latest::MultiLocation {
					xcm::latest::MultiLocation {
							parents: 1,
							interior: $interiors
						}
				}

			}
		};
	}
}

/// CurrencyIdConvert
/// This type implements conversions from our `CurrencyId` type into `MultiLocation` and vice-versa.
/// A currency locally is identified with a `CurrencyId` variant but in the network it is identified
/// in the form of a `MultiLocation`, in this case a pCfg (Para-Id, Currency-Id).
pub struct CurrencyIdConvert<AssetRegistry>(sp_std::marker::PhantomData<AssetRegistry>);

impl<AssetRegistry: Inspect<AssetId = CurrencyId,Balance = Balance, CustomMetadata = CustomMetadata>> Convert<CurrencyId, Option<MultiLocation>> for CurrencyIdConvert<AssetRegistry> {
	fn convert(id: CurrencyId) -> Option<MultiLocation> {
		<AssetRegistry as Inspect>::metadata(&id)
			.filter(|m| m.location.is_some())
			.and_then(|m| m.location)
			.and_then(|l| l.try_into().ok())
	}
}

impl<AssetRegistry: Inspect<AssetId = CurrencyId,Balance = Balance, CustomMetadata = CustomMetadata>> Convert<MultiLocation, Option<CurrencyId>> for CurrencyIdConvert<AssetRegistry> {
	fn convert(location: MultiLocation) -> Option<CurrencyId>  {
		<AssetRegistry as Inspect>::asset_id(&location)
	}
}

impl<AssetRegistry: Inspect<AssetId = CurrencyId,Balance = Balance, CustomMetadata = CustomMetadata>> Convert<MultiAsset, Option<CurrencyId>> for CurrencyIdConvert<AssetRegistry> {
	fn convert(a: MultiAsset) -> Option<CurrencyId> {
		if let MultiAsset { id: AssetId::Concrete(id), fun: _ } = a {
			<Self as Convert<MultiLocation, Option<CurrencyId>>>::convert(id)
		} else {
			None
		}
	}
}

/// Convert an incoming `MultiLocation` into a `CurrencyId` if possible.
/// Here we need to know the canonical representation of all the tokens we handle in order to
/// correctly convert their `MultiLocation` representation into our internal `CurrencyId` type.
impl<AssetRegistry: Inspect<AssetId = CurrencyId,Balance = Balance, CustomMetadata = CustomMetadata>> xcm_executor::traits::Convert<MultiLocation, CurrencyId> for CurrencyIdConvert<AssetRegistry> {
	fn convert(location: MultiLocation) -> Result<CurrencyId, MultiLocation> {
		<CurrencyIdConvert<AssetRegistry> as Convert<MultiLocation, Option<CurrencyId>>>::convert(location)
			.ok_or(location)
	}
}
