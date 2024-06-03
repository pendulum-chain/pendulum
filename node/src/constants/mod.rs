use spacewalk_primitives::{Asset, CurrencyId};

pub mod amplitude;
pub mod foucoco;
pub mod pendulum;

// For testnet USDC issued by the testnet issuer
pub const TESTNET_USDC_CURRENCY_ID: CurrencyId = CurrencyId::Stellar(Asset::AlphaNum4 {
	code: *b"USDC",
	issuer: [
		59, 153, 17, 56, 14, 254, 152, 139, 160, 168, 144, 14, 177, 207, 228, 79, 54, 111, 125,
		190, 148, 107, 237, 7, 114, 64, 247, 246, 36, 223, 21, 197,
	],
});
