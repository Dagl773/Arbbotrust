//! Collectors: external chain events → internal `Event` enum.
//!
//! See CLAUDE.md §5. Each collector runs on its own task and pushes into a
//! shared `tokio::mpsc::Sender<Event>`.

pub mod block;
pub mod pool_event;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::types::Event;

#[async_trait]
pub trait Collector: Send + 'static {
    /// Run forever (or until the channel closes). Bound retries internally
    /// per CLAUDE.md §15.9.
    async fn run(self: Box<Self>, tx: mpsc::Sender<Event>) -> Result<()>;
}
