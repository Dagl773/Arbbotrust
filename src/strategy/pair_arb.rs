//! V2-V2 pair arbitrage detector. CLAUDE.md §5 / §8.
//!
//! On every pool-reserve update, we look at every other tracked pool covering
//! the same token pair, run the closed-form optimal-input solver for each
//! (a, b) ordering, and emit an [`Action::Execute`] if positive gross profit
//! exists. V3 hops in pair-arb need REVM quoting; deferred to a future phase.

use alloy::primitives::U256;

use super::{Detector, PoolRegistry};
use crate::math::optimal::{V2Leg, optimal_v2_v2};
use crate::types::{Action, DexKind, Event, Hop, Opportunity, PoolUpdateKind};

pub struct PairArbDetector {
    /// Minimum gross profit (in `asset` units) to emit. Default 1 — caller's
    /// profitability gate (§9) does the dollar-converted final check.
    pub min_profit: U256,
}

impl Default for PairArbDetector {
    fn default() -> Self {
        Self {
            min_profit: U256::from(1u64),
        }
    }
}

impl Detector for PairArbDetector {
    fn name(&self) -> &'static str {
        "pair_arb"
    }

    fn on_event(&mut self, event: &Event, registry: &PoolRegistry) -> Vec<Action> {
        let Event::PoolUpdate {
            pool: updated_addr,
            kind,
            block_number,
        } = event
        else {
            return Vec::new();
        };
        if !matches!(kind, PoolUpdateKind::V2Sync { .. }) {
            return Vec::new();
        }
        let Some(updated) = registry.get(*updated_addr) else {
            return Vec::new();
        };
        if updated.entry.dex == DexKind::UniV3 {
            return Vec::new();
        }
        let Some((r0, r1)) = updated.reserves else {
            return Vec::new();
        };

        let mut out = Vec::new();
        let pair_pools = registry.pools_for_pair(updated.entry.token0, updated.entry.token1);
        for &other_addr in pair_pools {
            if other_addr == *updated_addr {
                continue;
            }
            let Some(other) = registry.get(other_addr) else {
                continue;
            };
            if other.entry.dex == DexKind::UniV3 {
                continue;
            }
            let Some((or0, or1)) = other.reserves else {
                continue;
            };

            // Try both directions: borrow token0, swap on updated → other → back to token0;
            // and borrow token1.
            for borrow_is_token0 in [true, false] {
                let (a_in, a_out, b_in, b_out, asset, mid_token) = if borrow_is_token0 {
                    (r0, r1, or1, or0, updated.entry.token0, updated.entry.token1)
                } else {
                    (r1, r0, or0, or1, updated.entry.token1, updated.entry.token0)
                };
                let opt = optimal_v2_v2(
                    V2Leg {
                        reserve_in: a_in,
                        reserve_out: a_out,
                        fee_bps: updated.entry.fee_bps,
                    },
                    V2Leg {
                        reserve_in: b_in,
                        reserve_out: b_out,
                        fee_bps: other.entry.fee_bps,
                    },
                );
                let Some(arb) = opt else { continue };
                if arb.gross_profit < self.min_profit {
                    continue;
                }
                let opp = Opportunity {
                    asset,
                    amount_in: arb.amount_in,
                    hops: vec![
                        Hop {
                            dex: updated.entry.dex,
                            pool: *updated_addr,
                            token_in: asset,
                            token_out: mid_token,
                            fee: 0,
                        },
                        Hop {
                            dex: other.entry.dex,
                            pool: other_addr,
                            token_in: mid_token,
                            token_out: asset,
                            fee: 0,
                        },
                    ],
                    expected_profit: arb.gross_profit,
                    block_number: *block_number,
                };
                tracing::info!(
                    target: "strategy::pair_arb",
                    pool_a = %updated_addr,
                    pool_b = %other_addr,
                    %asset,
                    amount_in = %arb.amount_in,
                    gross_profit = %arb.gross_profit,
                    "opportunity detected"
                );
                out.push(Action::Execute(Box::new(opp)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::pool_registry::{PoolEntry, PoolRegistry};
    use alloy::primitives::address;

    #[test]
    fn emits_action_when_pair_imbalance_exists() {
        let token0 = address!("0000000000000000000000000000000000000010");
        let token1 = address!("0000000000000000000000000000000000000020");
        let pool_a = address!("000000000000000000000000000000000000aaaa");
        let pool_b = address!("000000000000000000000000000000000000bbbb");

        let mut reg = PoolRegistry::new();
        reg.insert(PoolEntry {
            address: pool_a,
            dex: DexKind::UniV2,
            token0,
            token1,
            fee: 0,
            fee_bps: 30,
        });
        reg.insert(PoolEntry {
            address: pool_b,
            dex: DexKind::UniV2,
            token0,
            token1,
            fee: 0,
            fee_bps: 30,
        });

        // Seed both pools with reserves directly via apply_event.
        reg.apply_event(&Event::PoolUpdate {
            pool: pool_b,
            kind: PoolUpdateKind::V2Sync {
                reserve0: U256::from(150_000_000u64),
                reserve1: U256::from(200_000_000u64),
            },
            block_number: 1,
        });
        // The "updated" pool whose update triggers detection.
        reg.apply_event(&Event::PoolUpdate {
            pool: pool_a,
            kind: PoolUpdateKind::V2Sync {
                reserve0: U256::from(100_000_000u64),
                reserve1: U256::from(100_000_000u64),
            },
            block_number: 2,
        });

        let mut det = PairArbDetector::default();
        let actions = det.on_event(
            &Event::PoolUpdate {
                pool: pool_a,
                kind: PoolUpdateKind::V2Sync {
                    reserve0: U256::from(100_000_000u64),
                    reserve1: U256::from(100_000_000u64),
                },
                block_number: 2,
            },
            &reg,
        );

        assert!(!actions.is_empty(), "expected at least one Execute action");
        let Action::Execute(opp) = &actions[0];
        assert_eq!(opp.hops.len(), 2);
        assert_eq!(opp.block_number, 2);
        assert!(opp.expected_profit > U256::ZERO);
    }

    #[test]
    fn ignores_non_v2_events() {
        let mut det = PairArbDetector::default();
        let reg = PoolRegistry::new();
        let evt = Event::NewBlock {
            number: 1,
            hash: alloy::primitives::B256::ZERO,
            timestamp: 0,
        };
        assert!(det.on_event(&evt, &reg).is_empty());
    }
}
