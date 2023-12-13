use codec::{Decode, Encode, MaxEncodedLen};
use orml_traits::MultiCurrency;
use scale_info::TypeInfo;

pub(crate) type BalanceOf<T> = <<T as orml_currencies::Config>::MultiCurrency as MultiCurrency<
	<T as frame_system::Config>::AccountId,
>>::Balance;

pub(crate) type CurrencyOf<T> = <<T as orml_currencies::Config>::MultiCurrency as MultiCurrency<
	<T as frame_system::Config>::AccountId,
>>::CurrencyId;

pub(crate) type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq)]
pub struct CurrencyDetails<AccountId, Balance> {
	/// Can change `owner`, `issuer` and `admin` accounts.
	pub(super) owner: AccountId,
	/// Can mint tokens.
	pub(super) issuer: AccountId,
	/// Can burn tokens from any account.
	pub(super) admin: AccountId,
	/// Deposit reserved upon takin ownership of the currency
	pub(super) deposit: Balance,
}
