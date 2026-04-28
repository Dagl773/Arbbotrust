//! Uniswap V3 quote wrapper. Per CLAUDE.md §8 we don't do tick math
//! off-chain in v1 — we delegate to a REVM-backed `Quoter` call.
//!
//! Real wiring lands in Phase 5 alongside the simulator.

use alloy::primitives::{Address, U256};

#[derive(Debug, Clone, Copy)]
pub struct V3QuoteParams {
    pub pool: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub fee: u32,
    pub amount_in: U256,
}

/// Placeholder; the simulator will provide an `impl` of this.
pub trait V3Quoter {
    fn quote_exact_input_single(&mut self, params: V3QuoteParams) -> anyhow::Result<U256>;
}
