use frame_support::error::LookupError;
use sp_core::ed25519;
use sp_runtime::traits::{IdentifyAccount, StaticLookup};
use sp_runtime::{AccountId32, MultiSigner};
use substrate_stellar_sdk as stellar;

pub struct AddressConversion;

impl StaticLookup for AddressConversion {
    type Source = AccountId32;
    type Target = stellar::PublicKey;

    fn lookup(key: Self::Source) -> Result<Self::Target, LookupError> {
        // We just assume (!) an Ed25519 key has been passed to us
        Ok(stellar::PublicKey::from_binary(key.into()) as stellar::PublicKey)
    }

    fn unlookup(stellar_addr: stellar::PublicKey) -> Self::Source {
        MultiSigner::Ed25519(ed25519::Public::from_raw(*stellar_addr.as_binary())).into_account()
    }
}

/// Error type for key decoding errors
#[derive(Debug)]
pub enum AddressConversionError {
    //     UnexpectedKeyType
}
