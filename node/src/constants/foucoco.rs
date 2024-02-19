use runtime_common::{Balance, UNIT};

pub const PARACHAIN_ID: u32 = 2124;
pub const INITIAL_ISSUANCE: Balance = 200_000_000 * UNIT;

pub const INITIAL_ISSUANCE_PER_SIGNATORY: Balance = 200 * UNIT;

pub const INITIAL_COLLATOR_STAKING: Balance = 10_000 * UNIT;
pub const COLLATOR_ADDITIONAL: Balance = 10 * UNIT;

pub const OFF_CHAIN_WORKER_ADDRESS: &str = "6m69vWMouLarYCbJGJisVaDDpfNGETkD5hsDWf2T7osW4Cn1";
pub const ALICIA: &str = "6mfqoTMHrMeVMyKwjqomUjVomPMJ4AjdCm1VReFtk7Be8wqr";


pub const TOKEN_DECIMALS: u32 = 12;

pub const INITIAL_SUDO_SIGNATORIES: [&str; 5] = [
	"6mSy3qQKgAez9zpqY1JSnYW7d1njMNX93P4mkkQvsmPXmehB",
	"6mrdgs7NsHwceSPQRcXCagYzZiB4hoMBGmpMPLA4rS4BGyo7",
	"6jBUR27UemaZBF2aYrEbMuN3u76aANEpA3uxLrQcWP8jNDtf",
	"6hcDDb1nV6zrqfiB7dgQ5DbzuLkPmxkvSZ5LSA9kcE3gxNs8",
	"6k4NQX2KepBkeexrWVNabnWG9GZxvQTYi4ytHHCNwPhLZMnE",
];

pub const INITIAL_COLLATORS: [&str; 4] = [
	"6ihktBwyFJYjE1LKdqoAWzo5VDPJJGso9D5iASZyhuN5JvGH",
	"6mbXa9Qca6B6cX51cbtfWWLhup84rMoMFCxNHjso15GBFyGh",
	"6mMdv2wmb4Cp8PAtDLF1WTh1wLPwPbETwtcjqgJLskdB8EYo",
	"6kL1dzcBJiLgMdAT1qDFD79CLupX1gCCF8RSg5Dh5qRgQeCx",
];
