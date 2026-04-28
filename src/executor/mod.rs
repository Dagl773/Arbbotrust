//! Executor: REVM pre-flight → sign → submit. CLAUDE.md §5, §9.

pub mod builder;
pub mod sender;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::types::Action;

pub struct ExecutorService {
    pub dry_run: bool,
}

impl ExecutorService {
    pub async fn run(self, mut actions: mpsc::Receiver<Action>) -> Result<()> {
        while let Some(action) = actions.recv().await {
            match action {
                Action::Execute(opp) => {
                    tracing::info!(
                        target: "executor",
                        block = opp.block_number,
                        amount_in = %opp.amount_in,
                        expected_profit = %opp.expected_profit,
                        dry_run = self.dry_run,
                        "received opportunity"
                    );
                    // Phase 8 wires REVM sim → eth_estimateGas → sign → send.
                }
            }
        }
        Ok(())
    }
}
