//! `PoolEventCollector`: subscribes to V2 `Sync` and V3 `Swap` logs filtered
//! to tracked pool addresses.
//!
//! Real implementation lands in Phase 6.

use alloy::primitives::Address;
use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use super::Collector;
use crate::types::Event;

pub struct PoolEventCollector {
    pub wss_url: String,
    pub pools: Vec<Address>,
}

impl PoolEventCollector {
    pub fn new(wss_url: impl Into<String>, pools: Vec<Address>) -> Self {
        Self {
            wss_url: wss_url.into(),
            pools,
        }
    }
}

#[async_trait]
impl Collector for PoolEventCollector {
    async fn run(self: Box<Self>, _tx: mpsc::Sender<Event>) -> Result<()> {
        tracing::warn!(
            target: "collector::pool_event",
            tracked = self.pools.len(),
            "PoolEventCollector::run not yet implemented"
        );
        Ok(())
    }
}
