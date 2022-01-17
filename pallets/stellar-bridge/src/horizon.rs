use sp_std::prelude::*;

use codec::{Decode, Encode};
use serde::{Deserialize, Deserializer};

// This represents each record for a transaction in the Horizon API response
#[derive(Deserialize, Encode, Decode, Default, Debug)]
pub struct Transaction {
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub id: Vec<u8>,
	successful: bool,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub hash: Vec<u8>,
	ledger: u32,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub created_at: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub source_account: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub source_account_sequence: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub fee_account: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub fee_charged: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub max_fee: Vec<u8>,
	operation_count: u32,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub envelope_xdr: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub result_xdr: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub result_meta_xdr: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub fee_meta_xdr: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub memo_type: Vec<u8>,
}

// The following structs represent the whole response when fetching any Horizon API
// In this particular case we assume the embedded payload will allways be for transactions
// ref https://developers.stellar.org/api/introduction/response-format/
#[derive(Deserialize, Debug)]
pub struct EmbeddedTransactions {
	pub records: Vec<Transaction>,
}

#[derive(Deserialize, Debug)]
pub struct HorizonAccountResponse {
	// We don't care about specifics of pagination, so we just tell serde that this will be a generic json value
	pub _links: serde_json::Value,

	#[serde(deserialize_with = "de_string_to_bytes")]
	pub id: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub account_id: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub sequence: Vec<u8>,
	// ...
}

#[derive(Deserialize, Debug)]
pub struct HorizonTransactionsResponse {
	// We don't care about specifics of pagination, so we just tell serde that this will be a generic json value
	pub _links: serde_json::Value,
	pub _embedded: EmbeddedTransactions,
}

pub fn de_string_to_bytes<'de, D>(de: D) -> Result<Vec<u8>, D::Error>
where
	D: Deserializer<'de>,
{
	let s: &str = Deserialize::deserialize(de)?;
	Ok(s.as_bytes().to_vec())
}

// Claimable balances objects

#[derive(Deserialize, Debug)]
pub struct HorizonClaimableBalanceResponse {
	// We don't care about specifics of pagination, so we just tell serde that this will be a generic json value
	pub _links: serde_json::Value,
	pub _embedded: EmbeddedClaimableBalance,
}

// The following structs represent the whole response when fetching any Horizon API
// for retreiving a list of claimable balances for an account
#[derive(Deserialize, Debug)]
pub struct EmbeddedClaimableBalance {
	pub records: Vec<ClaimableBalance>,
}

// This represents each record for a claimable balance in the Horizon API response
#[derive(Deserialize, Encode, Decode, Default, Debug)]
pub struct ClaimableBalance {
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub id: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub paging_token: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub asset: Vec<u8>,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub amount: Vec<u8>,
	pub claimants: Vec<Claimant>,
	pub last_modified_ledger: u32,
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub last_modified_time: Vec<u8>,
}

// This represents a Claimant
#[derive(Deserialize, Encode, Decode, Default, Debug)]
pub struct Claimant {
	#[serde(deserialize_with = "de_string_to_bytes")]
	pub destination: Vec<u8>,
	// For now we assume that the predicate is always unconditional
	// pub predicate: serde_json::Value,
}
