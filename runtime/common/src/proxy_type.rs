use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use sp_runtime::RuntimeDebug;
/// The type used to represent the kinds of proxying allowed.
#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	RuntimeDebug,
	MaxEncodedLen,
	scale_info::TypeInfo,
)]
pub enum ProxyType {
	/// Allows all runtime calls for proxy account
	Any,
}

impl Default for ProxyType {
	fn default() -> Self {
		Self::Any
	}
}
