//! `BlockCollector`: subscribes to `newHeads` over WSS and emits
//! [`Event::NewBlock`]. Bounded retry per CLAUDE.md §15.9.

use std::time::Duration;

use alloy::providers::{Provider, ProviderBuilder, WsConnect};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::mpsc;

use super::Collector;
use crate::types::Event;

const RETRY_ATTEMPTS: u32 = 3;
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);

pub struct BlockCollector {
    pub wss_url: String,
}

impl BlockCollector {
    pub fn new(wss_url: impl Into<String>) -> Self {
        Self {
            wss_url: wss_url.into(),
        }
    }

    async fn run_once(&self, tx: &mpsc::Sender<Event>) -> Result<()> {
        let ws = WsConnect::new(&self.wss_url);
        let provider = ProviderBuilder::new()
            .connect_ws(ws)
            .await
            .context("connect WSS")?;
        let sub = provider
            .subscribe_blocks()
            .await
            .context("subscribe newHeads")?;
        let mut stream = sub.into_stream();
        while let Some(header) = stream.next().await {
            let event = Event::NewBlock {
                number: header.number,
                hash: header.hash,
                timestamp: header.timestamp,
            };
            if tx.send(event).await.is_err() {
                tracing::warn!(target: "collector::block", "downstream channel closed; stopping");
                return Ok(());
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Collector for BlockCollector {
    async fn run(self: Box<Self>, tx: mpsc::Sender<Event>) -> Result<()> {
        let mut backoff = INITIAL_BACKOFF;
        for attempt in 1..=RETRY_ATTEMPTS {
            match self.run_once(&tx).await {
                Ok(()) => {
                    tracing::info!(
                        target: "collector::block",
                        "block stream ended cleanly"
                    );
                    return Ok(());
                }
                Err(err) if attempt < RETRY_ATTEMPTS => {
                    tracing::warn!(
                        target: "collector::block",
                        attempt,
                        backoff_ms = backoff.as_millis() as u64,
                        ?err,
                        "block subscription failed; retrying"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff *= 2;
                }
                Err(err) => {
                    tracing::error!(target: "collector::block", ?err, "giving up after {attempt} attempts");
                    return Err(err);
                }
            }
        }
        Ok(())
    }
}
