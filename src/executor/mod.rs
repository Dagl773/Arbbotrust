//! Executor service: enforces the profitability gate (CLAUDE.md §9), drives
//! pre-flight simulation, and either logs (scan/dry-run) or submits (live).

pub mod builder;
pub mod sender;

use std::sync::Arc;

use alloy::primitives::{Address, U256};
use anyhow::Result;
use tokio::sync::mpsc;

use crate::config::Mode;
use crate::simulator::revm_fork::ForkSim;
use crate::types::{Action, Opportunity};

#[allow(unused_imports)]
pub use builder::{build_execute_calldata, encode_path};
#[allow(unused_imports)]
pub use sender::{LiveSender, SendOutcome, SendRequest};

/// Profitability-gate parameters. CLAUDE.md §9.
#[derive(Debug, Clone)]
pub struct GateParams {
    /// Aave premium in basis points of `amount_in` (Aave V3 default = 5 bps).
    pub flash_loan_premium_bps: u16,
    /// Floor in `asset` units. The dollar conversion is the caller's job.
    pub min_profit_floor: U256,
    /// Skip submission if estimated `maxFeePerGas` would exceed this.
    pub max_gas_price_wei: u128,
}

impl Default for GateParams {
    fn default() -> Self {
        Self {
            // Aave V3 standard: https://aave.com/docs/aave-v3/guides/flash-loans
            flash_loan_premium_bps: 5,
            min_profit_floor: U256::from(1u64),
            max_gas_price_wei: 5_000_000_000,
        }
    }
}

pub struct ExecutorService {
    pub mode: Mode,
    pub executor_address: Option<Address>,
    pub sim: Option<Arc<ForkSim>>,
    pub sender: Option<Arc<LiveSender>>,
    pub gate: GateParams,
}

impl ExecutorService {
    /// Pull `Action`s off the channel and dispatch.
    pub async fn run(self, mut actions: mpsc::Receiver<Action>) -> Result<()> {
        while let Some(action) = actions.recv().await {
            match action {
                Action::Execute(opp) => {
                    if let Err(err) = self.handle_opportunity(&opp).await {
                        tracing::warn!(
                            target: "executor",
                            ?err,
                            block = opp.block_number,
                            "opportunity handling failed"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    async fn handle_opportunity(&self, opp: &Opportunity) -> Result<()> {
        // §9 step 1+2 are the detector's job. We pick up at step 3.
        let executor = match self.executor_address {
            Some(a) => a,
            None => {
                tracing::warn!(
                    target: "executor",
                    "FLASH_EXECUTOR_ADDRESS not set; cannot simulate or send"
                );
                return Ok(());
            }
        };

        // Build calldata using the gate's min_profit_floor as a contract-side
        // safety net. The off-chain gate below catches anything else.
        let calldata = build_execute_calldata(opp, self.gate.min_profit_floor, executor);

        // §9 step 3: pre-flight in REVM (eth_call against fork in v1).
        if let Some(sim) = &self.sim {
            let caller = self
                .sender
                .as_ref()
                .map(|s| s.signer_address())
                .unwrap_or(Address::ZERO);
            let outcome = sim
                .simulate_arb(executor, caller, calldata.clone(), opp.asset)
                .await?;
            if !outcome.success {
                tracing::info!(
                    target: "executor",
                    block = opp.block_number,
                    revert = ?outcome.revert_reason,
                    "pre-flight reverted; skipping"
                );
                return Ok(());
            }
            // §9 step 4: profit_after_repay ≥ floor.
            let premium = opp
                .amount_in
                .saturating_mul(U256::from(self.gate.flash_loan_premium_bps))
                / U256::from(10_000u64);
            if outcome.profit_after_repay < premium + self.gate.min_profit_floor {
                tracing::info!(
                    target: "executor",
                    block = opp.block_number,
                    profit = %outcome.profit_after_repay,
                    premium = %premium,
                    "below floor after Aave premium; skipping"
                );
                return Ok(());
            }
            tracing::info!(
                target: "executor",
                block = opp.block_number,
                profit_after_repay = %outcome.profit_after_repay,
                gas_used = outcome.gas_used,
                mode = ?self.mode,
                "pre-flight clean"
            );
        } else {
            tracing::warn!(target: "executor", "no simulator wired; skipping pre-flight");
        }

        // §9 step 5 + send.
        if matches!(self.mode, Mode::Live) {
            let Some(sender) = &self.sender else {
                tracing::error!(
                    target: "executor",
                    "live mode but no sender configured"
                );
                return Ok(());
            };
            let request = SendRequest {
                to: executor,
                data: calldata,
                value: U256::ZERO,
                gas_limit: 5_000_000,
                max_fee_per_gas_wei: self.gate.max_gas_price_wei,
                max_priority_fee_per_gas_wei: 0,
            };
            match sender.send(request).await {
                Ok(outcome) => tracing::info!(
                    target: "executor",
                    tx_hash = %outcome.tx_hash,
                    block = ?outcome.block_number,
                    "live tx submitted"
                ),
                Err(err) => tracing::warn!(target: "executor", ?err, "submit failed"),
            }
        } else {
            tracing::info!(
                target: "executor",
                mode = ?self.mode,
                block = opp.block_number,
                expected_profit = %opp.expected_profit,
                "scan-only: would submit"
            );
        }
        Ok(())
    }
}
