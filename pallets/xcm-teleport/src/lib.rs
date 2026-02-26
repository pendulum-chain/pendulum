//! # XCM Teleport Pallet
//!
//! A pallet that enables teleporting the native PEN token from Pendulum to AssetHub
//! with the correct XCM message ordering that passes AssetHub's barrier.
//!
//! ## Problem
//!
//! The standard `pallet_xcm::limitedTeleportAssets` uses `InitiateTeleport` which always
//! prepends `ReceiveTeleportedAsset` to the inner XCM. This produces a message ordering
//! that AssetHub's barrier rejects when DOT is needed for fees (PEN is not fee-payable on
//! AssetHub).
//!
//! ## Solution
//!
//! This pallet constructs the remote XCM message manually with the correct ordering:
//! ```text
//! WithdrawAsset(DOT)          ← from Pendulum's sovereign account on AssetHub
//! BuyExecution(DOT)           ← passes the barrier
//! ReceiveTeleportedAsset(PEN) ← mints PEN on AssetHub
//! ClearOrigin
//! DepositAsset(All, beneficiary)
//! ```
//!
//! Locally, PEN is withdrawn from the sender's account and burned (removed from circulation).
//! The message is sent via `XcmRouter` from the **parachain origin** (no `DescendOrigin`),
//! so `WithdrawAsset(DOT)` correctly accesses the Pendulum sovereign account on AssetHub.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		pallet_prelude::*,
		traits::{Currency, ExistenceRequirement, WithdrawReasons},
	};
	use frame_system::pallet_prelude::*;
	use sp_std::vec;
	use xcm::v3::{
		prelude::*, Instruction, Junction, Junctions, MultiAsset, MultiAssetFilter, MultiAssets,
		MultiLocation, SendXcm, WeightLimit, WildMultiAsset, Xcm,
	};

	type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching runtime event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The native currency (PEN / Balances pallet).
		type Currency: Currency<Self::AccountId>;

		/// The XCM router used to send messages to other chains.
		type XcmRouter: SendXcm;

		/// The MultiLocation of the destination chain (AssetHub) relative to this chain.
		/// For Pendulum → AssetHub: `(Parent, Parachain(1000))`.
		#[pallet::constant]
		type DestinationLocation: Get<MultiLocation>;

		/// The MultiLocation of the native token as seen from the destination chain.
		/// For PEN on AssetHub: `(parents: 1, X2(Parachain(2094), PalletInstance(10)))`.
		#[pallet::constant]
		type NativeAssetOnDest: Get<MultiLocation>;

		/// The MultiLocation of the fee asset (DOT) as seen from the destination chain.
		/// For DOT on AssetHub: `(parents: 1, Here)`.
		#[pallet::constant]
		type FeeAssetOnDest: Get<MultiLocation>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Native tokens were teleported to the destination chain.
		TeleportedNativeTo {
			/// The account that initiated the teleport.
			sender: T::AccountId,
			/// The beneficiary account on the destination chain.
			beneficiary: T::AccountId,
			/// The amount of native token teleported.
			amount: BalanceOf<T>,
			/// The amount of fee asset (DOT) used for execution on the destination.
			fee_amount: u128,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Failed to send the XCM message to the destination chain.
		XcmSendFailed,
		/// The teleport amount must be greater than zero.
		ZeroAmount,
		/// The fee amount must be greater than zero.
		ZeroFeeAmount,
		/// Failed to convert the amount to u128.
		AmountConversionFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Into<[u8; 32]>,
	{
		/// Teleport native tokens to the destination chain (AssetHub).
		///
		/// This extrinsic:
		/// 1. Burns `amount` of native tokens from the sender's account.
		/// 2. Sends an XCM message to the destination that:
		///    - Withdraws `fee_amount` of the fee asset (DOT) from this chain's
		///      sovereign account for execution fees.
		///    - Mints `amount` native tokens on the destination via `ReceiveTeleportedAsset`.
		///    - Deposits all assets to the `beneficiary`.
		///
		/// # Parameters
		/// - `origin`: Must be a signed origin (the sender).
		/// - `amount`: The amount of native tokens to teleport.
		/// - `fee_amount`: The amount of the fee asset (DOT) to use for execution fees
		///   on the destination. This is withdrawn from this chain's sovereign account.
		/// - `beneficiary`: The destination AccountId32 on the destination chain.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(200_000_000, 10_000))]
		pub fn teleport_native_to_dest(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
			fee_amount: u128,
			beneficiary: T::AccountId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			// Validate inputs
			ensure!(amount > BalanceOf::<T>::from(0u32), Error::<T>::ZeroAmount);
			ensure!(fee_amount > 0, Error::<T>::ZeroFeeAmount);

			// Convert balance to u128 for XCM
			let amount_u128: u128 = amount
				.try_into()
				.map_err(|_| Error::<T>::AmountConversionFailed)?;

			// 1. Withdraw and burn native tokens from the sender's account.
			//    Dropping the NegativeImbalance burns the tokens (reduces total issuance).
			let _imbalance = T::Currency::withdraw(
				&sender,
				amount,
				WithdrawReasons::TRANSFER,
				ExistenceRequirement::AllowDeath,
			)?;
			// _imbalance is dropped here → tokens are burned

			// 2. Construct the remote XCM message for the destination chain.
			let fee_asset_location = T::FeeAssetOnDest::get();
			let native_asset_on_dest = T::NativeAssetOnDest::get();

			let beneficiary_bytes: [u8; 32] = beneficiary.clone().into();
			let beneficiary_location = MultiLocation {
				parents: 0,
				interior: Junctions::X1(Junction::AccountId32 {
					network: None,
					id: beneficiary_bytes,
				}),
			};

			let fee_multi_asset = MultiAsset {
				id: AssetId::Concrete(fee_asset_location),
				fun: Fungibility::Fungible(fee_amount),
			};

			let native_multi_asset = MultiAsset {
				id: AssetId::Concrete(native_asset_on_dest),
				fun: Fungibility::Fungible(amount_u128),
			};

			let message: Xcm<()> = Xcm(vec![
				// Withdraw fee asset (DOT) from this chain's sovereign account
				Instruction::WithdrawAsset(MultiAssets::from(vec![fee_multi_asset.clone()])),
				// Pay for execution with the fee asset — this passes the barrier
				Instruction::BuyExecution {
					fees: fee_multi_asset,
					weight_limit: WeightLimit::Unlimited,
				},
				// Mint the teleported native tokens on the destination
				Instruction::ReceiveTeleportedAsset(MultiAssets::from(vec![native_multi_asset])),
				// Remove origin to prevent further privileged operations
				Instruction::ClearOrigin,
				// Deposit everything (native token + leftover DOT) to the beneficiary
				Instruction::DepositAsset {
					assets: MultiAssetFilter::Wild(WildMultiAsset::All),
					beneficiary: beneficiary_location,
				},
			]);

			// 3. Send the message to the destination via the XCM router.
			//    Since we call the router directly (not through pallet_xcm::send),
			//    no DescendOrigin is prepended. The message arrives from the
			//    parachain origin, so WithdrawAsset accesses the sovereign account.
			let dest = T::DestinationLocation::get();

			log::info!(
				target: "xcm-teleport",
				"Sending teleport message to {:?}: amount={}, fee_amount={}",
				dest, amount_u128, fee_amount,
			);

			let (ticket, _price) = T::XcmRouter::validate(&mut Some(dest), &mut Some(message))
				.map_err(|e| {
					log::error!(
						target: "xcm-teleport",
						"Failed to validate XCM message: {:?}", e
					);
					Error::<T>::XcmSendFailed
				})?;

			T::XcmRouter::deliver(ticket).map_err(|e| {
				log::error!(
					target: "xcm-teleport",
					"Failed to deliver XCM message: {:?}", e
				);
				Error::<T>::XcmSendFailed
			})?;

			// 4. Emit event
			Self::deposit_event(Event::TeleportedNativeTo {
				sender,
				beneficiary,
				amount,
				fee_amount,
			});

			Ok(())
		}
	}
}
