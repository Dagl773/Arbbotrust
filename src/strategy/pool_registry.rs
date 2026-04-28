//! In-memory state of all tracked pools (V2 reserves and V3 sqrt-price/tick
//! anchors). Loaded from `data/pools.json` on startup.

use std::collections::HashMap;
use std::path::Path;

use alloy::primitives::{Address, U256};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::types::{DexKind, Event, PoolUpdateKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolEntry {
    pub address: Address,
    pub dex: DexKind,
    pub token0: Address,
    pub token1: Address,
    /// Fee in 1e6 units for V3, ignored for V2 (V2 uses `fee_bps`).
    #[serde(default)]
    pub fee: u32,
    /// Fee in basis points for V2 (default 30 = 0.3%). Camelot V2 reads this
    /// dynamically per pool — Phase 7 hooks the refresh.
    #[serde(default = "default_fee_bps")]
    pub fee_bps: u16,
}

const fn default_fee_bps() -> u16 {
    30
}

#[derive(Debug, Clone, Default)]
pub struct PoolState {
    pub entry: PoolEntry,
    /// V2 reserves (token0, token1).
    pub reserves: Option<(U256, U256)>,
    /// V3 sqrtPriceX96 latest snapshot (refreshed lazily).
    pub sqrt_price_x96: Option<U256>,
    pub last_updated_block: u64,
}

impl PoolState {
    fn new(entry: PoolEntry) -> Self {
        Self {
            entry,
            reserves: None,
            sqrt_price_x96: None,
            last_updated_block: 0,
        }
    }
}

impl Default for PoolEntry {
    fn default() -> Self {
        Self {
            address: Address::ZERO,
            dex: DexKind::UniV2,
            token0: Address::ZERO,
            token1: Address::ZERO,
            fee: 0,
            fee_bps: default_fee_bps(),
        }
    }
}

#[derive(Debug, Default)]
pub struct PoolRegistry {
    pools: HashMap<Address, PoolState>,
    /// `(token0, token1)` (sorted) → list of pool addresses, for fast pair lookup.
    by_pair: HashMap<(Address, Address), Vec<Address>>,
}

impl PoolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_json_file(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read pool registry {}", path.display()))?;
        let entries: Vec<PoolEntry> = serde_json::from_str(&raw)
            .with_context(|| format!("parse pool registry {}", path.display()))?;
        let mut reg = Self::new();
        for entry in entries {
            reg.insert(entry);
        }
        Ok(reg)
    }

    pub fn insert(&mut self, entry: PoolEntry) {
        let key = sorted_pair(entry.token0, entry.token1);
        self.by_pair.entry(key).or_default().push(entry.address);
        self.pools.insert(entry.address, PoolState::new(entry));
    }

    pub fn get(&self, address: Address) -> Option<&PoolState> {
        self.pools.get(&address)
    }

    pub fn pools_for_pair(&self, a: Address, b: Address) -> &[Address] {
        self.by_pair
            .get(&sorted_pair(a, b))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Address, &PoolState)> {
        self.pools.iter()
    }

    pub fn len(&self) -> usize {
        self.pools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pools.is_empty()
    }

    /// Apply a collector event to local state. Silently ignores events for
    /// untracked pools.
    pub fn apply_event(&mut self, event: &Event) {
        match event {
            Event::PoolUpdate {
                pool,
                kind,
                block_number,
            } => {
                let Some(state) = self.pools.get_mut(pool) else {
                    return;
                };
                state.last_updated_block = *block_number;
                match kind {
                    PoolUpdateKind::V2Sync { reserve0, reserve1 } => {
                        state.reserves = Some((*reserve0, *reserve1));
                    }
                    PoolUpdateKind::V3SwapTouched => {
                        // Real impl will re-read slot0 via the simulator; for
                        // now we just note the block number.
                    }
                }
            }
            Event::NewBlock { .. } => {}
        }
    }
}

fn sorted_pair(a: Address, b: Address) -> (Address, Address) {
    if a < b { (a, b) } else { (b, a) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    #[test]
    fn insert_and_lookup() {
        let mut reg = PoolRegistry::new();
        let pool = address!("0000000000000000000000000000000000000001");
        let t0 = address!("0000000000000000000000000000000000000010");
        let t1 = address!("0000000000000000000000000000000000000020");
        reg.insert(PoolEntry {
            address: pool,
            dex: DexKind::UniV2,
            token0: t0,
            token1: t1,
            fee: 0,
            fee_bps: 30,
        });
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.pools_for_pair(t1, t0), &[pool]);
        assert!(reg.get(pool).is_some());
    }

    /// Sanity-check that the checked-in `data/pools.json` parses cleanly with
    /// every DexKind variant supported by the loader.
    #[test]
    fn checked_in_pools_json_parses() {
        let path = std::path::Path::new("data/pools.json");
        let reg = PoolRegistry::from_json_file(path).expect("data/pools.json must parse");
        assert!(reg.len() >= 5, "seed registry has at least 5 pools");
        let dex_kinds: std::collections::HashSet<_> =
            reg.iter().map(|(_, s)| s.entry.dex).collect();
        assert!(dex_kinds.contains(&DexKind::UniV2));
        assert!(dex_kinds.contains(&DexKind::UniV3));
        assert!(dex_kinds.contains(&DexKind::CamelotV2));
    }
}
