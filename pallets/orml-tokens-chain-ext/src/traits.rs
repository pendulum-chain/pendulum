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
use orml_tokens::Error as AssetError;
use obce::substrate::frame_support::error::BadOrigin;

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

    // The chain doesn't necessarily have metadata for all on chain assets. 
    // However, the metadata can be hardcoded in the wrapper contract anyways.
    // //ERC20: name
    // fn metadata_name(&self, id: T::AssetId) -> Vec<u8>;

    // //ERC20: symbol
    // fn metadata_symbol(&self, id: T::AssetId) -> Vec<u8>;

    // //ERC20: decimals
    // fn metadata_decimals(&self, id: T::AssetId) -> u8;

    //ERC20: total_supply
    fn total_supply(&self, id: T::AssetId) -> T::Balance;

    //ERC20: balance_of
    fn balance_of(&self, id: T::AssetId, owner: T::AccountId) -> T::Balance;

    //ERC20: transfer
    fn transfer(
        &mut self,
        origin: Origin,
        id: T::AssetId,
        target: T::AccountId,
        amount: T::Balance,
    ) -> Result<(), Error<T>>;

    //ERC20: transferFrom
    fn transfer_approved(
        &mut self,
        origin: Origin,
        id: T::AssetId,
        owner: T::AccountId,
        target: T::AccountId,
        amount: T::Balance,
    ) -> Result<(), Error<T>>;

    //ERC20: approve
    fn approve_transfer(
        &mut self,
        origin: Origin,
        id: T::AssetId,
        target: T::AccountId,
        amount: T::Balance,
    ) -> Result<(), Error<T>>;

    //ERC20: allowance
    fn allowance(&self, id: T::AssetId, owner: T::AccountId, spender: T::AccountId) -> T::Balance;

    //price_feed takes a blockchain and token symbol and returns the coin info (which includes price and timestamp)
    fn price_feed(&self, blockchain: Vec<u8>, symbol: Vec<u8>) -> Result<dia_oracle::CoinInfo, Error<T>>;

}

/// The common errors that can be emitted by the `pallet-asset`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum Error<T> {
    // dia_oracle errors
    /// CoinInfo unavailable
    CoinInfoUnavailable,

    // Errors of chain extension
    /// Only the admin can execute methods on behalf of the `caller`.
    ContractIsNotAdmin,

    /// Bad Origin
    BadOrigin,

    // orml_tokens errors
    /// The balance is too low
    BalanceTooLow,
    /// Cannot convert Amount into Balance type
    AmountIntoBalanceFailed,
    /// Failed because liquidity restrictions due to locking
    LiquidityRestrictions,
    /// Failed because the maximum locks was exceeded
    MaxLocksExceeded,
    /// Transfer/payment would kill account
    KeepAlive,
    /// Value too low to create account due to existential deposit
    ExistentialDeposit,
    /// Beneficiary account must pre-exist
    DeadAccount,
    // Number of named reserves exceed `T::MaxReserves`
    TooManyReserves,
    /// Unknown internal orml tokens error.
    OrmlTokensInternal,

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
impl<T: orml_tokens::Config> From<CriticalError> for Error<T> {
    fn from(dispatch: CriticalError) -> Self {
        let asset_module = <orml_tokens::Pallet<T> as PalletInfoAccess>::index() as u8;

        // If error from the `orml_tokens` module, map it into ink! error
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
            AssetError::<T>::BalanceTooLow => Error::<T>::BalanceTooLow,
            AssetError::<T>::AmountIntoBalanceFailed => Error::<T>::AmountIntoBalanceFailed,
            AssetError::<T>::LiquidityRestrictions => Error::<T>::LiquidityRestrictions,
            AssetError::<T>::MaxLocksExceeded => Error::<T>::MaxLocksExceeded,
            AssetError::<T>::KeepAlive => Error::<T>::KeepAlive,
            AssetError::<T>::ExistentialDeposit => Error::<T>::ExistentialDeposit,
            AssetError::<T>::DeadAccount => Error::<T>::DeadAccount,
            AssetError::<T>::TooManyReserves => Error::<T>::TooManyReserves,
            _ => Error::<T>::OrmlTokensInternal,
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

#[cfg(feature = "substrate")]
impl<T> From<BadOrigin> for Error<T> {
    fn from(err: BadOrigin) -> Self {
        Error::<T>::BadOrigin
    }
}