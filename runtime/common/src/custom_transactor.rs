use frame_support::traits::Contains;
use sp_std::{marker::PhantomData, result};

use staging_xcm_executor::{traits::TransactAsset, Assets};
use xcm::v3::{prelude::*, Error as XcmError, MultiAsset, MultiLocation, Result};

pub struct AssetData {
	pub length: u8,
	pub data: [u8; 32],
}

pub trait AutomationPalletConfig {
	fn matches_asset(asset: &MultiAsset) -> Option<u128>;
	fn matches_beneficiary(beneficiary_location: &MultiLocation) -> Option<AssetData>;
	fn callback(length: u8, data: [u8; 32], amount: u128) -> Result;
}

/// A wrapper around an inner `TransactAsset` that:
/// 1. Intercepts `deposit_asset` to optionally route to an automation pallet callback.
/// 2. Validates teleport destinations in `can_check_out` against `AllowedTeleportDest`.
///
/// `AllowedTeleportDest` is a `Contains<MultiLocation>` filter that determines which
/// destinations are valid for teleporting assets out of this chain. If a destination
/// is not in the allowed set, `can_check_out` returns an error.
pub struct CustomTransactorInterceptor<
	WrappedTransactor,
	AutomationPalletConfigT,
	AllowedTeleportDest,
>(PhantomData<(WrappedTransactor, AutomationPalletConfigT, AllowedTeleportDest)>);

impl<
		WrappedTransactor: TransactAsset,
		AutomationPalletConfigT: AutomationPalletConfig,
		AllowedTeleportDest: Contains<MultiLocation>,
	> TransactAsset
	for CustomTransactorInterceptor<WrappedTransactor, AutomationPalletConfigT, AllowedTeleportDest>
{
	fn deposit_asset(
		asset: &MultiAsset,
		location: &MultiLocation,
		_context: Option<&XcmContext>,
	) -> Result {
		if let (Some(amount_deposited), Some(asset_data)) = (
			AutomationPalletConfigT::matches_asset(asset),
			AutomationPalletConfigT::matches_beneficiary(location),
		) {
			AutomationPalletConfigT::callback(
				asset_data.length,
				asset_data.data,
				amount_deposited,
			)?;
			return Ok(());
		}

		WrappedTransactor::deposit_asset(asset, location, _context)
	}

	fn withdraw_asset(
		asset: &MultiAsset,
		location: &MultiLocation,
		_maybe_context: Option<&XcmContext>,
	) -> result::Result<Assets, XcmError> {
		WrappedTransactor::withdraw_asset(asset, location, _maybe_context)
	}

	fn transfer_asset(
		asset: &MultiAsset,
		from: &MultiLocation,
		to: &MultiLocation,
		_context: &XcmContext,
	) -> result::Result<Assets, XcmError> {
		WrappedTransactor::transfer_asset(asset, from, to, _context)
	}

	fn can_check_out(
		dest: &MultiLocation,
		_what: &MultiAsset,
		_context: &XcmContext,
	) -> Result {
		// Only allow teleporting assets to destinations in the AllowedTeleportDest set.
		// This prevents users from burning tokens by teleporting to chains that don't
		// recognize this asset as teleportable.
		if !AllowedTeleportDest::contains(dest) {
			log::warn!(
				target: "xcm::custom_transactor",
				"Teleport check-out rejected: destination {:?} is not in the allowed set",
				dest,
			);
			return Err(XcmError::Unroutable);
		}
		Ok(())
	}

	fn check_out(
		_dest: &MultiLocation,
		_what: &MultiAsset,
		_context: &XcmContext,
	) {
		// No-op: the asset was already withdrawn from the sender's account via
		// WithdrawAsset which correctly reduces total issuance via ORML's
		// MultiCurrencyAdapter. No additional accounting needed here.
	}

	fn can_check_in(
		_origin: &MultiLocation,
		_what: &MultiAsset,
		_context: &XcmContext,
	) -> Result {
		// Allow teleport check-in (receiving teleported assets).
		// The origin is already validated by IsTeleporter (TrustedTeleporters)
		// before this method is called.
		Ok(())
	}

	fn check_in(
		_origin: &MultiLocation,
		_what: &MultiAsset,
		_context: &XcmContext,
	) {
		// No-op: the asset will be minted/deposited via deposit_asset which
		// correctly increases total issuance.
	}
}
