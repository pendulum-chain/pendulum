use runtime_common::{Balance, UNIT};
use spacewalk_primitives::{oracle::Key, Asset, CurrencyId, CurrencyId::XCM, VaultCurrencyPair};

pub const AMPLITUDE_PARACHAIN_ID: u32 = 2124;
pub const AMPLITUDE_INITIAL_ISSUANCE: Balance = 200_000_000 * UNIT;
pub const INITIAL_ISSUANCE_PER_SIGNATORY: Balance = 200 * UNIT;
pub const INITIAL_COLLATOR_STAKING: Balance = 10_010 * UNIT;

pub const OFF_CHAIN_WORKER_ADDRESS: &str = "6m69vWMouLarYCbJGJisVaDDpfNGETkD5hsDWf2T7osW4Cn1";

pub const TOKEN_DECIMALS: u32 = 12;

pub const INITIAL_AMPLITUDE_SUDO_SIGNATORIES: [&str; 5] = [
	"6nJwMD3gk36fe6pMRL2UpbwAEjDdjjxdngQGShe753pyAvCT",
	"6i4xDEE1Q2Bv8tnJtgD4jse4YTAzzCwCJVUehRQ93hCqKp8f",
	"6n62KZWvmZHgeyEXTvQFmoHRMqjKfFWvQVsApkePekuNfek5",
	"6kwxQBRKMadrY9Lq3K8gZXkw1UkjacpqYhcqvX3AqmN9DofF",
	"6kKHwcpCVC18KepwvdMSME8Q7ZTNr1RoRUrFDH9AdAmhL3Pt",
];

pub const INITIAL_AMPLITUDE_VALIDATORS: [&str; 8] = [
	"6mTATq7Ug9RPk4s8aMv5H7WVZ7RvwrJ1JitbYMXWPhanzqiv",
	"6n8WiWqjEB8nCNRo5mxXc89FqhuMd2dgXNSrzuPxoZSnatnL",
	"6ic56zZmjqo746yifWzcNxxzxLe3pRo8WNitotniUQvgKnyU",
	"6gvFApEyYj4EavJP26mwbVu7YxFBYZ9gaJFB7gv5gA6vNfze",
	"6mz3ymVAsfHotEhHphVRvLLBhMZ2frnwbuvW5QZiMRwJghxE",
	"6mpD3zcHcUBkxCjTsGg2tMTfmQZdXLVYZnk4UkN2XAUTLkRe",
	"6mGcZntk59RK2JfxfdmprgDJeByVUgaffMQYkp1ZeoEKeBJA",
	"6jq7obxC7AxhWeJNzopwYidKNNe48cLrbGSgB2zs2SuRTWGA",
];

// For Testnet Stellar Native issued by the testnet issuer
pub const TESTNET_STELLAR_NATIVE_CURRENCY_ID: CurrencyId = CurrencyId::Stellar(Asset::StellarNative);

// For Testnet USDC issued by the testnet issuer
pub const TESTNET_USDC_CURRENCY_ID: CurrencyId = CurrencyId::Stellar(Asset::AlphaNum4{
	code: *b"USDC",
	issuer: [
		20, 209, 150, 49, 176, 55, 23, 217, 171, 154, 54, 110, 16, 50, 30, 226, 102, 231, 46, 199,
		108, 171, 97, 144, 240, 161, 51, 109, 72, 34, 159, 139,
	],
});

// For Testnet BRL issued by the testnet issuer
pub const TESTNET_BRL_CURRENCY_ID: CurrencyId = CurrencyId::Stellar(Asset::AlphaNum4{
	code: *b"BRL\0",
	issuer: [
		20, 209, 150, 49, 176, 55, 23, 217, 171, 154, 54, 110, 16, 50, 30, 226, 102, 231, 46, 199,
		108, 171, 97, 144, 240, 161, 51, 109, 72, 34, 159, 139,
	],
});

// For Testnet TZS issued by the testnet issuer
pub const TESTNET_TZS_CURRENCY_ID: CurrencyId = CurrencyId::Stellar(Asset::AlphaNum4{
	code: *b"TZS\0",
	issuer: [
		20, 209, 150, 49, 176, 55, 23, 217, 171, 154, 54, 110, 16, 50, 30, 226, 102, 231, 46, 199,
		108, 171, 97, 144, 240, 161, 51, 109, 72, 34, 159, 139,
	],
});

pub const FOUCOCO_PARACHAIN_ID: u32 = 2124;