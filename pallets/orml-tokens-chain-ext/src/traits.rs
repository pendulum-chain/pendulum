// Copyright (c) 2012-2022 Supercolony
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the"Software"),
// to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
// LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
// WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

#[cfg(feature = "ink")]
use obce::ink_lang::prelude::vec::Vec;
#[cfg(feature = "substrate")]
use obce::substrate::sp_std::vec::Vec;
#[cfg(feature = "substrate")]
use obce::substrate::{
    frame_support::traits::PalletInfoAccess,
    CriticalError,
    SupportCriticalError,
};
#[cfg(feature = "substrate")]
use pallet_assets::Error as AssetError;

/// The origin of the call. The smart contract can execute methods on behalf of the `caller` or itself.
#[derive(Debug, Copy, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
#[cfg_attr(all(feature = "ink", feature = "std"), derive(ink::storage::traits::StorageLayout))]
pub enum Origin {
    Caller,
    Address,
}

impl Default for Origin {
    fn default() -> Self {
        Self::Address
    }
}

/// The trait describes types used in the chain extension definition. Substrate and ink! side can
/// have its types, so the trait is agnostic.
pub trait Environment {
    type AccountId;
    type AssetId;
    type Balance;
}

// TODO: Add comments
#[obce::definition(id = "pallet-assets-chain-extension@v0.1")]
pub trait PalletAssets<T: Environment> {
    fn create(&mut self, id: T::AssetId, admin: T::AccountId, min_balance: T::Balance) -> Result<(), Error<T>>;

    fn mint(&mut self, id: T::AssetId, who: T::AccountId, amount: T::Balance) -> Result<(), Error<T>>;

    fn burn(&mut self, id: T::AssetId, who: T::AccountId, amount: T::Balance) -> Result<(), Error<T>>;

    fn balance_of(&self, id: T::AssetId, owner: T::AccountId) -> T::Balance;

    fn total_supply(&self, id: T::AssetId) -> T::Balance;

    fn allowance(&self, id: T::AssetId, owner: T::AccountId, spender: T::AccountId) -> T::Balance;

    fn approve_transfer(
        &mut self,
        origin: Origin,
        id: T::AssetId,
        target: T::AccountId,
        amount: T::Balance,
    ) -> Result<(), Error<T>>;

    fn cancel_approval(&mut self, origin: Origin, id: T::AssetId, target: T::AccountId) -> Result<(), Error<T>>;

    fn transfer(
        &mut self,
        origin: Origin,
        id: T::AssetId,
        target: T::AccountId,
        amount: T::Balance,
    ) -> Result<(), Error<T>>;

    fn transfer_approved(
        &mut self,
        origin: Origin,
        id: T::AssetId,
        owner: T::AccountId,
        target: T::AccountId,
        amount: T::Balance,
    ) -> Result<(), Error<T>>;

    // Metadata section

    fn set_metadata(&mut self, id: T::AssetId, name: Vec<u8>, symbol: Vec<u8>, decimals: u8) -> Result<(), Error<T>>;

    fn metadata_name(&self, id: T::AssetId) -> Vec<u8>;

    fn metadata_symbol(&self, id: T::AssetId) -> Vec<u8>;

    fn metadata_decimals(&self, id: T::AssetId) -> u8;
}

/// The common errors that can be emitted by the `pallet-asset`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum Error<T> {
    // Errors of chain extension
    /// Only the admin can execute methods on behalf of the `caller`.
    ContractIsNotAdmin,

    // Asset pallet errors
    /// Account balance must be greater than or equal to the transfer amount.
    BalanceLow,
    /// The account to alter does not exist.
    NoAccount,
    /// The signing account has no permission to do the operation.
    NoPermission,
    /// The given asset ID is unknown.
    Unknown,
    /// The origin account is frozen.
    Frozen,
    /// The asset ID is already taken.
    InUse,
    /// Invalid witness data given.
    BadWitness,
    /// Minimum balance should be non-zero.
    MinBalanceZero,
    /// Unable to increment the consumer reference counters on the account. Either no provider
    /// reference exists to allow a non-zero balance of a non-self-sufficient asset, or the
    /// maximum number of consumers has been reached.
    NoProvider,
    /// Invalid metadata given.
    BadMetadata,
    /// No approval exists that would allow the transfer.
    Unapproved,
    /// The source account would not survive the transfer and it needs to stay alive.
    WouldDie,
    /// The asset-account already exists.
    AlreadyExists,
    /// The asset-account doesn't have an associated deposit.
    NoDeposit,
    /// The operation would result in funds being burned.
    WouldBurn,
    /// Unknown internal asset pallet error.
    AssetPalletInternal,

    // Substrate errors
    #[cfg(feature = "substrate")]
    /// Critical errors which stop the execution of the chain extension on the substrate level.
    Critical(CriticalError),
    #[doc(hidden)]
    #[codec(skip)]
    /// It is a dummy variant to support unused generics.
    __Ignore(core::marker::PhantomData<T>),
}

#[cfg(feature = "substrate")]
impl<T: pallet_assets::Config> From<CriticalError> for Error<T> {
    fn from(dispatch: CriticalError) -> Self {
        let asset_module = <pallet_assets::Pallet<T> as PalletInfoAccess>::index() as u8;

        // If error from the `pallet_assets` module, map it into ink! error
        if let CriticalError::Module(module) = dispatch {
            if module.index == asset_module {
                let mut input = module.error.as_slice();
                if let Ok(asset_error) = <AssetError<T> as scale::Decode>::decode(&mut input) {
                    return asset_error.into()
                }
            }
        }

        Error::Critical(dispatch)
    }
}

#[cfg(feature = "substrate")]
impl<T> From<AssetError<T>> for Error<T> {
    fn from(asset: AssetError<T>) -> Self {
        match asset {
            AssetError::<T>::BalanceLow => Error::<T>::BalanceLow,
            AssetError::<T>::NoAccount => Error::<T>::NoAccount,
            AssetError::<T>::NoPermission => Error::<T>::NoPermission,
            AssetError::<T>::Unknown => Error::<T>::Unknown,
            AssetError::<T>::Frozen => Error::<T>::Frozen,
            AssetError::<T>::InUse => Error::<T>::InUse,
            AssetError::<T>::BadWitness => Error::<T>::BadWitness,
            AssetError::<T>::MinBalanceZero => Error::<T>::MinBalanceZero,
            AssetError::<T>::NoProvider => Error::<T>::NoProvider,
            AssetError::<T>::BadMetadata => Error::<T>::BadMetadata,
            AssetError::<T>::Unapproved => Error::<T>::Unapproved,
            AssetError::<T>::WouldDie => Error::<T>::WouldDie,
            AssetError::<T>::AlreadyExists => Error::<T>::AlreadyExists,
            AssetError::<T>::NoDeposit => Error::<T>::NoDeposit,
            AssetError::<T>::WouldBurn => Error::<T>::WouldBurn,
            _ => Error::<T>::AssetPalletInternal,
        }
    }
}

#[cfg(feature = "substrate")]
impl<T> SupportCriticalError for Error<T> {
    fn try_to_critical(self) -> Result<CriticalError, Self> {
        match self {
            Error::<T>::Critical(error) => Ok(error),
            _ => Err(self),
        }
    }
}
