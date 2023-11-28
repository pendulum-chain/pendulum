use codec::{Decode, Encode, HasCompact, MaxEncodedLen};
use scale_info::TypeInfo;


#[derive(Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct AssetDetails<Balance, AccountId> {
	/// Can change `owner`, `issuer`, `freezer` and `admin` accounts.
	pub(super) owner: AccountId,
	/// Can mint tokens.
	pub(super) issuer: AccountId,
	/// Can thaw tokens, force transfers and burn tokens from any account.
	pub(super) admin: AccountId,
	/// Can freeze tokens.
	pub(super) freezer: AccountId,
	/// The total supply across all accounts.
	pub(super) supply: Balance,
}