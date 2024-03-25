#![cfg_attr(not(feature = "std"), no_std)]
#![allow(non_snake_case)]

use sp_runtime::{
	traits::{IdentifyAccount, Verify,Convert},
	DispatchError, MultiSignature,
};
use frame_support::pallet_prelude::Get;
use cumulus_primitives_core::ParaId;
use spacewalk_primitives::CurrencyId;
use xcm::v3::{MultiAsset, AssetId, Junction::Parachain, MultiLocation};
use orml_asset_registry::AssetMetadata;
use asset_registry::CustomMetadata;

pub mod asset_registry;
pub mod chain_ext;
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

	pub mod polkadot {
		pub mod asset_hub {
			pub const PARA_ID: u32 = 1000;
			pub const ASSET_PALLET_INDEX: u8 = 50;

			parachain_asset_location!(USDC, 1337);
			parachain_asset_location!(USDT, 1984);
			parachain_asset_location!(PINK, 23);
		}

		pub mod equilibrium {
			pub const PARA_ID: u32 = 2011;
			pub const ASSET_PALLET_INDEX: u8 = 11;

			parachain_asset_location!(EQ, 25_969);
			parachain_asset_location!(EQD, 6_648_164);
		}

		pub mod moonbeam {
			use xcm::latest::{
				Junction::{AccountKey20, PalletInstance, Parachain},
				Junctions::{X2, X3},
			};

			pub const PARA_ID: u32 = 2004;
			pub const ASSET_PALLET_INDEX: u8 = 110;
			pub const BALANCES_PALLET_INDEX: u8 = 10;

			// The address of the BRZ token on Moonbeam `0x3225edCe8aD30Ae282e62fa32e7418E4b9cf197b` as byte array
			pub const BRZ_ASSET_ACCOUNT_IN_BYTES: [u8; 20] = [
				50, 37, 237, 206, 138, 211, 10, 226, 130, 230, 47, 163, 46, 116, 24, 228, 185, 207,
				25, 123,
			];

			parachain_asset_location!(
				BRZ,
				X3(
					Parachain(PARA_ID),
					PalletInstance(ASSET_PALLET_INDEX),
					AccountKey20 { network: None, key: BRZ_ASSET_ACCOUNT_IN_BYTES }
				)
			);

			parachain_asset_location!(
				GLMR,
				X2(Parachain(PARA_ID), PalletInstance(BALANCES_PALLET_INDEX))
			);
		}

		pub mod polkadex {
			use xcm::latest::{Junction::Parachain, Junctions::X1};

			pub const PARA_ID: u32 = 2040;

			parachain_asset_location!(PDEX, X1(Parachain(PARA_ID)));
		}
	}

	pub mod kusama {
		/// values of kusama asset_hub is similar to polkadot's asset_hub
		pub mod asset_hub {
			pub use super::super::polkadot::asset_hub::*;
		}
	}

	pub mod moonbase_alpha_relay {
		pub mod moonbase_alpha {
			use xcm::latest::{
				Junction::{PalletInstance, Parachain},
				Junctions::X2,
			};

			pub const PARA_ID: u32 = 1000;
			pub const BALANCES_PALLET_INDEX: u8 = 3;

			parachain_asset_location!(
				DEV,
				X2(Parachain(PARA_ID), PalletInstance(BALANCES_PALLET_INDEX))
			);
		}
	}
}

/// CurrencyIdConvert
/// This type implements conversions from our `CurrencyId` type into `MultiLocation` and vice-versa.
/// A currency locally is identified with a `CurrencyId` variant but in the network it is identified
/// in the form of a `MultiLocation`, in this case a pCfg (Para-Id, Currency-Id).
pub struct CurrencyIdConvert<ParachainId, AssetRegistry>(sp_std::marker::PhantomData<(ParachainId, AssetRegistry)>);

impl<ParachainId: Get<ParaId>, AssetRegistryInspector: AssetRegistryInspect> Convert<CurrencyId, Option<MultiLocation>> for CurrencyIdConvert<ParachainId, AssetRegistryInspector> {
	fn convert(id: CurrencyId) -> Option<MultiLocation> {
		AssetRegistryInspector::metadata(&id)
			.filter(|m| m.location.is_some())
			.and_then(|m| m.location)
			.and_then(|l| l.try_into().ok())
	}
}

impl<ParachainId: Get<ParaId>, AssetRegistryInspector: AssetRegistryInspect> Convert<MultiLocation, Option<CurrencyId>> for CurrencyIdConvert<ParachainId, AssetRegistryInspector> {
	fn convert(location: MultiLocation) -> Option<CurrencyId>  {
		let para_id = ParachainId::get();

		let unanchored_location = match location {
            MultiLocation { parents: 0, interior } => {
     
                match interior.pushed_front_with(Parachain(u32::from(para_id))) {
                    Ok(new_interior) => MultiLocation {
                        parents: 1,
                        interior: new_interior,
                    },
                    Err(_) => return None, 
                }
            },
            x => x,
        };
		AssetRegistryInspector::location_to_asset_id(unanchored_location)
	}
}

impl<ParachainId: Get<ParaId>, AssetRegistryInspector: AssetRegistryInspect> Convert<MultiAsset, Option<CurrencyId>> for CurrencyIdConvert<ParachainId, AssetRegistryInspector> {
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
impl<ParachainId: Get<ParaId>, AssetRegistryInspector: AssetRegistryInspect> xcm_executor::traits::Convert<MultiLocation, CurrencyId> for CurrencyIdConvert<ParachainId, AssetRegistryInspector> {
	fn convert(location: MultiLocation) -> Result<CurrencyId, MultiLocation> {
		<CurrencyIdConvert<ParachainId, AssetRegistryInspector> as Convert<MultiLocation, Option<CurrencyId>>>::convert(location)
			.ok_or(location)
	}
}

pub trait AssetRegistryInspect {
    fn location_to_asset_id(multilocation: MultiLocation) ->  Option<CurrencyId>;
	fn metadata(id: &CurrencyId) -> Option<AssetMetadata<Balance, CustomMetadata>>;
}

#[cfg(test)]
mod tests {
	use super::parachains::polkadot::*;
	use xcm::{
		latest::prelude::{AccountKey20, PalletInstance, Parachain},
		prelude::GeneralIndex,
	};

	#[test]
	fn test_BRZ() {
		let brz_loc = moonbeam::BRZ_location();
		let mut junctions = brz_loc.interior().into_iter();

		assert_eq!(junctions.next(), Some(&Parachain(moonbeam::PARA_ID)));
		assert_eq!(junctions.next(), Some(&PalletInstance(moonbeam::ASSET_PALLET_INDEX)));
		assert_eq!(
			junctions.next(),
			Some(&AccountKey20 { network: None, key: moonbeam::BRZ_ASSET_ACCOUNT_IN_BYTES })
		);
		assert_eq!(junctions.next(), None);
	}

	#[test]
	fn test_GLMR() {
		let glmr_loc = moonbeam::GLMR_location();
		let mut junctions = glmr_loc.interior().into_iter();

		assert_eq!(junctions.next(), Some(&Parachain(moonbeam::PARA_ID)));
		assert_eq!(junctions.next(), Some(&PalletInstance(moonbeam::BALANCES_PALLET_INDEX)));
		assert_eq!(junctions.next(), None);
	}

	#[test]
	fn test_PINK() {
		let pink_loc = asset_hub::PINK_location();
		let mut junctions = pink_loc.interior().into_iter();

		assert_eq!(junctions.next(), Some(&Parachain(asset_hub::PARA_ID)));
		assert_eq!(junctions.next(), Some(&PalletInstance(asset_hub::ASSET_PALLET_INDEX)));
		assert_eq!(junctions.next(), Some(&GeneralIndex(asset_hub::PINK_ASSET_ID)));
		assert_eq!(junctions.next(), None);

	}

	#[test]
	fn test_constants() {
		let expected_EQ_value = 25_969;
		assert_eq!(equilibrium::EQ_ASSET_ID, expected_EQ_value);

		let eq_interior = equilibrium::EQ_location().interior;
		let mut junctions = eq_interior.into_iter();

		assert_eq!(junctions.next(), Some(Parachain(equilibrium::PARA_ID)));
		assert_eq!(junctions.next(), Some(PalletInstance(equilibrium::ASSET_PALLET_INDEX)));
		assert_eq!(junctions.next(), Some(GeneralIndex(equilibrium::EQ_ASSET_ID)));
		assert_eq!(junctions.next(), None);

		let expected_USDT_value = 1984;
		assert_eq!(asset_hub::USDT_ASSET_ID, expected_USDT_value);
	}
}
