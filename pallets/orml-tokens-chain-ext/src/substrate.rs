// Copyright (c) 2012-2022 727
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

use super::traits::{
    Environment as AssetsEnvironment,
    PalletAssets,
};
use crate::traits::{
    Error,
    Origin,
};
use obce::substrate::{
    frame_support::traits::fungibles::{
        approvals,
        Inspect,
        InspectMetadata,
    },
    frame_system::{
        Config as SysConfig,
        RawOrigin,
    },
    pallet_contracts::{
        chain_extension::Ext,
        Config as ContractConfig,
    },
    sp_runtime::traits::StaticLookup,
    sp_std::vec::Vec,
    ExtensionContext,
};
use pallet_assets::Config as AssetConfig;

#[derive(Default)]
pub struct AssetsExtension;

impl<T: SysConfig + AssetConfig + ContractConfig> AssetsEnvironment for T {
    type AccountId = <T as SysConfig>::AccountId;
    type AssetId = <T as AssetConfig>::AssetId;
    type Balance = <T as AssetConfig>::Balance;
}

#[obce::implementation]
impl<'a, 'b, E, T> PalletAssets<T> for ExtensionContext<'a, 'b, E, T, AssetsExtension>
where
    T: SysConfig + AssetConfig + ContractConfig,
    <<T as SysConfig>::Lookup as StaticLookup>::Source: From<<T as SysConfig>::AccountId>,
    E: Ext<T = T>,
{
    fn create(&mut self, id: T::AssetId, admin: T::AccountId, min_balance: T::Balance) -> Result<(), Error<T>> {
        // The contract should have money for the deposit
        Ok(pallet_assets::Pallet::<T>::create(
            self.origin(),
            id.into(),
            admin.into(),
            min_balance,
        )?)
    }

    fn mint(&mut self, id: T::AssetId, who: T::AccountId, amount: T::Balance) -> Result<(), Error<T>> {
        // Only origin with `issuer` right can do mint
        Ok(pallet_assets::Pallet::<T>::mint(self.origin(), id.into(), who.into(), amount)?)
    }

    fn burn(&mut self, id: T::AssetId, who: T::AccountId, amount: T::Balance) -> Result<(), Error<T>> {
        // Only origin with `admin` right can do burn
        Ok(pallet_assets::Pallet::<T>::burn(self.origin(), id.into(), who.into(), amount)?)
    }

    fn balance_of(&self, id: T::AssetId, owner: T::AccountId) -> T::Balance {
        <pallet_assets::Pallet<T> as Inspect<T::AccountId>>::balance(id.into(), &owner)
    }

    fn total_supply(&self, id: T::AssetId) -> T::Balance {
        <pallet_assets::Pallet<T> as Inspect<T::AccountId>>::total_issuance(id)
    }

    fn allowance(&self, id: T::AssetId, owner: T::AccountId, spender: T::AccountId) -> T::Balance {
        <pallet_assets::Pallet<T> as approvals::Inspect<T::AccountId>>::allowance(id.into(), &owner, &spender)
    }

    fn approve_transfer(
        &mut self,
        origin: Origin,
        id: T::AssetId,
        target: T::AccountId,
        amount: T::Balance,
    ) -> Result<(), Error<T>> {
        Ok(pallet_assets::Pallet::<T>::approve_transfer(
            self.select_origin(origin)?,
            id.into(),
            target.into(),
            amount,
        )?)
    }

    fn cancel_approval(&mut self, origin: Origin, id: T::AssetId, target: T::AccountId) -> Result<(), Error<T>> {
        Ok(pallet_assets::Pallet::<T>::cancel_approval(
            self.select_origin(origin)?,
            id.into(),
            target.into(),
        )?)
    }

    fn transfer(
        &mut self,
        origin: Origin,
        id: T::AssetId,
        target: T::AccountId,
        amount: T::Balance,
    ) -> Result<(), Error<T>> {
        Ok(pallet_assets::Pallet::<T>::transfer(
            self.select_origin(origin)?,
            id.into(),
            target.into(),
            amount,
        )?)
    }

    fn transfer_approved(
        &mut self,
        origin: Origin,
        id: T::AssetId,
        owner: T::AccountId,
        target: T::AccountId,
        amount: T::Balance,
    ) -> Result<(), Error<T>> {
        Ok(pallet_assets::Pallet::<T>::transfer_approved(
            self.select_origin(origin)?,
            id.into(),
            owner.into(),
            target.into(),
            amount,
        )?)
    }

    fn set_metadata(&mut self, id: T::AssetId, name: Vec<u8>, symbol: Vec<u8>, decimals: u8) -> Result<(), Error<T>> {
        Ok(pallet_assets::Pallet::<T>::set_metadata(
            self.origin(),
            id.into(),
            name,
            symbol,
            decimals,
        )?)
    }

    fn metadata_name(&self, id: T::AssetId) -> Vec<u8> {
        <pallet_assets::Pallet<T> as InspectMetadata<T::AccountId>>::name(&id)
    }

    fn metadata_symbol(&self, id: T::AssetId) -> Vec<u8> {
        <pallet_assets::Pallet<T> as InspectMetadata<T::AccountId>>::symbol(&id)
    }

    fn metadata_decimals(&self, id: T::AssetId) -> u8 {
        <pallet_assets::Pallet<T> as InspectMetadata<T::AccountId>>::decimals(&id)
    }
}

/// Trait with additional helpers functions.
pub trait Internal<T: AssetsEnvironment + SysConfig> {
    /// Returns the `AccountId` of the contract as signed origin.
    fn origin(&mut self) -> T::RuntimeOrigin;

    /// Returns the `AccountId` of the contract as signed origin based on the permission.
    fn select_origin(&mut self, origin: Origin) -> Result<T::RuntimeOrigin, Error<T>>;
}

impl<'a, 'b, E, T> Internal<T> for ExtensionContext<'a, 'b, E, T, AssetsExtension>
where
    T: SysConfig + AssetConfig + ContractConfig,
    <<T as SysConfig>::Lookup as StaticLookup>::Source: From<<T as SysConfig>::AccountId>,
    E: Ext<T = T>,
{
    fn origin(&mut self) -> T::RuntimeOrigin {
        RawOrigin::Signed(self.env.ext().address().clone()).into()
    }

    fn select_origin(&mut self, origin: Origin) -> Result<T::RuntimeOrigin, Error<T>> {
        let origin = RawOrigin::Signed(match origin {
            Origin::Caller => {
                // TODO: Add check that the contract is admin. Right now `asset-pallet` doesn't have getter for admin.
                // TODO: Return `Error::<T>::ContractIsNotAdmin`
                // let a = pallet_assets::Pallet::<T>::asset();
                self.env.ext().caller().clone()
            }
            Origin::Address => self.env.ext().address().clone(),
        });

        Ok(origin.into())
    }
}
