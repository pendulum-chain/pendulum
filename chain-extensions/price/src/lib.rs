#![cfg_attr(not(feature = "std"), no_std)]
#![allow(non_snake_case)]

use chain_extension_common::{Blockchain, ChainExtensionOutcome, Symbol, ToTrimmedVec};
use codec::Encode;
use dia_oracle::{CoinInfo as DiaCoinInfo, DiaOracle};
use frame_support::{
	dispatch::Weight,
	inherent::Vec,
	pallet_prelude::{Get, PhantomData},
	sp_tracing::{error, trace},
	DefaultNoBound,
};
use pallet_contracts::chain_extension::{
	ChainExtension, Environment, Ext, InitState, RetVal, SysConfig,
};
use sp_core::crypto::UncheckedFrom;
use sp_runtime::DispatchError;

// Enum that handles all supported function id options for this chain extension module
#[derive(Debug)]
enum FuncId {
	// get_coin_info(blockchain, symbol)
	GetCoinInfo,
}

impl TryFrom<u16> for FuncId {
	type Error = DispatchError;
	fn try_from(func_id: u16) -> Result<Self, Self::Error> {
		let id = match func_id {
			1200 => Self::GetCoinInfo,
			_ => {
				error!("Called an unregistered `func_id`: {:}", func_id);
				return Err(DispatchError::Other("Unimplemented func_id"))
			},
		};
		Ok(id)
	}
}

#[derive(DefaultNoBound)]
pub struct PriceChainExtension<T>(PhantomData<T>);

impl<T> ChainExtension<T> for PriceChainExtension<T>
where
	T: SysConfig + pallet_contracts::Config + dia_oracle::Config,
	<T as SysConfig>::AccountId: UncheckedFrom<<T as SysConfig>::Hash> + AsRef<[u8]>,
{
	fn call<E: Ext>(&mut self, env: Environment<E, InitState>) -> Result<RetVal, DispatchError>
	where
		E: Ext<T = T>,
		<E::T as SysConfig>::AccountId: UncheckedFrom<<E::T as SysConfig>::Hash> + AsRef<[u8]>,
	{
		let func_id = FuncId::try_from(env.func_id())?;
		trace!("Calling function with ID {:?} from PriceChainExtension", &func_id);
		// debug_message weight is a good approximation of the additional overhead of going
		// from contract layer to substrate layer.
		let overhead_weight = Weight::from_parts(
			<T as pallet_contracts::Config>::Schedule::get()
				.host_fn_weights
				.debug_message
				.ref_time(),
			0,
		);

		let result = match func_id {
			FuncId::GetCoinInfo => get_coin_info(env, overhead_weight),
		};
		result
	}

	fn enabled() -> bool {
		true
	}
}

fn get_coin_info<E: Ext, T>(
	env: Environment<'_, '_, E, InitState>,
	overhead_weight: Weight,
) -> Result<RetVal, DispatchError>
where
	T: SysConfig + pallet_contracts::Config + dia_oracle::Config,
	E: Ext<T = T>,
{
	let mut env = env.buf_in_buf_out();
	let base_weight = <T as frame_system::Config>::DbWeight::get().reads(1);
	env.charge_weight(base_weight.saturating_add(overhead_weight))?;
	let (blockchain, symbol): (Blockchain, Symbol) = env.read_as()?;

	let result = <dia_oracle::Pallet<T> as DiaOracle>::get_coin_info(
		blockchain.to_trimmed_vec(),
		symbol.to_trimmed_vec(),
	);

	trace!("Calling get_coin_info() for: {:?}:{:?}", blockchain, symbol);

	let result = match result {
		Ok(coin_info) => Result::<CoinInfo, ChainExtensionOutcome>::Ok(CoinInfo::from(coin_info)),
		Err(e) => return Ok(RetVal::Converging(ChainExtensionOutcome::from(e).as_u32())),
	};

	if let Err(_) = env.write(&result.encode(), false, None) {
		return Ok(RetVal::Converging(ChainExtensionOutcome::WriteError.as_u32()))
	};
	return Ok(RetVal::Converging(ChainExtensionOutcome::Success.as_u32()))
}

// this was in common, but we don't use it in amplitude's chain extension, is it needed?

/// CoinInfo is almost the same as Dia's CoinInfo, but with Encode, Decode, and TypeInfo which are necessary for contract to chain extension communication. Implements From<dia::CoinInfo> to make conversion.
#[derive(Debug, Clone, PartialEq, Eq, codec::Encode, codec::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub struct CoinInfo {
	pub symbol: Vec<u8>,
	pub name: Vec<u8>,
	pub blockchain: Vec<u8>,
	pub supply: u128,
	pub last_update_timestamp: u64,
	pub price: u128,
}

impl From<DiaCoinInfo> for CoinInfo {
	fn from(coin_info: DiaCoinInfo) -> Self {
		Self {
			symbol: coin_info.symbol,
			name: coin_info.name,
			blockchain: coin_info.blockchain,
			supply: coin_info.supply,
			last_update_timestamp: coin_info.last_update_timestamp,
			price: coin_info.price,
		}
	}
}
