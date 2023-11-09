#![cfg_attr(not(feature = "std"), no_std)]
#![allow(non_snake_case)]

use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	DispatchError, MultiSignature,
};

pub mod asset_registry;
pub mod chain_ext;
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

pub mod parachains {
	macro_rules! parachain_asset_loc {
		($fn_name:ident) => {
			paste::item! {
				pub fn [< $fn_name _location >] () -> xcm::latest::MultiLocation {
				xcm::latest::MultiLocation {
					parents: 1,
					interior: xcm::latest::Junctions:: X3(
						xcm::latest::Junction::Parachain(PARA_ID),
						xcm::latest::Junction::PalletInstance(ASSET_PALLET_ID),
						xcm::latest::Junction::GeneralIndex([< $fn_name _ASSET_ID >])
					),
				}
			}

			}
		};
	}

	pub mod polkadot {
		pub mod asset_hub {
			pub const PARA_ID: u32 = 1000;
			pub const ASSET_PALLET_ID: u8 = 50;

			pub const USDC_ASSET_ID: u128 = 1337;
			pub const USDT_ASSET_ID: u128 = 1984;

			parachain_asset_loc!(USDC);
			parachain_asset_loc!(USDT);
		}

		pub mod equilibrium {
			pub const PARA_ID: u32 = 2011;
			pub const ASSET_PALLET_ID: u8 = 11;

			pub const EQ_ASSET_ID: u128 = 25_969;
			pub const EQD_ASSET_ID: u128 = 6_648_164;

			parachain_asset_loc!(EQ);
			parachain_asset_loc!(EQD);
		}

		pub mod moonbeam {
			use xcm::latest::{
				Junction::{AccountKey20, PalletInstance, Parachain},
				Junctions::X3,
				MultiLocation,
			};

			pub const PARA_ID: u32 = 2004;
			pub const ASSET_PALLET_ID: u8 = 110;
			// 0xD65A1872f2E2E26092A443CB86bb5d8572027E6E
			// extracted using `H160::from_str("...")` then `as_bytes()`
			pub const BRZ_ASSET_ACCOUNT_IN_BYTES: [u8; 20] = [
				214, 90, 24, 114, 242, 226, 226, 96, 146, 164, 67, 203, 134, 187, 93, 133, 114, 2,
				126, 110,
			];

			pub fn BRZ_location() -> MultiLocation {
				MultiLocation {
					parents: 1,
					interior: X3(
						Parachain(PARA_ID),
						PalletInstance(ASSET_PALLET_ID),
						AccountKey20 { network: None, key: BRZ_ASSET_ACCOUNT_IN_BYTES },
					),
				}
			}
		}

		pub mod polkadex {
			pub const PARA_ID: u32 = 2040;
			pub const ASSET_PALLET_ID: u8 = 25;
		}
	}

	pub mod kusama {
		pub mod asset_hub {
			pub const PARA_ID: u32 = 1000;
			pub const ASSET_PALLET_ID: u8 = 50;

			pub const USDC_ASSET_ID: u128 = 1337;
			pub const USDT_ASSET_ID: u128 = 1984;
		}
	}
}
