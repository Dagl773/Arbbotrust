//! Anvil-fork-style integration test for the pre-flight simulator.
//!
//! Marked `#[ignore]` so the default `cargo test` skips it; runs via
//! `cargo test --test pair_arb_fork -- --ignored` when `ARBITRUM_HTTP_URL`
//! is set in the environment.
//!
//! What this asserts: against the live (or anvil-forked) Arbitrum chain at
//! `latest`, querying QuoterV2 for 1 WETH → USDC.e through the canonical
//! 0.05% pool returns a non-zero amountOut.
//!
//! It does NOT spin Anvil itself — running anvil + forking a deep state at
//! a specific block adds setup complexity that isn't a v1 priority. The HTTP
//! provider against `latest` is functionally equivalent for this assertion.

use alloy::primitives::{U256, address};

#[tokio::test]
#[ignore = "requires ARBITRUM_HTTP_URL"]
async fn quoter_v3_returns_nonzero_for_weth_usdce() {
    let http_url = match std::env::var("ARBITRUM_HTTP_URL") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("skipping: ARBITRUM_HTTP_URL not set");
            return;
        }
    };

    // Verified Arbitrum One addresses (CLAUDE.md §11).
    let weth = address!("82aF49447D8a07e3bd95BD0d56f35241523fBab1");
    let usdce = address!("FF970A61A04b1cA14834A43f5dE4533eBDDB5CC8");
    let quoter = address!("61fFE014bA17989E743c5F6cB21bF9697530B21e");

    let sim = arbi_bot::simulator::revm_fork::ForkSim::new(
        arbi_bot::simulator::revm_fork::ForkSimConfig::new(http_url),
        quoter,
    )
    .await
    .expect("connect ForkSim");

    let amount_in = U256::from(10u128.pow(18)); // 1 WETH
    let out = sim
        .quote_v3(weth, usdce, 500, amount_in)
        .await
        .expect("quoter call should succeed");

    assert!(out > U256::ZERO, "WETH→USDC.e quote returned zero");
    // USDC.e has 6 decimals. 1 WETH should be worth more than $100 in any
    // realistic market state (sanity floor; not a price assertion).
    assert!(
        out > U256::from(100u128 * 10u128.pow(6)),
        "WETH→USDC.e quote suspiciously low: {out}"
    );
}
