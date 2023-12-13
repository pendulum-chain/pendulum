use core::marker::PhantomData;
use frame_support::{ensure, log, traits::Contains};

use xcm::{latest::{prelude::*, Instruction, Weight as XCMWeight}};

use xcm_executor::traits::ShouldExecute;

use scale_info::prelude::boxed::Box;
use sp_std::vec::Vec;

pub trait ReserveAssetDepositedMatcher: Sync {
	fn matches(&self, multi_asset: &MultiAsset) -> Option<MultiAsset>;
}

pub trait DepositAssetMatcher {
	fn matches<'a>(
		&self,
		assets: &'a MultiAssetFilter,
		beneficiary: &'a MultiLocation,
	) -> Option<(u8, &'a [u8])>;
}
pub struct MatcherPair {
	reserve_deposited_asset_matcher: Box<dyn ReserveAssetDepositedMatcher>,
	deposit_asset_matcher: Box<dyn DepositAssetMatcher>,
}

impl MatcherPair {
	pub fn new(
		reserve_deposited_asset_matcher: Box<dyn ReserveAssetDepositedMatcher>,
		deposit_asset_matcher: Box<dyn DepositAssetMatcher>,
	) -> Self {
		MatcherPair { reserve_deposited_asset_matcher, deposit_asset_matcher }
	}

	fn matches_reserve_deposited(&self, multi_asset: &MultiAsset) -> Option<MultiAsset> {
		self.reserve_deposited_asset_matcher.matches(multi_asset)
	}

	fn matches_deposit_asset<'a>(
		&'a self,
		assets: &'a MultiAssetFilter,
		beneficiary: &'a MultiLocation,
	) -> Option<(u8, &'a [u8])> {
		self.deposit_asset_matcher.matches(assets, beneficiary)
	}
}

pub trait MatcherConfig {
	fn get_matcher_pairs() -> Vec<MatcherPair>;
	fn callback(length: u8, data: &[u8], amount: u128) -> Result<(), ()>;
	fn get_incoming_parachain_id() -> u32;
	fn extract_fee(location: MultiLocation, amount: u128)-> u128;
}

pub struct AllowUnpaidExecutionFromCustom<T, V> {
	_phantom: PhantomData<(T, V)>,
}
impl<T: Contains<MultiLocation>, V: MatcherConfig> ShouldExecute
	for AllowUnpaidExecutionFromCustom<T, V>
{
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
		log::info!("origin {:?}", origin);
		let incoming_parachain_id = V::get_incoming_parachain_id();

		ensure!(T::contains(origin), ());
		
		// Check if the origin is the specific parachain
		if let MultiLocation { parents: 1, interior: X1(Parachain(parachain_id)) } = origin {
			log::info!("paraid {:?}", *parachain_id);
			if *parachain_id == incoming_parachain_id {
				log::info!("parachain match");

				let matcher_pairs = V::get_matcher_pairs();
				// Iterate through the instructions, for
				// each match pair we allow
				for matcher_pair in matcher_pairs {
					let mut reserve_deposited_asset_matched: Option<MultiLocation> = None;
					let mut amount_to_process: u128 = 0;

					
					// Check for ReserveAssetDeposited instruction
					for instruction in instructions.iter() {
						if let Instruction::ReserveAssetDeposited(assets) = instruction {

							for asset in assets.clone().into_inner().iter() {
								if let Some(matched_asset) = matcher_pair.matches_reserve_deposited(asset) {
									
									// Check if the matched asset is fungible and extract the amount
									if let MultiAsset {
										id: Concrete(location), 
										fun: Fungibility::Fungible(amount_deposited),
									} = matched_asset
									{
										reserve_deposited_asset_matched = Some(location);
										amount_to_process = amount_deposited;
										break; // Break as the matched asset is found and amount is extracted
									}
								}
							}
						}
					}

					// If ReserveAssetDeposited matches, then check for DepositAsset with the same matcher pair
					// and execute the callback
					if let Some(location) = reserve_deposited_asset_matched {
						for instruction in instructions.iter() {
							if let Instruction::DepositAsset { assets, beneficiary } = instruction {
								if let Some((length, data)) =
									matcher_pair.matches_deposit_asset(assets, beneficiary)
								{

									let amount_to_deposit = V::extract_fee(location, amount_to_process);
									V::callback(length, data,amount_to_deposit);
									return Err(())
								}
							}
						}
					}
				}
			}
		}

		
		Ok(())
	}
}
