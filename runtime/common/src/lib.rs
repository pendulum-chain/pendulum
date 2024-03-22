#![cfg_attr(not(feature = "std"), no_std)]
#![allow(non_snake_case)]

use sp_runtime::{
	traits::{CheckedDiv, IdentifyAccount, Saturating, Verify},
	DispatchError, MultiSignature,
};

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

pub struct RelativeValue<Amount> {
	pub num: Amount,
	pub denominator: Amount,
}

impl<Amount: CheckedDiv<Output = Amount> + Saturating + Clone> RelativeValue<Amount> {
	pub fn divide_by_relative_value(
		amount: Amount,
		relative_value: RelativeValue<Amount>,
	) -> Amount {
		// Calculate the adjusted amount
		if let Some(adjusted_amount) = amount
			.clone()
			.saturating_mul(relative_value.denominator)
			.checked_div(&relative_value.num)
		{
			return adjusted_amount
		}
		// We should never specify a numerator of 0, but just to be safe
		return amount
	}
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

	// These values are shared accross both Pendulum and Kusama
	pub mod asset_hub {
		/// values of kusama asset_hub is similar to polkadot's asset_hub
		pub const PARA_ID: u32 = 1000;
		pub const ASSET_PALLET_INDEX: u8 = 50;

		parachain_asset_location!(USDC, 1337);
		parachain_asset_location!(USDT, 1984);
		parachain_asset_location!(PINK, 23);
	}
	
}

// #[cfg(test)]
// mod tests {
// 	use super::parachains::polkadot::*;
// 	use xcm::{
// 		latest::prelude::{AccountKey20, PalletInstance, Parachain},
// 		prelude::GeneralIndex,
// 	};

// 	#[test]
// 	fn test_BRZ() {
// 		let brz_loc = moonbeam::BRZ_location();
// 		let mut junctions = brz_loc.interior().into_iter();

// 		assert_eq!(junctions.next(), Some(&Parachain(moonbeam::PARA_ID)));
// 		assert_eq!(junctions.next(), Some(&PalletInstance(moonbeam::ASSET_PALLET_INDEX)));
// 		assert_eq!(
// 			junctions.next(),
// 			Some(&AccountKey20 { network: None, key: moonbeam::BRZ_ASSET_ACCOUNT_IN_BYTES })
// 		);
// 		assert_eq!(junctions.next(), None);
// 	}

// 	#[test]
// 	fn test_GLMR() {
// 		let glmr_loc = moonbeam::GLMR_location();
// 		let mut junctions = glmr_loc.interior().into_iter();

// 		assert_eq!(junctions.next(), Some(&Parachain(moonbeam::PARA_ID)));
// 		assert_eq!(junctions.next(), Some(&PalletInstance(moonbeam::BALANCES_PALLET_INDEX)));
// 		assert_eq!(junctions.next(), None);
// 	}

// 	#[test]
// 	fn test_PINK() {
// 		let pink_loc = asset_hub::PINK_location();
// 		let mut junctions = pink_loc.interior().into_iter();

// 		assert_eq!(junctions.next(), Some(&Parachain(asset_hub::PARA_ID)));
// 		assert_eq!(junctions.next(), Some(&PalletInstance(asset_hub::ASSET_PALLET_INDEX)));
// 		assert_eq!(junctions.next(), Some(&GeneralIndex(asset_hub::PINK_ASSET_ID)));
// 		assert_eq!(junctions.next(), None);

// 	}

// 	#[test]
// 	fn test_constants() {
// 		let expected_EQ_value = 25_969;
// 		assert_eq!(equilibrium::EQ_ASSET_ID, expected_EQ_value);

// 		let eq_interior = equilibrium::EQ_location().interior;
// 		let mut junctions = eq_interior.into_iter();

// 		assert_eq!(junctions.next(), Some(Parachain(equilibrium::PARA_ID)));
// 		assert_eq!(junctions.next(), Some(PalletInstance(equilibrium::ASSET_PALLET_INDEX)));
// 		assert_eq!(junctions.next(), Some(GeneralIndex(equilibrium::EQ_ASSET_ID)));
// 		assert_eq!(junctions.next(), None);

// 		let expected_USDT_value = 1984;
// 		assert_eq!(asset_hub::USDT_ASSET_ID, expected_USDT_value);
// 	}
// }
