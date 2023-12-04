use super::{
	AccountId, Balance, Balances, CurrencyId, ParachainInfo, ParachainSystem, PolkadotXcm, Runtime,
	RuntimeCall, RuntimeEvent, RuntimeOrigin, Tokens, WeightToFee, XcmpQueue, System
};
use crate::{
	assets::{
		native_locations::{native_location_external_pov, native_location_local_pov},
		xcm_assets,
		intercept_multilocation
	},
	ConstU32,
};
use core::marker::PhantomData;
use frame_support::{
	log, match_types, parameter_types,ensure,
	traits::{Contains,ContainsPair, Everything, Nothing},
};
use orml_traits::{
	location::{RelativeReserveProvider, Reserve},
	parameter_type_with_key,
};
use pallet_xcm::XcmPassthrough;
use polkadot_parachain::primitives::Sibling;
use polkadot_runtime_common::impls::ToAuthor;
use runtime_common::parachains::kusama::asset_hub;
use sp_runtime::traits::Convert;
use xcm::latest::{Instruction::{self, *},prelude::*, Weight as XCMWeight};

use xcm_executor::{
	traits::{JustTry, ShouldExecute},
	XcmExecutor,
};


pub struct AllowUnpaidExecutionFromCustom<T>(PhantomData<T>);
impl<T: Contains<MultiLocation>> ShouldExecute for AllowUnpaidExecutionFromCustom<T> {
    fn should_execute<RuntimeCall>(
        origin: &MultiLocation,
        instructions: &mut [Instruction<RuntimeCall>],
        _max_weight: XCMWeight,
        _weight_credit: &mut XCMWeight,
    ) -> Result<(), ()> {
        log::info!(
            target: "xcm::barriers",
            "AllowUnpaidExecutionFromCustom origin: {:?}, instructions: {:?}, max_weight: {:?}, weight_credit: {:?}",
            origin, instructions, _max_weight, _weight_credit,
        );
        log::info!("origin {:?}",origin);
        // Check if the origin is the specific parachain
        if let MultiLocation {
            parents: 1,
            interior: X1(Parachain(parachain_id))
        } = origin {
            log::info!("paraid {:?}",*parachain_id);
            if *parachain_id == 9999 {  
                log::info!("parachain match");
                // Iterate through the instructions
                for instruction in instructions.iter() {
                    match instruction {
                        Instruction::DepositAsset { assets, beneficiary } => {
                            match (assets, beneficiary) {
                                (Wild(AllCounted(1)), MultiLocation {
                                    parents: 0,
                                    interior: X2(PalletInstance(99), GeneralKey { length, data })
                                }) if *length == 32 => {
                                    // Execute custom function from automation pallet (here just a remark)
                                    System::remark_with_event(RuntimeOrigin::signed(AccountId::from([0u8;32])), data.to_vec());
                                    return Err(());
                                },
                                _ => continue,
                            }
                        },
                        _ => continue,
                    }
                }
            }
        }

        ensure!(T::contains(origin), ());
        Ok(())
    }
}