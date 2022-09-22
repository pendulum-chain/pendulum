use super::{
	AccountId, Balances, Call, Event, Origin, ParachainInfo, ParachainSystem, PolkadotXcm, Runtime,
	WeightToFee,
};
use core::marker::PhantomData;
use frame_support::{
	log, match_types, parameter_types,
	traits::{Everything, Nothing},
	weights::Weight,
};
use pallet_xcm::XcmPassthrough;
use polkadot_parachain::primitives::Sibling;
use polkadot_runtime_common::impls::ToAuthor;
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, AllowTopLevelPaidExecutionFrom, AllowUnpaidExecutionFrom, CurrencyAdapter,
	EnsureXcmOrigin, FixedWeightBounds, IsConcrete, LocationInverter, NativeAsset, ParentIsPreset,
	RelayChainAsNative, SiblingParachainAsNative, SiblingParachainConvertsVia,
	SignedAccountId32AsNative, SignedToAccountId32, SovereignSignedViaLocation, TakeWeightCredit,
	UsingComponents,
};
use xcm_executor::{traits::ShouldExecute, XcmExecutor};
use cumulus_primitives_core::ParaId;
use xcm::VersionedXcm;
use polkadot_parachain::primitives::XcmpMessageHandler;
use polkadot_parachain::primitives::DmpMessageHandler;
use sp_std::vec::Vec;
use runtime_common::BlockNumber;
#[frame_support::pallet]
pub mod mock_msg_queue {
	use super::*;
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		type XcmExecutor: ExecuteXcm<Self::Call>;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn parachain_id)]
	pub(super) type ParachainId<T: Config> = StorageValue<_, ParaId, ValueQuery>;


	impl<T: Config> Get<ParaId> for Pallet<T> {
		fn get() -> ParaId {
			Self::parachain_id()
		}
	}

	pub type MessageId = [u8; 32];

	#[pallet::event]
	#[pallet::generate_deposit(pub fn deposit_event)]
	pub enum Event<T: Config> {
		// XCMP
		/// Some XCM was executed OK.
		Success(Option<T::Hash>),
		/// Some XCM failed.
		Fail(Option<T::Hash>, XcmError),
		/// Bad XCM version used.
		BadVersion(Option<T::Hash>),
		/// Bad XCM format used.
		BadFormat(Option<T::Hash>),

		// DMP
		/// Downward message is invalid XCM.
		InvalidFormat(MessageId),
		/// Downward message is unsupported version of XCM.
		UnsupportedVersion(MessageId),
		/// Downward message executed with the given outcome.
		ExecutedDownward(MessageId, Outcome),
	}

	impl<T: Config> Pallet<T> {
		pub fn set_para_id(para_id: ParaId) {
			ParachainId::<T>::put(para_id);
		}

		fn handle_xcmp_message(
			sender: ParaId,
			_sent_at: BlockNumber,
			xcm: VersionedXcm<T::Call>,
			max_weight: Weight,
		) -> Result<Weight, XcmError> {
			println!("xcmp_message");
			// let hash = Encode::using_encoded(&xcm, T::Hashing::hash);
			// let (result, event) = match Xcm::<T::RuntimeCall>::try_from(xcm) {
			// 	Ok(xcm) => {
			// 		let location = (1, Parachain(sender.into()));
			// 		match T::XcmExecutor::execute_xcm(location, xcm, max_weight.ref_time()) {
			// 			Outcome::Error(e) => (Err(e.clone()), Event::Fail(Some(hash), e)),
			// 			Outcome::Complete(w) =>
			// 				(Ok(Weight::from_ref_time(w)), Event::Success(Some(hash))),
			// 			// As far as the caller is concerned, this was dispatched without error, so
			// 			// we just report the weight used.
			// 			Outcome::Incomplete(w, e) =>
			// 				(Ok(Weight::from_ref_time(w)), Event::Fail(Some(hash), e)),
			// 		}
			// 	},
			// 	Err(()) => (Err(XcmError::UnhandledXcmVersion), Event::BadVersion(Some(hash))),
			// };
			// Self::deposit_event(event);
			// result
			unimplemented!()
		}
	}
	impl<T> xcm::v2::SendXcm for Pallet<T>{
		fn send_xcm(destination: impl Into<MultiLocation>, message: Xcm<()>) -> SendResult{
			println!("unimplemented send_xcm");
			unimplemented!();
		}
	}

	impl<T: Config> XcmpMessageHandler for Pallet<T> {
		fn handle_xcmp_messages<'a, I: Iterator<Item = (ParaId, BlockNumber, &'a [u8])>>(
			iter: I,
			max_weight: Weight,
		) -> Weight {
			println!("xcmp_message");
			// for (sender, sent_at, data) in iter {
			// 	let mut data_ref = data;
			// 	let _ = XcmpMessageFormat::decode(&mut data_ref)
			// 		.expect("Simulator encodes with versioned xcm format; qed");

			// 	let mut remaining_fragments = &data_ref[..];
			// 	while !remaining_fragments.is_empty() {
			// 		if let Ok(xcm) =
			// 			VersionedXcm::<T::RuntimeCall>::decode(&mut remaining_fragments)
			// 		{
			// 			let _ = Self::handle_xcmp_message(sender, sent_at, xcm, max_weight);
			// 		} else {
			// 			debug_assert!(false, "Invalid incoming XCMP message data");
			// 		}
			// 	}
			// }
			// max_weight
			unimplemented!()
		}
	}

	impl<T: Config> DmpMessageHandler for Pallet<T> {
		fn handle_dmp_messages(
			iter: impl Iterator<Item = (BlockNumber, Vec<u8>)>,
			limit: Weight,
		) -> Weight {
			println!("xcmp_message");
			// for (_i, (_sent_at, data)) in iter.enumerate() {
			// 	let id = sp_io::hashing::blake2_256(&data[..]);
			// 	let maybe_msg = VersionedXcm::<T::RuntimeCall>::decode(&mut &data[..])
			// 		.map(Xcm::<T::RuntimeCall>::try_from);
			// 	match maybe_msg {
			// 		Err(_) => {
			// 			Self::deposit_event(Event::InvalidFormat(id));
			// 		},
			// 		Ok(Err(())) => {
			// 			Self::deposit_event(Event::UnsupportedVersion(id));
			// 		},
			// 		Ok(Ok(x)) => {
			// 			let outcome =
			// 				T::XcmExecutor::execute_xcm(Parent, x.clone(), limit.ref_time());
			// 			<ReceivedDmp<T>>::append(x);
			// 			Self::deposit_event(Event::ExecutedDownward(id, outcome));
			// 		},
			// 	}
			// }
			// limit
			unimplemented!()
		}
	}
}
