//! Strategy dispatcher. Owns the [`PoolRegistry`] and runs the registered
//! detectors on each incoming [`Event`], emitting [`Action`] for the executor.

pub mod pair_arb;
pub mod path_finder;
pub mod pool_registry;
pub mod triangular;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::types::{Action, Event};

pub use pool_registry::PoolRegistry;

pub trait Detector: Send {
    fn name(&self) -> &'static str;
    fn on_event(&mut self, event: &Event, registry: &PoolRegistry) -> Vec<Action>;
}

pub struct StrategyRunner {
    registry: PoolRegistry,
    detectors: Vec<Box<dyn Detector>>,
}

impl StrategyRunner {
    pub fn new(registry: PoolRegistry, detectors: Vec<Box<dyn Detector>>) -> Self {
        Self {
            registry,
            detectors,
        }
    }

    pub async fn run(
        mut self,
        mut events: mpsc::Receiver<Event>,
        actions: mpsc::Sender<Action>,
    ) -> Result<()> {
        while let Some(event) = events.recv().await {
            self.registry.apply_event(&event);
            for detector in &mut self.detectors {
                for action in detector.on_event(&event, &self.registry) {
                    if actions.send(action).await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
}
