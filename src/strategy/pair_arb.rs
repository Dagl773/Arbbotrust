//! Pair arbitrage detector — full implementation lands in Phase 7.

use crate::types::{Action, Event};

use super::{Detector, PoolRegistry};

pub struct PairArbDetector;

impl Detector for PairArbDetector {
    fn name(&self) -> &'static str {
        "pair_arb"
    }
    fn on_event(&mut self, _event: &Event, _registry: &PoolRegistry) -> Vec<Action> {
        Vec::new()
    }
}
