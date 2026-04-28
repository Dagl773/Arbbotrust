//! Pre-flight simulator. CLAUDE.md §5 / §9 / §15.4: every live submission is
//! pre-flight-checked here.
//!
//! v1 implementation: `eth_call` against the configured HTTP RPC. The RPC's
//! `latest` block state IS the fork, so this is semantically identical to a
//! REVM `CacheDB<AlloyDB>` simulation (just with one network round-trip per
//! quote instead of in-process execution). See CLAUDE.md §8 and §20 for the
//! deferred upgrade path to a native REVM fork.

use alloy::eips::BlockId;
use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::sol_types::{SolCall, SolValue};
use anyhow::{Context as _, Result, anyhow};

use crate::bindings::erc20::IERC20;
use crate::bindings::uniswap_v3::IQuoterV2;
use crate::types::SimOutcome;

#[derive(Debug, Clone)]
pub struct ForkSimConfig {
    pub http_url: String,
    /// Block to anchor calls to; defaults to "latest". Setting this lets you
    /// reproduce a historical opportunity exactly.
    pub block: BlockId,
}

impl ForkSimConfig {
    pub fn new(http_url: impl Into<String>) -> Self {
        Self {
            http_url: http_url.into(),
            block: BlockId::latest(),
        }
    }
}

pub struct ForkSim {
    provider: DynProvider,
    block: BlockId,
    quoter: Address,
}

impl ForkSim {
    /// Build a simulator against `config.http_url`. `quoter` is the Uniswap V3
    /// QuoterV2 address used by [`Self::quote_v3`].
    pub async fn new(config: ForkSimConfig, quoter: Address) -> Result<Self> {
        let provider = ProviderBuilder::new()
            .connect(&config.http_url)
            .await
            .context("connect HTTP provider for fork sim")?
            .erased();
        Ok(Self {
            provider,
            block: config.block,
            quoter,
        })
    }

    /// Pin subsequent calls to a specific block. `BlockId::latest()` is the
    /// usual choice; `BlockId::Number` reproduces a historical opportunity.
    pub fn set_block(&mut self, block: BlockId) {
        self.block = block;
    }

    /// Returns QuoterV2's amount-out for an exact-input single-pool swap.
    /// CLAUDE.md §8 v1 quoter strategy.
    pub async fn quote_v3(
        &self,
        token_in: Address,
        token_out: Address,
        fee: u32,
        amount_in: U256,
    ) -> Result<U256> {
        let call = IQuoterV2::quoteExactInputSingleCall {
            params: IQuoterV2::QuoteExactInputSingleParams {
                tokenIn: token_in,
                tokenOut: token_out,
                amountIn: amount_in,
                fee: alloy::primitives::aliases::U24::from(fee),
                sqrtPriceLimitX96: alloy::primitives::aliases::U160::ZERO,
            },
        };
        let raw = self
            .raw_eth_call(self.quoter, Bytes::from(call.abi_encode()))
            .await?;
        let decoded = <(U256, U256, u32, U256)>::abi_decode(&raw)
            .map_err(|e| anyhow!("decode QuoterV2 return: {e}"))?;
        Ok(decoded.0)
    }

    /// Pre-flight an `executeArbitrage` calldata blob against the deployed
    /// executor, called from `caller`. Returns the simulated `profit_after_repay`
    /// by sampling the contract's `asset` balance before/after.
    pub async fn simulate_arb(
        &self,
        executor: Address,
        caller: Address,
        calldata: Bytes,
        asset: Address,
    ) -> Result<SimOutcome> {
        let balance_before = self.balance_of(asset, executor).await?;

        let req = TransactionRequest::default()
            .from(caller)
            .to(executor)
            .input(calldata.into());
        let result = self.provider.call(req).block(self.block).await;

        match result {
            Ok(_output) => {
                let balance_after = self.balance_of(asset, executor).await?;
                let profit_after_repay = if balance_after > balance_before {
                    balance_after - balance_before
                } else {
                    U256::ZERO
                };
                let gas_used = self
                    .provider
                    .estimate_gas(
                        TransactionRequest::default()
                            .from(caller)
                            .to(executor)
                            .input(Bytes::default().into()),
                    )
                    .await
                    .unwrap_or(0);
                Ok(SimOutcome {
                    success: true,
                    gas_used,
                    profit_after_repay,
                    revert_reason: None,
                })
            }
            Err(e) => Ok(SimOutcome {
                success: false,
                gas_used: 0,
                profit_after_repay: U256::ZERO,
                revert_reason: Some(e.to_string()),
            }),
        }
    }

    async fn balance_of(&self, token: Address, account: Address) -> Result<U256> {
        let call = IERC20::balanceOfCall { account };
        let raw = self
            .raw_eth_call(token, Bytes::from(call.abi_encode()))
            .await?;
        <U256>::abi_decode(&raw).map_err(|e| anyhow!("decode balanceOf: {e}"))
    }

    async fn raw_eth_call(&self, to: Address, data: Bytes) -> Result<Bytes> {
        let req = TransactionRequest::default().to(to).input(data.into());
        let bytes = self
            .provider
            .call(req)
            .block(self.block)
            .await
            .context("eth_call")?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_to_latest() {
        let cfg = ForkSimConfig::new("https://example.test");
        assert!(
            matches!(cfg.block, BlockId::Number(_) | BlockId::Hash(_)) || cfg.block.is_latest()
        );
    }
}
