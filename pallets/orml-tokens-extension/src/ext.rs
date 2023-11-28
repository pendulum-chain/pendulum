#[cfg(test)]
use mocktopus::macros::mockable;

#[cfg_attr(test, mockable)]
pub(crate) mod orml_tokens {
	use sp_runtime::DispatchError;
	use crate::BalanceOf;
    use crate::CurrencyOf;
    use crate::AccountIdOf;
    use frame_system::Origin as RuntimeOrigin;
    use sp_runtime::traits::Zero;
    use orml_traits::MultiCurrency;

	pub fn mint<T: crate::Config>(
		amount: BalanceOf<T>,
        who:  &AccountIdOf<T>,
        currency_id: CurrencyOf<T>
	) -> Result<(), DispatchError> {
        <orml_currencies::Pallet<T> as MultiCurrency<AccountIdOf<T>>>::deposit(
            currency_id,
            who,
            amount,
        )?;
        Ok(())
	}

    
}
