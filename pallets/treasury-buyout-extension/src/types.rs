use codec::{Decode, Encode, MaxEncodedLen};
use orml_traits::MultiCurrency;
use scale_info::TypeInfo;
use crate::Config;

#[allow(type_alias_bounds)]
pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[allow(type_alias_bounds)]
pub(crate) type CurrencyIdOf<T> =
	<<T as orml_currencies::Config>::MultiCurrency as MultiCurrency<
		<T as frame_system::Config>::AccountId
	>>::CurrencyId;

#[allow(type_alias_bounds)]
pub(crate) type BalanceOf<T: Config> = <<T as Config>::Currency as MultiCurrency<AccountIdOf<T>>>::Balance;

/// Type of amount
#[derive(Copy, Clone, Debug, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub enum Amount<Balance> {
	/// Amount of native asset user get for buyout
	Buyout(Balance),
	/// Amount of exchange asset user give for buyout
	Exchange(Balance),
}

pub(crate) const BUYOUT_LIMIT_PERIOD_IN_SEC: u64 = 86400; // 1 day
