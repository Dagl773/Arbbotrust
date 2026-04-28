//! Triangular / cyclic arbitrage detector — full implementation lands in Phase 7.

use crate::types::{Action, Event};

use super::{Detector, PoolRegistry};

pub struct TriangularDetector;

impl Detector for TriangularDetector {
    fn name(&self) -> &'static str {
        "triangular"
    }
    fn on_event(&mut self, _event: &Event, _registry: &PoolRegistry) -> Vec<Action> {
        Vec::new()
    }
}
