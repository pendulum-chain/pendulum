
use sp_runtime::codec::FullCodec;
use sp_runtime::{
	traits::{Convert, MaybeSerializeDeserialize, SaturatedConversion},
};
use sp_std::{
	cmp::{Eq, PartialEq},
	fmt::Debug,
	marker::PhantomData,
	prelude::*,
	result,
};

use xcm::v3::{prelude::*, Error as XcmError, MultiAsset, MultiLocation, Result};
use xcm_executor::{
	traits::{Convert as MoreConvert, MatchesFungible, TransactAsset},
	Assets,
};
use orml_xcm_support::{UnknownAsset as UnknownAssetT,OnDepositFail};


/// Asset transaction errors.
enum Error {
	/// Failed to match fungible.
	FailedToMatchFungible,
	/// `MultiLocation` to `AccountId` Conversion failed.
	AccountIdConversionFailed,
	/// `CurrencyId` conversion failed.
	CurrencyIdConversionFailed,
}

impl From<Error> for XcmError {
	fn from(e: Error) -> Self {
		match e {
			Error::FailedToMatchFungible => XcmError::FailedToTransactAsset("FailedToMatchFungible"),
			Error::AccountIdConversionFailed => XcmError::FailedToTransactAsset("AccountIdConversionFailed"),
			Error::CurrencyIdConversionFailed => XcmError::FailedToTransactAsset("CurrencyIdConversionFailed"),
		}
	}
}

pub trait AutomationPalletConfig {
	fn matches_asset(asset: &MultiAsset) -> Option<u128>;
	fn matches_beneficiary(beneficiary_location: &MultiLocation) -> Option<(u8, [u8;32])>;
	fn callback(length: u8, data: [u8;32], amount: u128) -> Result;
}

#[allow(clippy::type_complexity)]
pub struct CustomMultiCurrencyAdapter<
	MultiCurrency,
	UnknownAsset,
	Match,
	AccountId,
	AccountIdConvert,
	CurrencyId,
	CurrencyIdConvert,
	DepositFailureHandler,
	AutomationPalletConfigT,
>(
	PhantomData<(
		MultiCurrency,
		UnknownAsset,
		Match,
		AccountId,
		AccountIdConvert,
		CurrencyId,
		CurrencyIdConvert,
		DepositFailureHandler,
		AutomationPalletConfigT,
	)>,
);

impl<
		MultiCurrency: orml_traits::MultiCurrency<AccountId, CurrencyId = CurrencyId>,
		UnknownAsset: UnknownAssetT,
		Match: MatchesFungible<MultiCurrency::Balance>,
		AccountId: sp_std::fmt::Debug + Clone,
		AccountIdConvert: MoreConvert<MultiLocation, AccountId>,
		CurrencyId: FullCodec + Eq + PartialEq + Copy + MaybeSerializeDeserialize + Debug,
		CurrencyIdConvert: Convert<MultiAsset, Option<CurrencyId>>,
		DepositFailureHandler: OnDepositFail<CurrencyId, AccountId, MultiCurrency::Balance>,
		AutomationPalletConfigT: AutomationPalletConfig
	> TransactAsset
	for CustomMultiCurrencyAdapter<
		MultiCurrency,
		UnknownAsset,
		Match,
		AccountId,
		AccountIdConvert,
		CurrencyId,
		CurrencyIdConvert,
		DepositFailureHandler,
		AutomationPalletConfigT,
	>
{
	fn deposit_asset(asset: &MultiAsset, location: &MultiLocation, _context: &XcmContext) -> Result {
		if let Some(amount_deposited) = AutomationPalletConfigT::matches_asset(asset){

			if let Some((length,data)) = AutomationPalletConfigT::matches_beneficiary(location){
				AutomationPalletConfigT::callback(length, data, amount_deposited);
				return Ok(());
			}
		}
		match (
			AccountIdConvert::convert_ref(location),
			CurrencyIdConvert::convert(asset.clone()),
			Match::matches_fungible(asset),
		) {
			// known asset
			(Ok(who), Some(currency_id), Some(amount)) => MultiCurrency::deposit(currency_id, &who, amount)
				.or_else(|err| DepositFailureHandler::on_deposit_currency_fail(err, currency_id, &who, amount)),
			// unknown asset
			_ => UnknownAsset::deposit(asset, location)
				.or_else(|err| DepositFailureHandler::on_deposit_unknown_asset_fail(err, asset, location)),
		}
	}

	fn withdraw_asset(
		asset: &MultiAsset,
		location: &MultiLocation,
		_maybe_context: Option<&XcmContext>,
	) -> result::Result<Assets, XcmError> {
		UnknownAsset::withdraw(asset, location).or_else(|_| {
			let who = AccountIdConvert::convert_ref(location)
				.map_err(|_| XcmError::from(Error::AccountIdConversionFailed))?;
			let currency_id = CurrencyIdConvert::convert(asset.clone())
				.ok_or_else(|| XcmError::from(Error::CurrencyIdConversionFailed))?;
			let amount: MultiCurrency::Balance = Match::matches_fungible(asset)
				.ok_or_else(|| XcmError::from(Error::FailedToMatchFungible))?
				.saturated_into();
			MultiCurrency::withdraw(currency_id, &who, amount).map_err(|e| XcmError::FailedToTransactAsset(e.into()))
		})?;

		Ok(asset.clone().into())
	}

	fn transfer_asset(
		asset: &MultiAsset,
		from: &MultiLocation,
		to: &MultiLocation,
		_context: &XcmContext,
	) -> result::Result<Assets, XcmError> {
		let from_account =
			AccountIdConvert::convert_ref(from).map_err(|_| XcmError::from(Error::AccountIdConversionFailed))?;
		let to_account =
			AccountIdConvert::convert_ref(to).map_err(|_| XcmError::from(Error::AccountIdConversionFailed))?;
		let currency_id = CurrencyIdConvert::convert(asset.clone())
			.ok_or_else(|| XcmError::from(Error::CurrencyIdConversionFailed))?;
		let amount: MultiCurrency::Balance = Match::matches_fungible(asset)
			.ok_or_else(|| XcmError::from(Error::FailedToMatchFungible))?
			.saturated_into();
		MultiCurrency::transfer(currency_id, &from_account, &to_account, amount)
			.map_err(|e| XcmError::FailedToTransactAsset(e.into()))?;

		Ok(asset.clone().into())
	}
}

