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
//! DepositAsset(PEN, beneficiary)           ← only PEN goes to the user
//! DepositAsset(remaining, sovereign_acct)  ← leftover DOT returns to sovereign
//! ```
//!
//! Locally, PEN is withdrawn from the sender's account and burned (removed from circulation).
//! The message is sent via `XcmRouter` from the **parachain origin** (no `DescendOrigin`),
//! so `WithdrawAsset(DOT)` correctly accesses the Pendulum sovereign account on AssetHub.
//!
//! ## Fee Protection
//!
//! Two layers of protection prevent users from draining the sovereign DOT balance:
//!
//! 1. **Max fee cap** (`MaxFeeAmount`): The `fee_amount` parameter is capped at a
//!    configurable maximum. Any value above this is rejected.
//!
//! 2. **Split deposits**: PEN is deposited to the beneficiary, but leftover DOT (not
//!    consumed by `BuyExecution`) is returned to the sovereign account — not the user.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		pallet_prelude::*,
		traits::{Currency, ExistenceRequirement, WithdrawReasons},
	};
	use frame_system::pallet_prelude::*;
	use xcm::v3::{
		prelude::*, Instruction, Junction, Junctions, MultiAsset, MultiAssetFilter, MultiAssets,
		MultiLocation, SendXcm, WeightLimit, WildFungibility, WildMultiAsset, Xcm,
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

		/// The MultiLocation of this chain's sovereign account on the destination,
		/// used to return leftover fee assets after execution.
		/// For Pendulum on AssetHub: `(parents: 0, X1(AccountId32 { network: None, id: sovereign_bytes }))`.
		#[pallet::constant]
		type SovereignAccountOnDest: Get<MultiLocation>;

		/// Maximum fee amount (in fee asset's smallest unit) that can be specified.
		/// This prevents users from draining the sovereign account's fee asset balance.
		#[pallet::constant]
		type MaxFeeAmount: Get<u128>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Native tokens were teleported to AssetHub.
		NativeTeleportedToAssetHub {
			/// The account that initiated the teleport.
			sender: T::AccountId,
			/// The beneficiary account on AssetHub.
			beneficiary: T::AccountId,
			/// The amount of native token teleported.
			amount: BalanceOf<T>,
			/// The amount of DOT used for execution fees on AssetHub.
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
		/// The fee amount exceeds the maximum allowed.
		FeeAmountTooHigh,
		/// Failed to convert the amount to u128.
		AmountConversionFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Into<[u8; 32]>,
	{
		/// Teleport native tokens to AssetHub.
		///
		/// This extrinsic:
		/// 1. Burns `amount` of native tokens from the sender's account on this chain.
		/// 2. Sends an XCM message to AssetHub that:
		///    - Withdraws `fee_amount` DOT from this chain's sovereign account for fees.
		///    - Mints `amount` native tokens on AssetHub via `ReceiveTeleportedAsset`.
		///    - Deposits only the native tokens to the `beneficiary`.
		///    - Returns any leftover DOT to the sovereign account.
		///
		/// # Parameters
		/// - `origin`: Must be a signed origin (the sender).
		/// - `amount`: The amount of native tokens to teleport.
		/// - `fee_amount`: The amount of DOT (in Plancks) to use for execution fees
		///   on AssetHub. Must not exceed `MaxFeeAmount`. This DOT is withdrawn
		///   from this chain's sovereign account on AssetHub.
		/// - `beneficiary`: The destination AccountId32 on AssetHub.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(200_000_000, 10_000))]
		pub fn teleport_native_to_asset_hub(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
			fee_amount: u128,
			beneficiary: T::AccountId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			// Validate inputs
			ensure!(amount > BalanceOf::<T>::from(0u32), Error::<T>::ZeroAmount);
			ensure!(fee_amount > 0, Error::<T>::ZeroFeeAmount);
			ensure!(
				fee_amount <= T::MaxFeeAmount::get(),
				Error::<T>::FeeAmountTooHigh
			);

			// Convert balance to u128 for XCM
			let amount_u128: u128 = amount
				.try_into()
				.map_err(|_| Error::<T>::AmountConversionFailed)?;

			// 1. Withdraw native tokens from the sender's account.
			//    We keep the imbalance and only burn it after successful XCM delivery.
			//    If delivery fails, we refund the tokens back to the sender.
			let imbalance = T::Currency::withdraw(
				&sender,
				amount,
				WithdrawReasons::TRANSFER,
				ExistenceRequirement::AllowDeath,
			)?;

			// 2. Construct the remote XCM message for AssetHub.
			let fee_asset_location = T::FeeAssetOnDest::get();
			let native_asset_on_dest = T::NativeAssetOnDest::get();
			let sovereign_on_dest = T::SovereignAccountOnDest::get();

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
				id: AssetId::Concrete(native_asset_on_dest.clone()),
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
				// Deposit ONLY the native token (PEN) to the beneficiary
				Instruction::DepositAsset {
					assets: MultiAssetFilter::Wild(WildMultiAsset::AllOf {
						id: AssetId::Concrete(native_asset_on_dest),
						fun: WildFungibility::Fungible,
					}),
					beneficiary: beneficiary_location,
				},
				// Return any leftover fee asset (DOT) to the sovereign account
				Instruction::DepositAsset {
					assets: MultiAssetFilter::Wild(WildMultiAsset::All),
					beneficiary: sovereign_on_dest,
				},
			]);

			// 3. Send the message to AssetHub via the XCM router.
			//    Since we call the router directly (not through pallet_xcm::send),
			//    no DescendOrigin is prepended. The message arrives from the
			//    parachain origin, so WithdrawAsset accesses the sovereign account.
			let asset_hub = T::DestinationLocation::get();

			log::info!(
				target: "xcm-teleport",
				"Teleporting native to AssetHub ({:?}): amount={}, fee_amount={}",
				asset_hub, amount_u128, fee_amount,
			);

			let (ticket, _price) = match T::XcmRouter::validate(&mut Some(asset_hub), &mut Some(message)) {
				Ok(result) => result,
				Err(e) => {
					log::error!(
						target: "xcm-teleport",
						"Failed to validate XCM message: {:?}", e
					);
					// Refund the withdrawn tokens back to the sender
					T::Currency::resolve_creating(&sender, imbalance);
					return Err(Error::<T>::XcmSendFailed.into());
				},
			};

			if let Err(e) = T::XcmRouter::deliver(ticket) {
				log::error!(
					target: "xcm-teleport",
					"Failed to deliver XCM message: {:?}", e
				);
				// Refund the withdrawn tokens back to the sender
				T::Currency::resolve_creating(&sender, imbalance);
				return Err(Error::<T>::XcmSendFailed.into());
			}

			// Drop the imbalance to burn the tokens (successful teleport)
			drop(imbalance);

			// 4. Emit event
			Self::deposit_event(Event::NativeTeleportedToAssetHub {
				sender,
				beneficiary,
				amount,
				fee_amount,
			});

			Ok(())
		}
	}
}
