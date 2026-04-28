use alloy::sol;

sol! {
    #[sol(rpc)]
    interface IUniswapV2Pair {
        event Sync(uint112 reserve0, uint112 reserve1);
        event Swap(
            address indexed sender,
            uint256 amount0In,
            uint256 amount1In,
            uint256 amount0Out,
            uint256 amount1Out,
            address indexed to
        );

        function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast);
        function token0() external view returns (address);
        function token1() external view returns (address);
        function swap(uint256 amount0Out, uint256 amount1Out, address to, bytes calldata data) external;
    }

    #[sol(rpc)]
    interface IUniswapV2Factory {
        function getPair(address tokenA, address tokenB) external view returns (address);
        function allPairs(uint256 i) external view returns (address);
        function allPairsLength() external view returns (uint256);
    }

    /// Camelot V2 pair: same V2 surface plus dynamic fees (CLAUDE.md §8).
    #[sol(rpc)]
    interface ICamelotV2Pair {
        function getReserves() external view returns (uint112 reserve0, uint112 reserve1, uint16 token0FeePercent, uint16 token1FeePercent);
        function token0() external view returns (address);
        function token1() external view returns (address);
        function stableFee() external view returns (uint16);
        function volatileFee() external view returns (uint16);
    }
}
