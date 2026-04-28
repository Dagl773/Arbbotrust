//! Token-graph helpers used by the triangular detector. Depth ≤ 3 in v1.

use std::collections::HashMap;

use alloy::primitives::Address;

use super::pool_registry::PoolRegistry;

/// Adjacency: `token -> [(neighbor_token, pool)]`.
pub fn build_adjacency(registry: &PoolRegistry) -> HashMap<Address, Vec<(Address, Address)>> {
    let mut adj: HashMap<Address, Vec<(Address, Address)>> = HashMap::new();
    for (pool_addr, state) in registry.iter() {
        let entry = &state.entry;
        adj.entry(entry.token0)
            .or_default()
            .push((entry.token1, *pool_addr));
        adj.entry(entry.token1)
            .or_default()
            .push((entry.token0, *pool_addr));
    }
    adj
}
