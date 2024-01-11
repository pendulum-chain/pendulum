use codec::{Decode, Encode, MaxEncodedLen};
// use orml_traits::MultiCurrency;
use scale_info::TypeInfo;

/// Type of amount
#[derive(Copy, Clone, Debug, Encode, Decode, PartialEq, Eq, TypeInfo, MaxEncodedLen)]
pub(crate) enum Amount<Balance> {
	/// Amount of native asset user get for buyout
	Buyout(Balance),
	/// Amount of exchange asset user give for buyout
	Exchange(Balance),
}

pub(crate) const BUYOUT_LIMIT_PERIOD_IN_SEC: u64 = 86400; // 1 day
