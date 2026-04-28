//! Closed-form optimal input for V2 ↔ V2 atomic arbitrage.
//!
//! Reference: Flashbots `simple-blind-arbitrage`
//! <https://github.com/flashbots/simple-blind-arbitrage>.
//!
//! Given two V2 pools quoting the same token pair, find the borrowed-asset
//! input X that maximises profit on the round trip `pool_a → token_other → pool_b`.
//! See [`optimal_v2_v2`] for the derivation; the closed form is exact, no
//! iteration required.

use alloy::primitives::U256;

use super::uniswap_v2::{BPS_DENOM, get_amount_out};

/// One side of a V2 arb. `reserve_in` is the side facing the *borrowed* token.
#[derive(Debug, Clone, Copy)]
pub struct V2Leg {
    pub reserve_in: U256,
    pub reserve_out: U256,
    pub fee_bps: u16,
}

/// Profitability outcome.
#[derive(Debug, Clone)]
pub struct OptimalArb {
    pub amount_in: U256,
    pub amount_out: U256,
    pub gross_profit: U256,
}

/// Compute the optimal flash-loan amount for a V2 → V2 atomic arb.
///
/// Derivation (with `f` denoting the post-fee multiplier, e.g. 0.997):
/// for path X →(A)→ Y →(B)→ Z, where pool A has reserves (A_in, A_out) and
/// pool B has reserves (B_in, B_out) (both oriented to the swap direction),
///
/// ```text
/// Z = (X * fa * fb * A_out * B_out)
///   / (A_in * B_in + X * fa * B_in + X * fa * fb * A_out)
/// ```
///
/// Maximising `Z - X` (constant-product calculus) yields
///
/// ```text
/// X* = (denom * sqrt(fa*fb * A_in*A_out*B_in*B_out) - A_in*B_in*denom^2)
///    / (fa*B_in*denom + fa*fb*A_out)
/// ```
///
/// where `denom = 10_000` and `fa, fb` are the fee numerators (e.g. `9970`).
/// Returns `None` if no positive-profit input exists. Caps `amount_in` at 50%
/// of `A_in` per CLAUDE.md §9.2.
pub fn optimal_v2_v2(pool_a: V2Leg, pool_b: V2Leg) -> Option<OptimalArb> {
    let fa = U256::from(BPS_DENOM - u32::from(pool_a.fee_bps));
    let fb = U256::from(BPS_DENOM - u32::from(pool_b.fee_bps));
    let denom = U256::from(BPS_DENOM);

    // Naming aligned with the derivation above.
    let a_in = pool_a.reserve_in;
    let a_out = pool_a.reserve_out;
    let b_in = pool_b.reserve_in;
    let b_out = pool_b.reserve_out;

    if a_in.is_zero() || a_out.is_zero() || b_in.is_zero() || b_out.is_zero() {
        return None;
    }

    // Inside the sqrt: fa * fb * A_in * A_out * B_in * B_out.
    let inside = fa
        .checked_mul(fb)?
        .checked_mul(a_in)?
        .checked_mul(a_out)?
        .checked_mul(b_in)?
        .checked_mul(b_out)?;
    let sqrt_inside = isqrt_u256(inside);

    // Numerator term to subtract: A_in * B_in * denom.
    // Profitable iff sqrt(fa*fb*A_in*A_out*B_in*B_out) > A_in*B_in*denom.
    let aib = a_in.checked_mul(b_in)?;
    let aib_denom = aib.checked_mul(denom)?;
    if sqrt_inside <= aib_denom {
        return None;
    }
    // numerator = denom * (sqrt_inside - A_in*B_in*denom)
    let numerator = denom.checked_mul(sqrt_inside.checked_sub(aib_denom)?)?;

    // denominator = fa*B_in*denom + fa*fb*A_out
    let denominator = fa
        .checked_mul(b_in)?
        .checked_mul(denom)?
        .checked_add(fa.checked_mul(fb)?.checked_mul(a_out)?)?;
    if denominator.is_zero() {
        return None;
    }
    let mut amount_in = numerator / denominator;
    if amount_in.is_zero() {
        return None;
    }

    // Cap at 50% of pool A's reserve_in (CLAUDE.md §9.2).
    let cap = a_in / U256::from(2u64);
    if amount_in > cap {
        amount_in = cap;
    }

    let mid = get_amount_out(amount_in, a_in, a_out, pool_a.fee_bps)?;
    let amount_out = get_amount_out(mid, b_in, b_out, pool_b.fee_bps)?;
    if amount_out <= amount_in {
        return None;
    }
    Some(OptimalArb {
        amount_in,
        amount_out,
        gross_profit: amount_out - amount_in,
    })
}

/// Integer square root of a `U256` via Newton's method. Returns the largest
/// `x` such that `x*x <= n`.
pub fn isqrt_u256(n: U256) -> U256 {
    if n.is_zero() {
        return U256::ZERO;
    }
    if n < U256::from(4u64) {
        return U256::from(1u64);
    }
    // Initial guess: 2^(ceil(bits(n)/2)).
    let bits = 256 - n.leading_zeros();
    let mut x = U256::from(1u64) << bits.div_ceil(2);
    loop {
        let nx = (x + n / x) >> 1;
        if nx >= x {
            return x;
        }
        x = nx;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn isqrt_known_values() {
        assert_eq!(isqrt_u256(U256::ZERO), U256::ZERO);
        assert_eq!(isqrt_u256(U256::from(1u64)), U256::from(1u64));
        assert_eq!(isqrt_u256(U256::from(15u64)), U256::from(3u64));
        assert_eq!(isqrt_u256(U256::from(16u64)), U256::from(4u64));
        assert_eq!(isqrt_u256(U256::from(10_000u64)), U256::from(100u64));
        // 2^128 → sqrt = 2^64
        assert_eq!(
            isqrt_u256(U256::from(1u128) << 128),
            U256::from(1u128) << 64
        );
    }

    proptest! {
        #[test]
        fn isqrt_property(n in 0u128..u128::MAX) {
            let r = isqrt_u256(U256::from(n));
            // r*r <= n < (r+1)*(r+1)
            let r_sq = r.checked_mul(r).unwrap();
            prop_assert!(r_sq <= U256::from(n));
            let r1 = r + U256::from(1u64);
            let r1_sq = r1.checked_mul(r1).unwrap();
            prop_assert!(r1_sq > U256::from(n));
        }
    }

    /// Iterative bisection reference for the optimal-input solver.
    fn brute_optimal(pool_a: V2Leg, pool_b: V2Leg, search_max: U256) -> Option<OptimalArb> {
        let try_input = |amount_in: U256| -> Option<U256> {
            let mid = get_amount_out(
                amount_in,
                pool_a.reserve_in,
                pool_a.reserve_out,
                pool_a.fee_bps,
            )?;
            let out = get_amount_out(mid, pool_b.reserve_in, pool_b.reserve_out, pool_b.fee_bps)?;
            if out > amount_in {
                Some(out - amount_in)
            } else {
                None
            }
        };

        // Coarse linear scan, fine enough for the unit fixtures we use.
        let steps = 200u64;
        let step = search_max / U256::from(steps);
        if step.is_zero() {
            return None;
        }
        let mut best: Option<(U256, U256)> = None;
        for i in 1..=steps {
            let amt = step * U256::from(i);
            if let Some(p) = try_input(amt)
                && best.is_none_or(|(_, bp)| p > bp)
            {
                best = Some((amt, p));
            }
        }
        let (amount_in, gross_profit) = best?;
        let mid = get_amount_out(
            amount_in,
            pool_a.reserve_in,
            pool_a.reserve_out,
            pool_a.fee_bps,
        )?;
        let amount_out =
            get_amount_out(mid, pool_b.reserve_in, pool_b.reserve_out, pool_b.fee_bps)?;
        Some(OptimalArb {
            amount_in,
            amount_out,
            gross_profit,
        })
    }

    #[test]
    fn optimal_finds_arb_when_one_pool_is_imbalanced() {
        // pool_a: 100 borrow / 100 other (1:1)
        // pool_b: 200 other / 100 borrow (2:1 → other is cheap on pool_b → arb buys
        // other on a, sells on b)
        let pool_a = V2Leg {
            reserve_in: U256::from(100_000_000u64),
            reserve_out: U256::from(100_000_000u64),
            fee_bps: 30,
        };
        let pool_b = V2Leg {
            reserve_in: U256::from(150_000_000u64),
            reserve_out: U256::from(200_000_000u64),
            fee_bps: 30,
        };
        let opt = optimal_v2_v2(pool_a, pool_b).expect("should find arb");
        let brute = brute_optimal(pool_a, pool_b, U256::from(50_000_000u64))
            .expect("brute should also find one");
        // Closed-form should be at least as profitable as the coarse scan.
        assert!(
            opt.gross_profit + U256::from(1000u64) >= brute.gross_profit,
            "closed_form={:?} brute={:?}",
            opt.gross_profit,
            brute.gross_profit
        );
    }

    #[test]
    fn optimal_rejects_when_no_arb() {
        let pool = V2Leg {
            reserve_in: U256::from(1_000_000u64),
            reserve_out: U256::from(1_000_000u64),
            fee_bps: 30,
        };
        // Identical pools: no arb possible.
        assert!(optimal_v2_v2(pool, pool).is_none());
    }
}
