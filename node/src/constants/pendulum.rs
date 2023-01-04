use runtime_common::{Balance, UNIT};

pub const PARACHAIN_ID: u32 = 2094;
pub const TOTAL_INITIAL_ISSUANCE: Balance = 160_000_000 * UNIT;

pub const INITIAL_ISSUANCE_PER_SIGNATORY: Balance = 200 * UNIT;

pub const INITIAL_COLLATOR_STAKING: Balance = 5_000 * UNIT;
pub const COLLATOR_ADDITIONAL: Balance = 10 * UNIT;

pub const SUDO_SIGNATORIES: [&str; 5] = [
	"6bgxxegcuBCYngSkJqp7bzgVZWd7brVrABEQetFq1R5dNa7T",
	"6cm5M5JVknj4NQdWpdayqwm92wzvSzjgE5SCRS7HM1EoauLm",
	"6gkRvy75f8gngVgtbJ72WHWPf17xgFGHuE2h2vKce5cnBidw",
	"6eD6Ep2GxAsPrCLNMtPNEv2Nj9D3EzJsPzMBGz8R8JGgNXg3",
	"6fvm48ZH2NYvBasbFLm8r2t33K9tVL6y9Z14HWiaoAYr4WNi",
];

pub const MULTISIG_ID_GENESIS: &str = "6ce4KspfCTmRnDzpQ3JYFYGPDgoqph6NYnqNTe58b86tusEn";
pub const MULTISIG_ID_TEAM: &str = "6diKWq553r9jYkuyeWLd7YLU36ovc71puFKEq4ayqChBUQAL";
pub const MULTISIG_ID_CL_RESERVES: &str = "6fMjg9qf8r6wZNJB71k3x7Gm7QRkFkjUsJq9ZuhQucav3sBZ";
pub const MULTISIG_ID_INCENTIVES: &str = "6dzVMr6dud6Qt5ztG9T3iscv7f5jN9N26PfGAqJq8EjHGkcd";
pub const MULTISIG_ID_MARKETING: &str = "6dxAdKt9zGDjUeHHcsi5U1fgxT1HKcHVjwoTANRPWjHd7Q1U";

pub const CL_RESERVES_ALLOCATION: Balance = 10_000_000 * UNIT;
pub const INCENTIVES_ALLOCATION: Balance = 8_000_000 * UNIT;
pub const MARKETING_ALLOCATION: Balance = 10_000_000 * UNIT;
pub const TREASURY_ALLOCATION: Balance = 26_000_000 * UNIT;

pub const INITIAL_COLLATORS: [&str; 8] = [
	"6gUmMnikYxEkk4H7RdnsLRrzNRuDrGAh8JgSiCghG39qenX9",
	"6cgKZANaeUJ42VC7iAXrTzX8NC2gdn4WmYAHRo1RBjBfVvnk",
	"6bh2t6KMJ9BKgCs1B6qcrp5BjMyv2azmgBC6ySwZ3wrTeW5s",
	"6bBH94XAkscX5Q1oswuPSenUzjb9f2iPcfhTKdu1XCK1uwVS",
	"6emSrvAgGZXGBu255njQg3pBxDyQN47T7H2XDZuS5V5epHaX",
	"6fciE2ek1AMFUaFm4nizaHEZtXBy6eRxEcoygr3SFKfddBBK",
	"6ftBtHvYrThAv1xHYDnYrm2qQLFcj2rhkaU5GqNuqvKp57v6",
	"6feqfoP5htFpSriTd9oomDa1dZDmcM4XpjKEq8dfdcADCfGt",
];

pub struct Allocation {
	pub address: &'static str,
	pub amount: Balance,
}

pub const ALLOCATIONS_10_24: [Allocation; 33] = [
	Allocation { address: "16LZddCHy8Td5p1T7hN52k1yPgS5Kvi4FbcN1qmgpEDLrmxW", amount: 200000 },
	Allocation { address: "124pJ5gdqKDXL3VjryYZiPBTTmtia6rEAGfUbHaf8LNToqZE", amount: 2000000 },
	Allocation { address: "15SvDT9AEzWbZU96Ea6KtPgFVvH1R1DqFDmUi9pYPr1ZymLG", amount: 280000 },
	Allocation { address: "16Zm7ctGjbVsADdPP8x8M6X6KXQxdzwjncWXbhFmQgGVQ1k6", amount: 1200000 },
	Allocation { address: "1kM6D9EgjD6VZQTZoEEVdANhaQFFeVtvowzVFpgv5DbqySk", amount: 553480 },
	Allocation { address: "13QdwVMsKiN8DVvjhwUSg3zrfFAPKvd1nPP3FbtKacwZGZW3", amount: 2960000 },
	Allocation { address: "1RyThWA1SCv3EGayyC2n74Cz28Prh7cYHZih5Hc1eCk157N", amount: 1900000 },
	Allocation { address: "15uXmzkjdBh9oJzBea1cTK1jw79djMwfw6845DyCiFQJzr17", amount: 400000 },
	Allocation { address: "4sVT7n5xGBq4X4Nyr1oPmw7qyKpxFZfB3PyuSPa44uFwh2oa", amount: 1080000 },
	Allocation { address: "13wbnsaCot1JqtP7HS3ASjgs5n2X2fzGC2TswSjyEmThFkjv", amount: 1000000 },
	Allocation { address: "12xmShQ49VKcmWNoMovChvUP3ar3muvFtkcf7PvPwkAk2V31", amount: 64000 },
	Allocation { address: "14s3dQEfCB9yaexpQFN4C5ZmkvkXwEdxrv9NToSsBP7rCegG", amount: 100000 },
	Allocation { address: "5EhUWtF2ZJpHMbG9G65LUjAtBA48zCV5PCWXibZKwZvzNKQa", amount: 160000 },
	Allocation { address: "1nHjrXTmso9AuGnssYRYxJnBf8NbwFQyhrb7Svk7FcvrvH3", amount: 276800 },
	Allocation { address: "5DhWaXkGNJVg1FU15myWRcSLEpotK4wz4fjBp7qtcmnDtSJ6", amount: 800000 },
	Allocation { address: "136X6NEWZEBQscsCaocy5DT328KHpxFWwR6Z54fHPLRVuVu3", amount: 800000 },
	Allocation { address: "1254xvMjWNvYGn7vB1vcSLS4gL1VUiJL8BHu8NWsrG1jkHi8", amount: 200000 },
	Allocation { address: "5CAR4QkAD2pxc9uXT2SzLJjpKPGDfxqFVVeQKGB4ixXVL74c", amount: 800000 },
	Allocation { address: "14uUXVDrEjUSv9Ec22w6GQ3keXRxNu98RZ39z5Xz2FWHzwCC", amount: 1840000 },
	Allocation { address: "16XjrAX12Drvcz3daqjoLJmCTP3dBC8zxS286Zx9G9am66qm", amount: 440000 },
	Allocation { address: "158kU5QMgc74aQniPbBZwaezXD62GbHLv59CSqB3KnjNy8sV", amount: 160000 },
	Allocation { address: "5CaosCeDNkCMGV4CQHy2YNia5zmvV4EmVv2aTg2jzNcxejFn", amount: 960000 },
	Allocation { address: "15Dg5uQM8fuckTSfLnEC5cTyyBMbNbkwx4FJSoFshwQwqvRr", amount: 1200000 },
	Allocation { address: "15E56zSiZXmZNyaLAmhwo8icCqQWgWJpunytkideS9ZQSNLT", amount: 1200000 },
	Allocation { address: "5GVtKE3KP2pJpK8HjeDTsSMMYgciHZpP4HNj8E2bL2wHmtvW", amount: 120000 },
	Allocation { address: "12gmcL9eej9jRBFT26vZLF4b7aAe4P9aEYHGHFzJdmf5arPi", amount: 200000 },
	Allocation { address: "152ryYwv8LaNCEA5v1kr5hPQF8LLCEq1xuJmykMD2dbbDBiM", amount: 200000 },
	Allocation { address: "5EHtSzWCq4aaC2XzPRmbPEdwV4Uty7pKqukVTdeEygVpEJxf", amount: 400000 },
	Allocation { address: "153GQ6MRcZiSwRy7xEy3X8kQybdFMov9TWvQr8NDmJLzoFan", amount: 80000 },
	Allocation { address: "12KdQaJLMovA6q6j6jGbooMuCuDPz1PWtTLtT5ExCmwKbNJ6", amount: 80000 },
	Allocation { address: "5ELsgzLeCyCBBPQ5yimZ9aXiLBmvHkcTTK6iP9TdtaS3BvMp", amount: 80000 },
	Allocation { address: "5G45b78emoZ1eLmu4FCarmhhaxsRNmLZKuQmXE6tLnUGyFSC", amount: 80000 },
	Allocation { address: "156DACh3KCPcFbxQmdQeWZD7B1TywvRviSQwcE8TxHkDij4b", amount: 160000 },
];

pub const ALLOCATIONS_12_36: [Allocation; 2] = [
	Allocation { address: "1xhckCAgNsFFTCeSN1VX7xMG6zrpL2dQqJDKqjt1mPmEAut", amount: 500000 },
	Allocation { address: "6gPTQUcQBM9xdmo3tfXDLuUNoZbERAdGHNhW3JeFDHuLfUBY", amount: 500000 },
];
