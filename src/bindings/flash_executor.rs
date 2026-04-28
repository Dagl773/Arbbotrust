//! Bindings for our own `FlashExecutor.sol`.
//!
//! Per CLAUDE.md §14.8: any change to `contracts/src/FlashExecutor.sol` MUST
//! land in the same commit as a regeneration of this file.

use alloy::sol;

sol! {
    /// Mirrors the Solidity `enum DexKind`. Order is load-bearing.
    #[derive(Debug, PartialEq, Eq)]
    enum DexKind {
        UniV2,
        UniV3,
        CamelotV2,
    }

    /// Mirrors the Solidity `struct Hop` decoded inside `executeOperation`.
    #[derive(Debug, PartialEq, Eq)]
    struct Hop {
        uint8 dex;
        address pool;
        address tokenIn;
        address tokenOut;
        uint24 fee;
    }

    /// Mirrors the Solidity `struct ArbPath`.
    #[derive(Debug, PartialEq, Eq)]
    struct ArbPath {
        Hop[] hops;
    }

    #[sol(rpc)]
    interface IFlashExecutor {
        event ArbExecuted(uint256 grossOut, uint256 fee, uint256 profit);

        function executeArbitrage(
            address asset,
            uint256 amount,
            bytes calldata path,
            uint256 minProfit
        ) external;

        function withdraw(address token, uint256 amount) external;
        function rescueETH() external;
        function owner() external view returns (address);
    }
}
