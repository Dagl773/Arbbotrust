//! Transaction signer + sender. Phase 8 implementation.

use alloy::primitives::{Address, Bytes, U256};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct SendRequest {
    pub to: Address,
    pub data: Bytes,
    pub value: U256,
    pub gas_limit: u64,
    pub max_fee_per_gas_wei: u128,
}

#[derive(Debug, Clone)]
pub struct SendOutcome {
    pub tx_hash: alloy::primitives::B256,
    pub block_number: Option<u64>,
}

pub trait Sender: Send {
    fn send(&self, request: SendRequest) -> Result<SendOutcome>;
}
