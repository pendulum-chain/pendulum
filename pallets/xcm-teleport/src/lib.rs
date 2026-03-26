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
//! Four layers of protection prevent users from draining the sovereign DOT balance:
//!
//! 1. **Max fee cap** (`MaxFeeAmount`): The `fee_amount` parameter is capped at a
//!    configurable maximum. Any value above this is rejected.
//!
//! 2. **Split deposits**: PEN is deposited to the beneficiary, but leftover DOT (not
//!    consumed by `BuyExecution`) is returned to the sovereign account — not the user.
//!
//! 3. **Minimum teleport amount** (`MinTeleportAmount`): A minimum PEN amount is
//!    required per teleport.
//!
//! 4. **Fee-equivalent PEN charge** (`FeeToNativeConverter`): The `fee_amount` DOT that
//!    will be withdrawn from the sovereign account on AssetHub is converted to PEN-equivalent
//!    using on-chain oracle prices. That PEN amount is transferred from the caller to the
//!    treasury. This ensures every teleport costs the caller the DOT-value of fees in PEN,
//!    making sovereign DOT drainage economically unviable. The fee is only charged on
//!    successful XCM delivery — failed extrinsics refund everything.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use frame_support::pallet_prelude::*;
use sp_runtime::DispatchError;

/// Converts a fee asset amount (in smallest units, e.g., DOT Plancks) to the
/// equivalent amount of the native currency (in smallest units, e.g., PEN Plancks).
///
/// Implementations should use on-chain price oracles and account for decimal
/// differences between the fee asset and the native currency.
///
/// A safety margin may be applied by the implementation to account for price
/// fluctuations between when the conversion is computed and when the XCM message
/// executes on the destination chain.
pub trait FeeToNativeConverter {
	/// The balance type used for the native currency.
	type Balance;

	/// Convert `fee_amount` units of the destination chain's fee asset to the
	/// equivalent amount in the local native currency.
	///
	/// Returns `Err` if the oracle price is unavailable or the conversion overflows.
	fn convert_fee_to_native(fee_amount: u128) -> Result<Self::Balance, DispatchError>;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::traits::{Currency, ExistenceRequirement, Get, WithdrawReasons};
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

		/// Minimum amount of native tokens required per teleport.
		///
		/// This is an anti-griefing measure. Each teleport costs real DOT from the
		/// sovereign account on the destination chain. Without a minimum, an attacker
		/// could spam teleports of dust amounts paying only a tiny transaction fee.
		#[pallet::constant]
		type MinTeleportAmount: Get<BalanceOf<Self>>;

		/// Converts a fee asset amount (DOT Plancks) to the equivalent native currency
		/// amount (PEN Plancks) using on-chain oracle prices.
		///
		/// This is the primary economic protection: the caller must pay PEN equal in
		/// value to the DOT that will be withdrawn from the sovereign account. This
		/// PEN is transferred to the treasury, removing any economic incentive for
		/// griefing attacks.
		type FeeToNativeConverter: FeeToNativeConverter<Balance = BalanceOf<Self>>;

		/// The treasury account that receives the PEN fee equivalent.
		///
		/// When a user teleports PEN to AssetHub, the fee_amount DOT consumed from
		/// the sovereign account is converted to PEN-equivalent and transferred from
		/// the caller to this treasury account.
		type TreasuryAccount: Get<Self::AccountId>;
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
			/// The DOT fee amount requested for execution on AssetHub.
			fee_amount: u128,
			/// The PEN equivalent of the DOT fee, transferred to treasury.
			fee_pen_equivalent: BalanceOf<T>,
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
		/// The teleport amount is below the required minimum (`MinTeleportAmount`).
		/// This minimum exists to prevent griefing attacks that drain the sovereign
		/// account's DOT balance on the destination chain.
		AmountBelowMinimum,
		/// Failed to convert the fee asset amount to native currency using oracle prices.
		/// This can happen if the oracle price is unavailable or the conversion overflows.
		FeeConversionFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: Into<[u8; 32]>,
	{
		/// Teleport native tokens to AssetHub.
		///
		/// This extrinsic:
		/// 1. Withdraws `amount` + fee-PEN from the sender upfront to ensure funds exist.
		/// 2. Sends an XCM message to AssetHub that:
		///    - Withdraws `fee_amount` DOT from this chain's sovereign account for fees.
		///    - Mints `amount` native tokens on AssetHub via `ReceiveTeleportedAsset`.
		///    - Deposits only the native tokens to the `beneficiary`.
		///    - Returns any leftover DOT to the sovereign account.
		/// 3. On success: burns the teleport amount and deposits fee-PEN to treasury.
		/// 4. On failure: refunds everything to the sender.
		///
		/// # Parameters
		/// - `origin`: Must be a signed origin (the sender).
		/// - `amount`: The amount of native tokens to teleport.
		/// - `fee_amount`: The amount of DOT (in Plancks) to use for execution fees
		///   on AssetHub. Must not exceed `MaxFeeAmount`. This DOT is withdrawn
		///   from this chain's sovereign account on AssetHub.
		/// - `beneficiary`: The destination AccountId32 on AssetHub.
		///
		/// # Fees
		///
		/// The caller pays two costs:
		/// 1. The normal Pendulum transaction fee (weight-based).
		/// 2. An additional PEN transfer to treasury equal to the DOT-value of
		///    `fee_amount`, computed via on-chain oracle prices. This compensates
		///    the chain for the sovereign DOT expenditure on AssetHub.
		//
		// Weight: Accounts for Currency::withdraw (x2), oracle reads (x2),
		// XCM message construction, and XcmRouter::validate + deliver.
		// TODO: Replace with proper frame-benchmarking weights.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(400_000_000, 65_000))]
		pub fn teleport_native_to_asset_hub(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
			fee_amount: u128,
			beneficiary: T::AccountId,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			// Validate inputs
			ensure!(amount >= T::MinTeleportAmount::get(), Error::<T>::AmountBelowMinimum);
			ensure!(fee_amount > 0, Error::<T>::ZeroFeeAmount);
			ensure!(fee_amount <= T::MaxFeeAmount::get(), Error::<T>::FeeAmountTooHigh);

			// Convert balance to u128 for XCM
			let amount_u128: u128 =
				amount.try_into().map_err(|_| Error::<T>::AmountConversionFailed)?;

			// Convert the DOT fee_amount to PEN-equivalent using oracle prices.
			let fee_pen_equivalent = T::FeeToNativeConverter::convert_fee_to_native(fee_amount)
				.map_err(|_| Error::<T>::FeeConversionFailed)?;

			log::info!(
				target: "xcm-teleport",
				"Fee conversion: {} DOT plancks => {:?} PEN plancks (will be sent to treasury)",
				fee_amount, fee_pen_equivalent,
			);

			// 1. Withdraw BOTH the fee-equivalent PEN and the teleport amount upfront.
			//    This ensures the sender has sufficient funds for everything before we
			//    attempt the XCM send. Both are refunded if the XCM send fails.

			// Withdraw the fee-equivalent PEN first (KeepAlive so account stays alive
			// for the subsequent teleport amount withdrawal).
			let fee_imbalance = T::Currency::withdraw(
				&sender,
				fee_pen_equivalent,
				WithdrawReasons::TRANSFER,
				ExistenceRequirement::KeepAlive,
			)?;

			// Withdraw the teleport amount (AllowDeath — sender may drain entirely).
			let teleport_imbalance = match T::Currency::withdraw(
				&sender,
				amount,
				WithdrawReasons::TRANSFER,
				ExistenceRequirement::AllowDeath,
			) {
				Ok(imbalance) => imbalance,
				Err(e) => {
					// Refund the fee withdrawal since the teleport withdrawal failed
					T::Currency::resolve_creating(&sender, fee_imbalance);
					return Err(e);
				},
			};

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

			let (ticket, _price) =
				match T::XcmRouter::validate(&mut Some(asset_hub), &mut Some(message)) {
					Ok(result) => result,
					Err(e) => {
						log::error!(
							target: "xcm-teleport",
							"Failed to validate XCM message: {:?}", e
						);
						// Refund everything — XCM was never sent
						T::Currency::resolve_creating(&sender, fee_imbalance);
						T::Currency::resolve_creating(&sender, teleport_imbalance);
						return Err(Error::<T>::XcmSendFailed.into());
					},
				};

			if let Err(e) = T::XcmRouter::deliver(ticket) {
				log::error!(
					target: "xcm-teleport",
					"Failed to deliver XCM message: {:?}", e
				);
				// Refund everything — XCM delivery failed
				T::Currency::resolve_creating(&sender, fee_imbalance);
				T::Currency::resolve_creating(&sender, teleport_imbalance);
				return Err(Error::<T>::XcmSendFailed.into());
			}

			// 4. XCM sent successfully — finalize:
			//    - Drop teleport_imbalance to burn the teleported PEN (removed from supply)
			drop(teleport_imbalance);

			//    - Deposit the fee-equivalent PEN to the treasury account
			let treasury = T::TreasuryAccount::get();
			T::Currency::resolve_creating(&treasury, fee_imbalance);

			// 5. Emit event
			Self::deposit_event(Event::NativeTeleportedToAssetHub {
				sender,
				beneficiary,
				amount,
				fee_amount,
				fee_pen_equivalent,
			});

			Ok(())
		}
	}
}
