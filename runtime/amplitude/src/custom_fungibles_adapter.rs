use xcm_builder::{FungiblesAdapter, FungiblesMutateAdapter, FungiblesTransferAdapter, AssetChecking};
use frame_support::traits::{tokens::fungibles, Contains, Get};
use sp_std::{marker::PhantomData, prelude::*, result};
use xcm::latest::prelude::*;
use xcm_executor::traits::{Convert, Error as MatchError, MatchesFungibles, TransactAsset};


pub struct CustomFungiblesAdapter<
	Assets,
	Matcher,
	AccountIdConverter,
	AccountId,
	CheckAsset,
	CheckingAccount,
    Handle
>(PhantomData<(Assets, Matcher, AccountIdConverter, AccountId, CheckAsset, CheckingAccount, Handle)>);
impl<
		Assets: fungibles::Mutate<AccountId> + fungibles::Transfer<AccountId>,
		Matcher: MatchesFungibles<Assets::AssetId, Assets::Balance>,
		AccountIdConverter: Convert<MultiLocation, AccountId>,
		AccountId: Clone, // can't get away without it since Currency is generic over it.
		CheckAsset: AssetChecking<Assets::AssetId>,
		CheckingAccount: Get<AccountId>,
        Handle: HandleSpecialLocation,
	> TransactAsset
	for CustomFungiblesAdapter<Assets, Matcher, AccountIdConverter, AccountId, CheckAsset, CheckingAccount, Handle>
{
	fn can_check_in(origin: &MultiLocation, what: &MultiAsset, context: &XcmContext) -> XcmResult {
		FungiblesMutateAdapter::<
			Assets,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckAsset,
			CheckingAccount,
		>::can_check_in(origin, what, context)
	}

	fn check_in(origin: &MultiLocation, what: &MultiAsset, context: &XcmContext) {
		FungiblesMutateAdapter::<
			Assets,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckAsset,
			CheckingAccount,
		>::check_in(origin, what, context)
	}

	fn can_check_out(dest: &MultiLocation, what: &MultiAsset, context: &XcmContext) -> XcmResult {
		FungiblesMutateAdapter::<
			Assets,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckAsset,
			CheckingAccount,
		>::can_check_out(dest, what, context)
	}

	fn check_out(dest: &MultiLocation, what: &MultiAsset, context: &XcmContext) {
		FungiblesMutateAdapter::<
			Assets,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckAsset,
			CheckingAccount,
		>::check_out(dest, what, context)
	}

	fn deposit_asset(what: &MultiAsset, who: &MultiLocation, context: &XcmContext) -> XcmResult {
        if !Handle::handle(who, context){
			
			return FungiblesMutateAdapter::<
				Assets,
				Matcher,
				AccountIdConverter,
				AccountId,
				CheckAsset,
				CheckingAccount,
			>::deposit_asset(what, who, context)
		};
		return Ok(());
        
	}

	fn withdraw_asset(
		what: &MultiAsset,
		who: &MultiLocation,
		maybe_context: Option<&XcmContext>,
	) -> result::Result<xcm_executor::Assets, XcmError> {
		FungiblesMutateAdapter::<
			Assets,
			Matcher,
			AccountIdConverter,
			AccountId,
			CheckAsset,
			CheckingAccount,
		>::withdraw_asset(what, who, maybe_context)
	}

	fn internal_transfer_asset(
		what: &MultiAsset,
		from: &MultiLocation,
		to: &MultiLocation,
		context: &XcmContext,
	) -> result::Result<xcm_executor::Assets, XcmError> {
		FungiblesTransferAdapter::<Assets, Matcher, AccountIdConverter, AccountId>::internal_transfer_asset(
			what, from, to, context
		)
	}
}

pub trait HandleSpecialLocation{

    fn handle(what: &MultiLocation, context: &XcmContext ) -> bool;
}


