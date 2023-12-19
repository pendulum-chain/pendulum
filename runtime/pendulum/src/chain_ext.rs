use crate::Tokens;
use codec::Encode;
use dia_oracle::{CoinInfo, DiaOracle};
use frame_support::{
	dispatch::Weight,
	pallet_prelude::Get,
	sp_tracing::{error, trace},
};
use orml_currencies::WeightInfo;
use orml_currencies_allowance_extension::{
	default_weights::WeightInfo as AllowanceWeightInfo, Config as AllowanceConfig,
};
use orml_traits::MultiCurrency;
use pallet_contracts::chain_extension::{
	ChainExtension, Environment, Ext, InitState, RetVal, SysConfig,
};
use runtime_common::{
	chain_ext,
	chain_ext::{
		Blockchain, ChainExtensionOutcome, ChainExtensionTokenError, Symbol, ToTrimmedVec,
	},
};
use sp_core::crypto::UncheckedFrom;
use sp_runtime::DispatchError;
use spacewalk_primitives::CurrencyId;

use super::AccountId;

pub(crate) type BalanceOfForChainExt<T> =
	<<T as orml_currencies::Config>::MultiCurrency as orml_traits::MultiCurrency<
		<T as frame_system::Config>::AccountId,
	>>::Balance;

// Enum that handles all supported function id options
#[derive(Debug)]
enum FuncId {
	// totalSupply(currency)
	TotalSupply,
	// balanceOf(currency, account)
	BalanceOf,
	// transfer(currency, recipient, amount)
	Transfer,
	// allowance(currency, owner, spender)
	Allowance,
	// approve(currency, spender, amount)
	Approve,
	// transfer_from(sender, currency, recipient, amount)
	TransferFrom,
	// get_coin_info(blockchain, symbol)
	GetCoinInfo,
}

impl TryFrom<u16> for FuncId {
	type Error = DispatchError;
	fn try_from(func_id: u16) -> Result<Self, Self::Error> {
		let id = match func_id {
			1101 => Self::TotalSupply,
			1102 => Self::BalanceOf,
			1103 => Self::Transfer,
			1104 => Self::Allowance,
			1105 => Self::Approve,
			1106 => Self::TransferFrom,
			1200 => Self::GetCoinInfo,
			_ => {
				error!("Called an unregistered `func_id`: {:}", func_id);
				return Err(DispatchError::Other("Unimplemented func_id"))
			},
		};
		Ok(id)
	}
}

#[derive(Default)]
pub struct Psp22Extension;

impl<T> ChainExtension<T> for Psp22Extension
where
	T: SysConfig
		+ orml_tokens::Config<CurrencyId = CurrencyId>
		+ pallet_contracts::Config
		+ orml_currencies::Config<MultiCurrency = Tokens, AccountId = AccountId>
		+ orml_currencies_allowance_extension::Config
		+ dia_oracle::Config,
	<T as SysConfig>::AccountId: UncheckedFrom<<T as SysConfig>::Hash> + AsRef<[u8]>,
{
	fn call<E: Ext>(&mut self, env: Environment<E, InitState>) -> Result<RetVal, DispatchError>
	where
		E: Ext<T = T>,
		<E::T as SysConfig>::AccountId: UncheckedFrom<<E::T as SysConfig>::Hash> + AsRef<[u8]>,
	{
		let func_id = FuncId::try_from(env.func_id())?;

		trace!("Calling function with ID {:?} from Psp22Extension", &func_id);

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
			FuncId::TotalSupply => total_supply(env, overhead_weight),
			FuncId::BalanceOf => balance_of(env, overhead_weight),
			FuncId::Transfer => transfer(env, overhead_weight),
			FuncId::Allowance => allowance(env, overhead_weight),
			FuncId::Approve => approve(env, overhead_weight),
			FuncId::TransferFrom => transfer_from(env, overhead_weight),
			FuncId::GetCoinInfo => get_coin_info(env, overhead_weight),
		};

		result
	}

	fn enabled() -> bool {
		true
	}
}

fn total_supply<E: Ext, T>(
	env: Environment<'_, '_, E, InitState>,
	overhead_weight: Weight,
) -> Result<RetVal, DispatchError>
where
	T: SysConfig
		+ orml_tokens::Config<CurrencyId = CurrencyId>
		+ pallet_contracts::Config
		+ orml_currencies::Config<MultiCurrency = Tokens, AccountId = AccountId>
		+ orml_currencies_allowance_extension::Config
		+ dia_oracle::Config,
	E: Ext<T = T>,
{
	let mut env = env.buf_in_buf_out();
	let base_weight = <T as frame_system::Config>::DbWeight::get().reads(1);
	env.charge_weight(base_weight.saturating_add(overhead_weight))?;
	let input = env.read(256)?;

	let currency_id: CurrencyId = match chain_ext::decode(input) {
		Ok(value) => value,
		Err(_) => return Ok(RetVal::Converging(ChainExtensionOutcome::DecodingError.as_u32())),
	};

	trace!("Calling totalSupply() for currency {:?}", currency_id);

	if !orml_currencies_allowance_extension::Pallet::<T>::is_allowed_currency(currency_id) {
		return Ok(RetVal::Converging(ChainExtensionTokenError::Unsupported.as_u32()))
	}

	let total_supply =
		<orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::total_issuance(currency_id);

	if let Err(_) = env.write(&total_supply.encode(), false, None) {
		return Ok(RetVal::Converging(ChainExtensionOutcome::WriteError.as_u32()))
	};
	return Ok(RetVal::Converging(ChainExtensionOutcome::Success.as_u32()))
}

fn balance_of<E: Ext, T>(
	env: Environment<'_, '_, E, InitState>,
	overhead_weight: Weight,
) -> Result<RetVal, DispatchError>
where
	T: SysConfig
		+ orml_tokens::Config<CurrencyId = CurrencyId>
		+ pallet_contracts::Config
		+ orml_currencies::Config<MultiCurrency = Tokens, AccountId = AccountId>
		+ orml_currencies_allowance_extension::Config
		+ dia_oracle::Config,
	E: Ext<T = T>,
{
	let mut env = env.buf_in_buf_out();
	let base_weight = <T as frame_system::Config>::DbWeight::get().reads(1);
	env.charge_weight(base_weight.saturating_add(overhead_weight))?;
	let input = env.read(256)?;
	let (currency_id, account_id): (CurrencyId, T::AccountId) = match chain_ext::decode(input) {
		Ok(value) => value,
		Err(_) => return Ok(RetVal::Converging(ChainExtensionOutcome::DecodingError.as_u32())),
	};

	trace!("Calling balanceOf() for currency {:?} and account {:?}", currency_id, account_id);

	if !orml_currencies_allowance_extension::Pallet::<T>::is_allowed_currency(currency_id) {
		return Ok(RetVal::Converging(ChainExtensionTokenError::Unsupported.as_u32()))
	}

	let balance = <orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::free_balance(
		currency_id,
		&account_id,
	);

	if let Err(_) = env.write(&balance.encode(), false, None) {
		return Ok(RetVal::Converging(ChainExtensionOutcome::WriteError.as_u32()))
	};
	return Ok(RetVal::Converging(ChainExtensionOutcome::Success.as_u32()))
}

fn transfer<E: Ext, T>(
	mut env: Environment<'_, '_, E, InitState>,
	overhead_weight: Weight,
) -> Result<RetVal, DispatchError>
where
	T: SysConfig
		+ orml_tokens::Config<CurrencyId = CurrencyId>
		+ pallet_contracts::Config
		+ orml_currencies::Config<MultiCurrency = Tokens, AccountId = AccountId>
		+ orml_currencies_allowance_extension::Config
		+ dia_oracle::Config,
	E: Ext<T = T>,
{
	let ext = env.ext();
	let caller = ext.caller().clone();

	let mut env = env.buf_in_buf_out();
	// Here we use weights for non native currency as worst case scenario, since we can't know whether it's native or not until we've already read from contract env.
	let base_weight = <T as orml_currencies::Config>::WeightInfo::transfer_non_native_currency();
	env.charge_weight(base_weight.saturating_add(overhead_weight))?;
	let input = env.read(256)?;
	let (currency_id, recipient, amount): (CurrencyId, T::AccountId, BalanceOfForChainExt<T>) =
		match chain_ext::decode(input) {
			Ok(value) => value,
			Err(_) => return Ok(RetVal::Converging(ChainExtensionOutcome::DecodingError.as_u32())),
		};

	trace!(
		"Calling transfer() sending {:?} {:?}, from {:?} to {:?}",
		amount,
		currency_id,
		caller,
		recipient
	);

	if !orml_currencies_allowance_extension::Pallet::<T>::is_allowed_currency(currency_id) {
		return Ok(RetVal::Converging(ChainExtensionTokenError::Unsupported.as_u32()))
	}

	<orml_currencies::Pallet<T> as MultiCurrency<T::AccountId>>::transfer(
		currency_id,
		&caller,
		&recipient,
		amount,
	)?;
	return Ok(RetVal::Converging(ChainExtensionOutcome::Success.as_u32()))
}

fn allowance<E: Ext, T>(
	env: Environment<'_, '_, E, InitState>,
	overhead_weight: Weight,
) -> Result<RetVal, DispatchError>
where
	T: SysConfig
		+ orml_tokens::Config<CurrencyId = CurrencyId>
		+ pallet_contracts::Config
		+ orml_currencies::Config<MultiCurrency = Tokens, AccountId = AccountId>
		+ orml_currencies_allowance_extension::Config
		+ dia_oracle::Config,
	E: Ext<T = T>,
{
	let mut env = env.buf_in_buf_out();
	let base_weight = <T as frame_system::Config>::DbWeight::get().reads(1);
	env.charge_weight(base_weight.saturating_add(overhead_weight))?;
	let input = env.read(256)?;
	let (currency_id, owner, spender): (CurrencyId, T::AccountId, T::AccountId) =
		match chain_ext::decode(input) {
			Ok(value) => value,
			Err(_) => return Ok(RetVal::Converging(ChainExtensionOutcome::DecodingError.as_u32())),
		};

	trace!(
		"Calling allowance() for currency {:?}, owner {:?} and spender {:?}",
		currency_id,
		owner,
		spender
	);

	if !orml_currencies_allowance_extension::Pallet::<T>::is_allowed_currency(currency_id) {
		return Ok(RetVal::Converging(ChainExtensionTokenError::Unsupported.as_u32()))
	}

	let allowance =
		orml_currencies_allowance_extension::Pallet::<T>::allowance(currency_id, &owner, &spender);

	if let Err(_) = env.write(&allowance.encode(), false, None) {
		return Ok(RetVal::Converging(ChainExtensionOutcome::WriteError.as_u32()))
	};
	return Ok(RetVal::Converging(ChainExtensionOutcome::Success.as_u32()))
}

fn approve<E: Ext, T>(
	mut env: Environment<'_, '_, E, InitState>,
	overhead_weight: Weight,
) -> Result<RetVal, DispatchError>
where
	T: SysConfig
		+ orml_tokens::Config<CurrencyId = CurrencyId>
		+ pallet_contracts::Config
		+ orml_currencies::Config<MultiCurrency = Tokens, AccountId = AccountId>
		+ orml_currencies_allowance_extension::Config
		+ dia_oracle::Config,
	E: Ext<T = T>,
{
	let ext = env.ext();
	let caller = ext.caller().clone();

	let mut env = env.buf_in_buf_out();
	let base_weight = <<T as AllowanceConfig>::WeightInfo as AllowanceWeightInfo>::approve();
	env.charge_weight(base_weight.saturating_add(overhead_weight))?;
	let input = env.read(256)?;
	let (currency_id, spender, amount): (CurrencyId, T::AccountId, BalanceOfForChainExt<T>) =
		match chain_ext::decode(input) {
			Ok(value) => value,
			Err(_) => return Ok(RetVal::Converging(ChainExtensionOutcome::DecodingError.as_u32())),
		};

	trace!(
		"Calling approve() allowing spender {:?} to transfer {:?} {:?} from {:?}",
		spender,
		amount,
		currency_id,
		caller
	);

	if !orml_currencies_allowance_extension::Pallet::<T>::is_allowed_currency(currency_id) {
		return Ok(RetVal::Converging(ChainExtensionTokenError::Unsupported.as_u32()))
	}

	orml_currencies_allowance_extension::Pallet::<T>::do_approve_transfer(
		currency_id,
		&caller,
		&spender,
		amount,
	)?;
	return Ok(RetVal::Converging(ChainExtensionOutcome::Success.as_u32()))
}

fn transfer_from<E: Ext, T>(
	mut env: Environment<'_, '_, E, InitState>,
	overhead_weight: Weight,
) -> Result<RetVal, DispatchError>
where
	T: SysConfig
		+ orml_tokens::Config<CurrencyId = CurrencyId>
		+ pallet_contracts::Config
		+ orml_currencies::Config<MultiCurrency = Tokens, AccountId = AccountId>
		+ orml_currencies_allowance_extension::Config
		+ dia_oracle::Config,
	E: Ext<T = T>,
{
	let ext = env.ext();
	let caller = ext.caller().clone();

	let mut env = env.buf_in_buf_out();
	let base_weight = <<T as AllowanceConfig>::WeightInfo as AllowanceWeightInfo>::transfer_from();
	env.charge_weight(base_weight.saturating_add(overhead_weight))?;
	let input = env.read(256)?;
	let (owner, currency_id, recipient, amount): (
		T::AccountId,
		CurrencyId,
		T::AccountId,
		BalanceOfForChainExt<T>,
	) = match chain_ext::decode(input) {
		Ok(value) => value,
		Err(_) => return Ok(RetVal::Converging(ChainExtensionOutcome::DecodingError.as_u32())),
	};

	trace!(
		"Calling transfer_from() for caller {:?}, sending {:?} {:?}, from {:?} to {:?}",
		caller,
		amount,
		currency_id,
		owner,
		recipient
	);

	if !orml_currencies_allowance_extension::Pallet::<T>::is_allowed_currency(currency_id) {
		return Ok(RetVal::Converging(ChainExtensionTokenError::Unsupported.as_u32()))
	}

	orml_currencies_allowance_extension::Pallet::<T>::do_transfer_approved(
		currency_id,
		&owner,
		&caller,
		&recipient,
		amount,
	)?;
	return Ok(RetVal::Converging(ChainExtensionOutcome::Success.as_u32()))
}

fn get_coin_info<E: Ext, T>(
	env: Environment<'_, '_, E, InitState>,
	overhead_weight: Weight,
) -> Result<RetVal, DispatchError>
where
	T: SysConfig
		+ orml_tokens::Config<CurrencyId = CurrencyId>
		+ pallet_contracts::Config
		+ orml_currencies::Config<MultiCurrency = Tokens, AccountId = AccountId>
		+ orml_currencies_allowance_extension::Config
		+ dia_oracle::Config,
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
