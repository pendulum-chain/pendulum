use frame_support::error::LookupError;
use sp_runtime::traits::StaticLookup;

pub struct BalanceConversion;

impl StaticLookup for BalanceConversion {
    type Source = u128;
    type Target = i64;

    fn lookup(pendulum_balance: Self::Source) -> Result<Self::Target, LookupError> {
        let stroops128: u128 = pendulum_balance / 100000;

        if stroops128 > i64::MAX as u128 {
            Err(LookupError)
        } else {
            Ok(stroops128 as i64)
        }
    }

    fn unlookup(stellar_stroops: Self::Target) -> Self::Source {
        (stellar_stroops * 100000) as u128
    }
}
