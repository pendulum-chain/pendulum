use spacewalk_primitives::{Asset, CurrencyId};

pub mod amplitude;
pub mod foucoco;
pub mod pendulum;

// For Mainnet USDC issued by the testnet issuer
pub const MAINNET_USDC_CURRENCY_ID: CurrencyId = CurrencyId::Stellar(Asset::AlphaNum4 {
	code: *b"USDC",
	issuer: [
		59, 153, 17, 56, 14, 254, 152, 139, 160, 168, 144, 14, 177, 207, 228, 79, 54, 111, 125,
		190, 148, 107, 237, 7, 114, 64, 247, 246, 36, 223, 21, 197,
	],
});

// For Mainnet BRL issued by the testnet issuer
pub const MAINNET_BRL_CURRENCY_ID: CurrencyId = CurrencyId::Stellar(Asset::AlphaNum4 {
	code: *b"BRL\0",
	issuer: [
		234, 172, 104, 212, 208, 227, 123, 76, 36, 194, 83, 105, 22, 232, 48, 115, 95, 3, 45, 13,
		107, 42, 28, 143, 202, 59, 197, 162, 94, 8, 62, 58,
	],
});

// For Mainnet TZS issued by the testnet issuer
pub const MAINNET_TZS_CURRENCY_ID: CurrencyId = CurrencyId::Stellar(Asset::AlphaNum4 {
	code: *b"TZS\0",
	issuer: [
		52, 201, 75, 42, 75, 169, 232, 181, 123, 34, 84, 125, 203, 179, 15, 68, 60, 76, 176, 45,
		163, 130, 154, 137, 170, 27, 212, 120, 14, 68, 102, 186,
	],
});
