//! `Opportunity` → `executeArbitrage` calldata.

use alloy::primitives::{Address, Bytes, U256};
use alloy::sol_types::{SolCall, SolValue};

use crate::bindings::flash_executor::{ArbPath, Hop as SolHop, IFlashExecutor};
use crate::types::Opportunity;

/// ABI-encode the off-chain [`Opportunity`] hops into the `bytes path`
/// parameter expected by `FlashExecutor.executeArbitrage`.
pub fn encode_path(opp: &Opportunity) -> Bytes {
    let hops: Vec<SolHop> = opp
        .hops
        .iter()
        .map(|h| SolHop {
            dex: h.dex as u8,
            pool: h.pool,
            tokenIn: h.token_in,
            tokenOut: h.token_out,
            fee: alloy::primitives::aliases::U24::from(h.fee),
        })
        .collect();
    let path = ArbPath { hops };
    Bytes::from(path.abi_encode())
}

/// Build the full `executeArbitrage(asset, amount, path, minProfit)`
/// calldata for the deployed `FlashExecutor`.
pub fn build_execute_calldata(
    opp: &Opportunity,
    min_profit: U256,
    _executor: Address, // future: route through a contract handle
) -> Bytes {
    let path = encode_path(opp);
    let call = IFlashExecutor::executeArbitrageCall {
        asset: opp.asset,
        amount: opp.amount_in,
        path,
        minProfit: min_profit,
    };
    Bytes::from(call.abi_encode())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DexKind, Hop};
    use alloy::primitives::address;

    fn make_opp() -> Opportunity {
        Opportunity {
            asset: address!("0000000000000000000000000000000000000001"),
            amount_in: U256::from(1_000_000u64),
            hops: vec![
                Hop {
                    dex: DexKind::UniV2,
                    pool: address!("0000000000000000000000000000000000000002"),
                    token_in: address!("0000000000000000000000000000000000000003"),
                    token_out: address!("0000000000000000000000000000000000000004"),
                    fee: 0,
                },
                Hop {
                    dex: DexKind::UniV3,
                    pool: address!("0000000000000000000000000000000000000005"),
                    token_in: address!("0000000000000000000000000000000000000004"),
                    token_out: address!("0000000000000000000000000000000000000003"),
                    fee: 3000,
                },
            ],
            expected_profit: U256::from(123u64),
            block_number: 42,
        }
    }

    #[test]
    fn encode_path_round_trips() {
        let opp = make_opp();
        let bytes = encode_path(&opp);
        let decoded = ArbPath::abi_decode(&bytes).unwrap();
        assert_eq!(decoded.hops.len(), 2);
        assert_eq!(decoded.hops[0].dex, DexKind::UniV2 as u8);
        assert_eq!(
            decoded.hops[1].fee,
            alloy::primitives::aliases::U24::from(3000u32)
        );
    }

    #[test]
    fn execute_calldata_decodes_to_call() {
        let opp = make_opp();
        let cd = build_execute_calldata(&opp, U256::from(50u64), Address::ZERO);
        // First 4 bytes: function selector for executeArbitrage(address,uint256,bytes,uint256).
        assert!(cd.len() >= 4);
        let selector = IFlashExecutor::executeArbitrageCall::SELECTOR;
        assert_eq!(&cd[0..4], &selector);
    }
}
