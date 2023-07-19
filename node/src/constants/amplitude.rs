use runtime_common::{Balance, UNIT};
use spacewalk_primitives::{Asset, CurrencyId};

pub const AMPLITUDE_PARACHAIN_ID: u32 = 2124;
pub const FOUCOCO_PARACHAIN_ID: u32 = 2124;
pub const AMPLITUDE_INITIAL_ISSUANCE: Balance = 200_000_000 * UNIT;

pub const INITIAL_ISSUANCE_PER_SIGNATORY: Balance = 200 * UNIT;

pub const INITIAL_COLLATOR_STAKING: Balance = 10_000 * UNIT;
pub const COLLATOR_ADDITIONAL: Balance = 10 * UNIT;

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

pub const INITIAL_FOUCOCO_VALIDATORS: [&str; 4] = [
	"6ihktBwyFJYjE1LKdqoAWzo5VDPJJGso9D5iASZyhuN5JvGH",
	"6mbXa9Qca6B6cX51cbtfWWLhup84rMoMFCxNHjso15GBFyGh",
	"6mMdv2wmb4Cp8PAtDLF1WTh1wLPwPbETwtcjqgJLskdB8EYo",
	"6kL1dzcBJiLgMdAT1qDFD79CLupX1gCCF8RSg5Dh5qRgQeCx",
];

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
