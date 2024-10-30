
pub(crate) mod orml_currencies_ext {
	use crate::types::{AccountIdOf, BalanceOf, CurrencyOf};
	use frame_support::traits::BalanceStatus;
	use orml_traits::{MultiCurrency, MultiReservableCurrency};
	use sp_runtime::DispatchError;

	pub fn mint<T: crate::Config>(
		currency_id: CurrencyOf<T>,
		who: &AccountIdOf<T>,
		amount: BalanceOf<T>,
	) -> Result<(), DispatchError> {
		<orml_currencies::Pallet<T> as MultiCurrency<AccountIdOf<T>>>::deposit(
			currency_id,
			who,
			amount,
		)?;
		Ok(())
	}

	pub fn burn<T: crate::Config>(
		currency_id: CurrencyOf<T>,
		who: &AccountIdOf<T>,
		amount: BalanceOf<T>,
	) -> Result<(), DispatchError> {
		<orml_currencies::Pallet<T> as MultiCurrency<AccountIdOf<T>>>::withdraw(
			currency_id,
			who,
			amount,
		)?;
		Ok(())
	}

	pub fn reserve<T: crate::Config>(
		currency_id: CurrencyOf<T>,
		who: &AccountIdOf<T>,
		amount: BalanceOf<T>,
	) -> Result<(), DispatchError> {
		<orml_currencies::Pallet<T> as MultiReservableCurrency<AccountIdOf<T>>>::reserve(
			currency_id,
			who,
			amount,
		)?;
		Ok(())
	}

	// moves the reserved balance from "source" to "destination"
	pub fn repatriate_reserve<T: crate::Config>(
		currency_id: CurrencyOf<T>,
		from: &AccountIdOf<T>,
		to: &AccountIdOf<T>,
		amount: BalanceOf<T>,
	) -> Result<(), DispatchError> {
		<orml_currencies::Pallet<T> as MultiReservableCurrency<AccountIdOf<T>>>::repatriate_reserved(
			currency_id,
			from,
			to,
			amount,
			BalanceStatus::Reserved
		)?;
		Ok(())
	}
}
