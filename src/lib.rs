//! Library surface for `arbi-bot`. Re-exports the modules so integration
//! tests under `tests/` can use them. The binary in `main.rs` wires these
//! same modules into the runtime pipeline.

#![allow(dead_code)]

pub mod bindings;
pub mod collectors;
pub mod config;
pub mod executor;
pub mod math;
pub mod simulator;
pub mod strategy;
pub mod types;
pub mod util;
