//! `BlockCollector`: subscribes to `newHeads` over WSS and emits
//! [`Event::NewBlock`].
//!
//! Real implementation lands in Phase 6 once the alloy provider plumbing is
//! in place. The struct compiles today so the runtime can wire it up.

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use super::Collector;
use crate::types::Event;

pub struct BlockCollector {
    pub wss_url: String,
}

impl BlockCollector {
    pub fn new(wss_url: impl Into<String>) -> Self {
        Self {
            wss_url: wss_url.into(),
        }
    }
}

#[async_trait]
impl Collector for BlockCollector {
    async fn run(self: Box<Self>, _tx: mpsc::Sender<Event>) -> Result<()> {
        tracing::warn!(target: "collector::block", "BlockCollector::run not yet implemented");
        Ok(())
    }
}
