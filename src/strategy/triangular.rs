//! Triangular / cyclic V2 arbitrage. CLAUDE.md §8.
//!
//! On a V2 pool reserve update, we walk every cycle of length 3 that passes
//! through the updated pool and check whether the round-trip rate exceeds 1.
//! v1 restricts to V2 hops because V3 quoting is async; multi-leg V3 paths
//! land in a follow-up phase.

use alloy::primitives::{Address, U256};

use super::{Detector, PoolRegistry, path_finder::build_adjacency};
use crate::math::uniswap_v2::get_amount_out;
use crate::types::{Action, DexKind, Event, Hop, Opportunity, PoolUpdateKind};

pub struct TriangularDetector {
    /// Minimum gross profit (in starting-asset units) to emit.
    pub min_profit: U256,
    /// Probe input used for cycle detection. Real systems iterate to find
    /// the optimal input; for v1 we use a fixed probe and let the executor's
    /// profitability gate filter out marginal cycles.
    pub probe_amount_in: U256,
}

impl Default for TriangularDetector {
    fn default() -> Self {
        Self {
            min_profit: U256::from(1u64),
            probe_amount_in: U256::from(1_000_000u64), // 1 USDC at 6 decimals
        }
    }
}

impl Detector for TriangularDetector {
    fn name(&self) -> &'static str {
        "triangular"
    }

    fn on_event(&mut self, event: &Event, registry: &PoolRegistry) -> Vec<Action> {
        let Event::PoolUpdate {
            pool: updated_addr,
            kind: PoolUpdateKind::V2Sync { .. },
            block_number,
        } = event
        else {
            return Vec::new();
        };
        let Some(updated) = registry.get(*updated_addr) else {
            return Vec::new();
        };
        if updated.entry.dex == DexKind::UniV3 || updated.reserves.is_none() {
            return Vec::new();
        }

        let adj = build_adjacency(registry);

        let mut out = Vec::new();
        for &start_token in &[updated.entry.token0, updated.entry.token1] {
            if let Some(opp) =
                self.try_cycles_from(start_token, *updated_addr, registry, &adj, *block_number)
            {
                tracing::info!(
                    target: "strategy::triangular",
                    pool = %updated_addr,
                    start = %start_token,
                    amount_in = %opp.amount_in,
                    gross_profit = %opp.expected_profit,
                    "cycle detected"
                );
                out.push(Action::Execute(Box::new(opp)));
            }
        }
        out
    }
}

impl TriangularDetector {
    /// Walks length-3 cycles `start_token → mid → tail → start_token` where the
    /// `start_token → mid` hop is the freshly-updated pool. Returns the best
    /// profitable cycle, or `None`.
    fn try_cycles_from(
        &self,
        start: Address,
        updated_pool: Address,
        registry: &PoolRegistry,
        adj: &std::collections::HashMap<Address, Vec<(Address, Address)>>,
        block_number: u64,
    ) -> Option<Opportunity> {
        let updated = registry.get(updated_pool)?;
        let (a_in, a_out, mid_token) = if updated.entry.token0 == start {
            (
                updated.reserves?.0,
                updated.reserves?.1,
                updated.entry.token1,
            )
        } else {
            (
                updated.reserves?.1,
                updated.reserves?.0,
                updated.entry.token0,
            )
        };

        let neighbours = adj.get(&mid_token)?;
        let mut best: Option<Opportunity> = None;

        for &(tail_token, mid_pool_addr) in neighbours {
            if tail_token == start || mid_pool_addr == updated_pool {
                continue;
            }
            let mid_pool = registry.get(mid_pool_addr)?;
            if mid_pool.entry.dex == DexKind::UniV3 {
                continue;
            }
            let Some((mr0, mr1)) = mid_pool.reserves else {
                continue;
            };
            let (m_in, m_out) = if mid_pool.entry.token0 == mid_token {
                (mr0, mr1)
            } else {
                (mr1, mr0)
            };

            // Find a tail pool: tail_token → start.
            let tail_neighbours = adj.get(&tail_token)?;
            for &(end_token, tail_pool_addr) in tail_neighbours {
                if end_token != start
                    || tail_pool_addr == updated_pool
                    || tail_pool_addr == mid_pool_addr
                {
                    continue;
                }
                let tail_pool = registry.get(tail_pool_addr)?;
                if tail_pool.entry.dex == DexKind::UniV3 {
                    continue;
                }
                let Some((tr0, tr1)) = tail_pool.reserves else {
                    continue;
                };
                let (t_in, t_out) = if tail_pool.entry.token0 == tail_token {
                    (tr0, tr1)
                } else {
                    (tr1, tr0)
                };

                // Probe quote.
                let amount_in = self.probe_amount_in;
                let after1 = get_amount_out(amount_in, a_in, a_out, updated.entry.fee_bps)?;
                let after2 = get_amount_out(after1, m_in, m_out, mid_pool.entry.fee_bps)?;
                let after3 = get_amount_out(after2, t_in, t_out, tail_pool.entry.fee_bps)?;
                if after3 <= amount_in {
                    continue;
                }
                let profit = after3 - amount_in;
                if profit < self.min_profit {
                    continue;
                }

                let candidate = Opportunity {
                    asset: start,
                    amount_in,
                    hops: vec![
                        Hop {
                            dex: updated.entry.dex,
                            pool: updated_pool,
                            token_in: start,
                            token_out: mid_token,
                            fee: 0,
                        },
                        Hop {
                            dex: mid_pool.entry.dex,
                            pool: mid_pool_addr,
                            token_in: mid_token,
                            token_out: tail_token,
                            fee: 0,
                        },
                        Hop {
                            dex: tail_pool.entry.dex,
                            pool: tail_pool_addr,
                            token_in: tail_token,
                            token_out: start,
                            fee: 0,
                        },
                    ],
                    expected_profit: profit,
                    block_number,
                };
                if best
                    .as_ref()
                    .is_none_or(|b| candidate.expected_profit > b.expected_profit)
                {
                    best = Some(candidate);
                }
            }
        }
        best
    }
}
