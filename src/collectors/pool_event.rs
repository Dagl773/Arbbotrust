//! `PoolEventCollector`: subscribes to V2 `Sync` and V3 `Swap` logs filtered
//! to tracked pool addresses, decodes the V2 reserve update payload, and
//! emits [`Event::PoolUpdate`].

use std::time::Duration;

use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder, WsConnect};
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use futures::StreamExt;
use tokio::sync::mpsc;

use super::Collector;
use crate::bindings::{uniswap_v2::IUniswapV2Pair, uniswap_v3::IUniswapV3Pool};
use crate::types::{Event, PoolUpdateKind};

const RETRY_ATTEMPTS: u32 = 3;
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);

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

    async fn run_once(&self, tx: &mpsc::Sender<Event>) -> Result<()> {
        if self.pools.is_empty() {
            tracing::warn!(target: "collector::pool_event", "no pools to track; idling");
            return Ok(());
        }
        let ws = WsConnect::new(&self.wss_url);
        let provider = ProviderBuilder::new()
            .connect_ws(ws)
            .await
            .context("connect WSS")?;

        let v2_sync_topic = IUniswapV2Pair::Sync::SIGNATURE_HASH;
        let v3_swap_topic = IUniswapV3Pool::Swap::SIGNATURE_HASH;
        let filter = Filter::new()
            .address(self.pools.clone())
            .event_signature(vec![v2_sync_topic, v3_swap_topic]);

        let sub = provider
            .subscribe_logs(&filter)
            .await
            .context("subscribe logs")?;
        let mut stream = sub.into_stream();

        while let Some(log) = stream.next().await {
            let Some(topic0) = log.topic0().copied() else {
                continue;
            };
            let block_number = log.block_number.unwrap_or_default();
            let pool = log.address();

            let kind = if topic0 == v2_sync_topic {
                match decode_v2_sync(&log.data().data) {
                    Some(k) => k,
                    None => {
                        tracing::warn!(
                            target: "collector::pool_event",
                            pool = %pool,
                            "failed to decode Sync event payload"
                        );
                        continue;
                    }
                }
            } else if topic0 == v3_swap_topic {
                PoolUpdateKind::V3SwapTouched
            } else {
                continue;
            };

            let event = Event::PoolUpdate {
                pool,
                kind,
                block_number,
            };
            if tx.send(event).await.is_err() {
                tracing::warn!(
                    target: "collector::pool_event",
                    "downstream channel closed; stopping"
                );
                return Ok(());
            }
        }
        Ok(())
    }
}

fn decode_v2_sync(data: &[u8]) -> Option<PoolUpdateKind> {
    // Sync(uint112,uint112) — 32-byte-padded reserves in the data field.
    if data.len() < 64 {
        return None;
    }
    let reserve0 = U256::from_be_slice(&data[0..32]);
    let reserve1 = U256::from_be_slice(&data[32..64]);
    Some(PoolUpdateKind::V2Sync { reserve0, reserve1 })
}

#[async_trait]
impl Collector for PoolEventCollector {
    async fn run(self: Box<Self>, tx: mpsc::Sender<Event>) -> Result<()> {
        let mut backoff = INITIAL_BACKOFF;
        for attempt in 1..=RETRY_ATTEMPTS {
            match self.run_once(&tx).await {
                Ok(()) => {
                    tracing::info!(target: "collector::pool_event", "log stream ended cleanly");
                    return Ok(());
                }
                Err(err) if attempt < RETRY_ATTEMPTS => {
                    tracing::warn!(
                        target: "collector::pool_event",
                        attempt,
                        backoff_ms = backoff.as_millis() as u64,
                        ?err,
                        "log subscription failed; retrying"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff *= 2;
                }
                Err(err) => {
                    tracing::error!(
                        target: "collector::pool_event",
                        ?err,
                        attempts = attempt,
                        "giving up"
                    );
                    return Err(err);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_v2_sync_payload() {
        // reserves: 1e18 and 2e6
        let mut data = [0u8; 64];
        let r0 = U256::from(10u128.pow(18));
        let r1 = U256::from(2_000_000u64);
        data[0..32].copy_from_slice(&r0.to_be_bytes::<32>());
        data[32..64].copy_from_slice(&r1.to_be_bytes::<32>());
        let kind = decode_v2_sync(&data).unwrap();
        let PoolUpdateKind::V2Sync { reserve0, reserve1 } = kind else {
            panic!("wrong kind");
        };
        assert_eq!(reserve0, r0);
        assert_eq!(reserve1, r1);
    }

    #[test]
    fn rejects_short_payload() {
        assert!(decode_v2_sync(&[0u8; 32]).is_none());
    }
}
