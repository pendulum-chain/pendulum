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

pub struct CustomTransactorInterceptor<WrappedTransactor, AutomationPalletConfigT>(
	PhantomData<(WrappedTransactor, AutomationPalletConfigT)>,
);

impl<WrappedTransactor: TransactAsset, AutomationPalletConfigT: AutomationPalletConfig>
	TransactAsset for CustomTransactorInterceptor<WrappedTransactor, AutomationPalletConfigT>
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
}
