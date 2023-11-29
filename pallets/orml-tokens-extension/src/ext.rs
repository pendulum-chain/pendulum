#[cfg(test)]
use mocktopus::macros::mockable;

#[cfg_attr(test, mockable)]
pub(crate) mod orml_tokens {
	use sp_runtime::DispatchError;
	use crate::BalanceOf;
    use crate::CurrencyOf;
    use crate::AccountIdOf;
    use orml_traits::MultiCurrency;

	pub fn mint<T: crate::Config>(
        currency_id: CurrencyOf<T>,
        who:  &AccountIdOf<T>,
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
        who:  &AccountIdOf<T>,
		amount: BalanceOf<T>,
	) -> Result<(), DispatchError> {
        <orml_currencies::Pallet<T> as MultiCurrency<AccountIdOf<T>>>::withdraw(
            currency_id,
            who,
            amount,
        )?;
        Ok(())
	}

    
}
