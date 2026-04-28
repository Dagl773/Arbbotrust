use alloy::sol;

sol! {
    #[sol(rpc)]
    interface IAavePool {
        function flashLoanSimple(
            address receiverAddress,
            address asset,
            uint256 amount,
            bytes calldata params,
            uint16 referralCode
        ) external;

        function FLASHLOAN_PREMIUM_TOTAL() external view returns (uint128);
    }

    #[sol(rpc)]
    interface IFlashLoanSimpleReceiver {
        function executeOperation(
            address asset,
            uint256 amount,
            uint256 premium,
            address initiator,
            bytes calldata params
        ) external returns (bool);
    }
}
