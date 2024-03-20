use crate::{Runtime, Tokens, AccountId};

// Define the global id's of our chain extensions
use pallet_contracts::chain_extension::RegisteredChainExtension;
pub use price_chain_extension::PriceChainExtension;
pub use token_chain_extension::TokensChainExtension;

impl RegisteredChainExtension<Runtime> for TokensChainExtension<Runtime, Tokens, AccountId> {
	const ID: u16 = 01;
}

impl RegisteredChainExtension<Runtime> for PriceChainExtension<Runtime> {
    const ID: u16 = 02;
}