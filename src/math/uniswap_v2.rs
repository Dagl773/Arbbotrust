//! Closed-form Uniswap V2 swap math. Source:
//! <https://github.com/Uniswap/v2-core/blob/master/contracts/UniswapV2Pair.sol>
//!
//! `amount_out = (amount_in * (10000 - fee_bps) * reserve_out)`
//! `           / (reserve_in * 10000 + amount_in * (10000 - fee_bps))`

use alloy::primitives::U256;

/// Standard V2 / SushiSwap fee = 0.30% (CLAUDE.md §8).
pub const DEFAULT_FEE_BPS: u16 = 30;
/// Basis-point denominator. Sourced from the V2 router formula.
pub const BPS_DENOM: u32 = 10_000;

/// Swap-exact-input on a constant-product pool with `fee_bps`.
///
/// Returns `None` if any reserve is zero or `amount_in` is zero.
pub fn get_amount_out(
    amount_in: U256,
    reserve_in: U256,
    reserve_out: U256,
    fee_bps: u16,
) -> Option<U256> {
    if amount_in.is_zero() || reserve_in.is_zero() || reserve_out.is_zero() {
        return None;
    }
    let fee_factor = U256::from(BPS_DENOM - u32::from(fee_bps));
    let denom_factor = U256::from(BPS_DENOM);

    let amount_in_with_fee = amount_in.checked_mul(fee_factor)?;
    let numerator = amount_in_with_fee.checked_mul(reserve_out)?;
    let denominator = reserve_in
        .checked_mul(denom_factor)?
        .checked_add(amount_in_with_fee)?;
    if denominator.is_zero() {
        return None;
    }
    Some(numerator / denominator)
}

/// Inverse: how much `token_in` is required to receive exactly `amount_out`.
/// Returns `None` if `amount_out` ≥ `reserve_out` (impossible).
pub fn get_amount_in(
    amount_out: U256,
    reserve_in: U256,
    reserve_out: U256,
    fee_bps: u16,
) -> Option<U256> {
    if amount_out.is_zero()
        || reserve_in.is_zero()
        || reserve_out.is_zero()
        || amount_out >= reserve_out
    {
        return None;
    }
    let fee_factor = U256::from(BPS_DENOM - u32::from(fee_bps));
    let denom_factor = U256::from(BPS_DENOM);

    let numerator = reserve_in
        .checked_mul(amount_out)?
        .checked_mul(denom_factor)?;
    let denominator = reserve_out
        .checked_sub(amount_out)?
        .checked_mul(fee_factor)?;
    Some(numerator / denominator + U256::from(1u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-checked against the Solidity formula at fee_bps = 30:
    /// 1e18 in, 1000e18 / 2_000_000e6 reserves shape — we use round numbers
    /// for analytic verification.
    #[test]
    fn get_amount_out_basic() {
        // 1 token in, reserves 1000 / 1000 → out ≈ 0.996...
        let out = get_amount_out(
            U256::from(1_000_000u64),
            U256::from(1_000_000_000u64),
            U256::from(1_000_000_000u64),
            30,
        )
        .unwrap();
        // Hand-calc: amount_in_with_fee = 1e6 * 9970 = 9_970_000_000
        // numerator = 9_970_000_000 * 1_000_000_000 = 9.97e18
        // denominator = 1_000_000_000 * 10_000 + 9_970_000_000 = 10_009_970_000_000
        // out = 9.97e18 / 1.000997e13 ≈ 996_006
        assert_eq!(out, U256::from(996_006u64));
    }

    #[test]
    fn get_amount_out_zero_inputs_return_none() {
        let r = U256::from(1_000_000u64);
        assert!(get_amount_out(U256::ZERO, r, r, 30).is_none());
        assert!(get_amount_out(r, U256::ZERO, r, 30).is_none());
        assert!(get_amount_out(r, r, U256::ZERO, 30).is_none());
    }

    #[test]
    fn get_amount_in_inverse_of_get_amount_out() {
        let r_in = U256::from(5_000_000_000u64);
        let r_out = U256::from(2_500_000_000u64);
        let amount_in = U256::from(1_000_000u64);
        let out = get_amount_out(amount_in, r_in, r_out, 30).unwrap();
        let recomputed_in = get_amount_in(out, r_in, r_out, 30).unwrap();
        // Inverse should be at most amount_in + 1 (off-by-one rounding up).
        assert!(recomputed_in <= amount_in + U256::from(1u64));
        assert!(recomputed_in >= amount_in - U256::from(1u64));
    }

    /// Golden vector mirroring `getAmountOut(1e18, 100e18, 50_000e6, 30)` from
    /// the Uniswap V2 router. Mid-price is 1 WETH = 500 USDC; fees + slippage
    /// against a thin 100-WETH pool drag the realised quote down to ~493.58 USDC.
    ///
    ///   amount_in_with_fee = 1e18 * 9970               = 9.970e21
    ///   numerator          = 9.970e21 * 5e10           = 4.985e32
    ///   denominator        = 100e18*10_000 + 9.970e21  = 1.00997e24
    ///   out                = 4.985e32 / 1.00997e24     = 493_579_018 (6-dec)
    #[test]
    fn get_amount_out_golden_weth_usdc() {
        let one_eth = U256::from(10u128.pow(18));
        let r_weth = U256::from(100u128) * one_eth;
        let r_usdc = U256::from(50_000_000_000u128); // 50_000 * 1e6
        let out = get_amount_out(one_eth, r_weth, r_usdc, 30).unwrap();
        // Implementation truncates; observed is 493_579_017.
        assert_eq!(out, U256::from(493_579_017u128));
    }
}
