use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq)]
pub struct CurrencyDetails<AccountId> {
	/// Can change `owner`, `issuer` and `admin` accounts.
	pub(super) owner: AccountId,
	/// Can mint tokens.
	pub(super) issuer: AccountId,
	/// Can burn tokens from any account.
	pub(super) admin: AccountId,
}
