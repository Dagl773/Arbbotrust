//! REVM forking simulator skeleton.
//!
//! The end goal is to host a `revm::CacheDB<AlloyDB>` against the Arbitrum
//! provider so we can:
//!   - call `IQuoterV2.quoteExactInputSingle` for V3 routes,
//!   - dry-run `executeArbitrage` calldata against the executor contract.
//!
//! Phase 5 fills in the actual REVM wiring; this module is the shape we want
//! the rest of the codebase to call into.

use alloy::primitives::{Address, Bytes, U256};
use anyhow::Result;

use crate::types::SimOutcome;

#[derive(Debug, Clone)]
pub struct ForkSimConfig {
    pub http_url: String,
    pub block_number: Option<u64>,
}

pub struct ForkSim {
    #[allow(dead_code)]
    config: ForkSimConfig,
}

impl ForkSim {
    pub fn new(config: ForkSimConfig) -> Self {
        Self { config }
    }

    /// V3 quote via REVM-hosted Quoter call. Phase 5 implementation.
    pub fn quote_v3(
        &mut self,
        _pool: Address,
        _token_in: Address,
        _token_out: Address,
        _fee: u32,
        _amount_in: U256,
    ) -> Result<U256> {
        anyhow::bail!("ForkSim::quote_v3 not yet implemented (Phase 5)");
    }

    /// Pre-flight an `executeArbitrage` calldata blob against the executor.
    /// Phase 5 implementation.
    pub fn simulate_arb(
        &mut self,
        _executor: Address,
        _calldata: Bytes,
        _value: U256,
    ) -> Result<SimOutcome> {
        anyhow::bail!("ForkSim::simulate_arb not yet implemented (Phase 5)");
    }
}
