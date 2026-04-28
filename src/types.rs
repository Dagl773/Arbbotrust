//! Shared types crossing the collector → strategy → executor channel boundary.
//!
//! See `CLAUDE.md` §5 for the architecture and §7 for the on-chain `ArbPath`
//! shape that `Opportunity::path` mirrors off-chain.

use alloy::primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};

/// On-chain DEX kind. Matches the Solidity enum in `FlashExecutor.sol`.
/// Order MUST stay in sync with the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum DexKind {
    UniV2 = 0,
    UniV3 = 1,
    CamelotV2 = 2,
}

/// One swap step inside an arbitrage path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hop {
    pub dex: DexKind,
    pub pool: Address,
    pub token_in: Address,
    pub token_out: Address,
    /// V3 fee tier in raw units (e.g. 3000 = 0.3%). Ignored for V2 hops.
    pub fee: u32,
}

/// Full path: borrow `asset`, walk `hops`, repay `asset + premium`, bank profit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Opportunity {
    pub asset: Address,
    pub amount_in: U256,
    pub hops: Vec<Hop>,
    /// Off-chain estimate; revalidated in REVM before submission.
    pub expected_profit: U256,
    /// Block this opportunity was detected on.
    pub block_number: u64,
}

/// Internal event emitted by collectors.
#[derive(Debug, Clone)]
pub enum Event {
    NewBlock {
        number: u64,
        hash: B256,
        timestamp: u64,
    },
    PoolUpdate {
        pool: Address,
        kind: PoolUpdateKind,
        block_number: u64,
    },
}

#[derive(Debug, Clone)]
pub enum PoolUpdateKind {
    /// Uniswap V2 `Sync(uint112,uint112)`.
    V2Sync { reserve0: U256, reserve1: U256 },
    /// Uniswap V3 `Swap(...)` — we read `slot0` post-event, this just signals.
    V3SwapTouched,
}

/// Dispatcher → executor.
#[derive(Debug, Clone)]
pub enum Action {
    Execute(Box<Opportunity>),
}

/// Result of a REVM pre-flight simulation.
#[derive(Debug, Clone)]
pub struct SimOutcome {
    pub success: bool,
    pub gas_used: u64,
    /// Net profit after Aave premium repayment, in `Opportunity::asset` units.
    pub profit_after_repay: U256,
    pub revert_reason: Option<String>,
}
