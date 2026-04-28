// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import {FlashLoanSimpleReceiverBase} from
    "@aave/misc/flashloan/base/FlashLoanSimpleReceiverBase.sol";
import {IPoolAddressesProvider} from "@aave/interfaces/IPoolAddressesProvider.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {SafeCast} from "@openzeppelin/contracts/utils/math/SafeCast.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/// @notice Per-hop swap descriptor decoded inside `executeOperation`.
/// Order MUST stay in sync with `src/types.rs::DexKind` and
/// `src/bindings/flash_executor.rs::DexKind`.
enum DexKind {
    UniV2,
    UniV3,
    CamelotV2
}

interface IUniswapV2Pair {
    function swap(uint256 amount0Out, uint256 amount1Out, address to, bytes calldata data)
        external;
    function token0() external view returns (address);
    function token1() external view returns (address);
    function getReserves()
        external
        view
        returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast);
}

interface IUniswapV3Pool {
    function token0() external view returns (address);
    function token1() external view returns (address);
    function fee() external view returns (uint24);
    function swap(
        address recipient,
        bool zeroForOne,
        int256 amountSpecified,
        uint160 sqrtPriceLimitX96,
        bytes calldata data
    ) external returns (int256 amount0, int256 amount1);
}

/// @title FlashExecutor
/// @notice Single-asset Aave V3 flash-loan arbitrage executor.
///
/// CLAUDE.md §7. Owner (single EOA in v1; multisig migration is a future
/// switch in `transferOwnership`) can call `executeArbitrage(asset, amount,
/// path, minProfit)`. The contract takes a `flashLoanSimple` from Aave,
/// walks `path` through Uniswap V2/V3 + Camelot V2 pools, repays the loan,
/// and asserts a minimum profit before returning.
contract FlashExecutor is FlashLoanSimpleReceiverBase, Ownable, ReentrancyGuard {
    using SafeERC20 for IERC20;
    using SafeCast for uint256;
    using SafeCast for int256;

    struct Hop {
        uint8 dex; // matches DexKind enum
        address pool;
        address tokenIn;
        address tokenOut;
        uint24 fee; // V3 only; ignored for V2 hops
    }

    struct ArbPath {
        Hop[] hops;
    }

    /// V3 sqrtPrice limits — copied from Uniswap's TickMath bounds (±1).
    uint160 private constant MIN_SQRT_RATIO_PLUS_ONE = 4295128740;
    uint160 private constant MAX_SQRT_RATIO_MINUS_ONE =
        1461446703485210103287273052203988822378723970341;

    event ArbExecuted(uint256 grossOut, uint256 fee, uint256 profit);

    error UnknownDex(uint8 dex);
    error EmptyPath();
    error PathStartMismatch(address expected, address got);
    error PathEndMismatch(address expected, address got);
    error InsufficientProfit(uint256 balance, uint256 owed, uint256 minProfit);
    error UnauthorisedCallback(address caller);
    error InitiatorNotOwner(address initiator);

    constructor(IPoolAddressesProvider provider, address initialOwner)
        FlashLoanSimpleReceiverBase(provider)
        Ownable(initialOwner)
    {}

    /// @notice Owner-only entry point. Triggers an Aave flash loan whose
    /// callback executes `path`.
    function executeArbitrage(address asset, uint256 amount, bytes calldata path, uint256 minProfit)
        external
        onlyOwner
        nonReentrant
    {
        bytes memory params = abi.encode(path, minProfit);
        POOL.flashLoanSimple(address(this), asset, amount, params, 0);
    }

    /// @notice Aave V3 flash-loan callback. Only callable by the Aave Pool,
    /// and only when the original initiator was this contract.
    function executeOperation(
        address asset,
        uint256 amount,
        uint256 premium,
        address initiator,
        bytes calldata params
    ) external override returns (bool) {
        if (msg.sender != address(POOL)) revert UnauthorisedCallback(msg.sender);
        if (initiator != address(this)) revert InitiatorNotOwner(initiator);

        (bytes memory rawPath, uint256 minProfit) = abi.decode(params, (bytes, uint256));
        ArbPath memory arb = abi.decode(rawPath, (ArbPath));
        if (arb.hops.length == 0) revert EmptyPath();
        if (arb.hops[0].tokenIn != asset) {
            revert PathStartMismatch(asset, arb.hops[0].tokenIn);
        }
        if (arb.hops[arb.hops.length - 1].tokenOut != asset) {
            revert PathEndMismatch(asset, arb.hops[arb.hops.length - 1].tokenOut);
        }

        // Walk hops; track running balance via balanceOf rather than per-hop
        // amount-out reporting (resilient to fee-on-transfer tokens).
        uint256 balanceBefore = IERC20(asset).balanceOf(address(this));
        for (uint256 i = 0; i < arb.hops.length; i++) {
            _swap(arb.hops[i]);
        }
        uint256 grossOut = IERC20(asset).balanceOf(address(this));

        uint256 owed = amount + premium;
        if (grossOut < owed + minProfit) {
            revert InsufficientProfit(grossOut, owed, minProfit);
        }

        // Aave pulls `owed` via `transferFrom`; pre-approve.
        IERC20(asset).forceApprove(address(POOL), owed);

        emit ArbExecuted(grossOut - balanceBefore, premium, grossOut - owed);
        return true;
    }

    function _swap(Hop memory h) internal {
        if (h.dex == uint8(DexKind.UniV2) || h.dex == uint8(DexKind.CamelotV2)) {
            _swapV2(h);
        } else if (h.dex == uint8(DexKind.UniV3)) {
            _swapV3(h);
        } else {
            revert UnknownDex(h.dex);
        }
    }

    /// V2-style swap. We pre-compute amountOut off-chain — but here we just
    /// use `balanceOf(this)` for amountIn and pull the constant-product
    /// reserve math at the time of call.
    function _swapV2(Hop memory h) internal {
        IUniswapV2Pair pair = IUniswapV2Pair(h.pool);
        uint256 amountIn = IERC20(h.tokenIn).balanceOf(address(this));
        IERC20(h.tokenIn).safeTransfer(h.pool, amountIn);

        (uint112 reserve0, uint112 reserve1,) = pair.getReserves();
        bool zeroIn = pair.token0() == h.tokenIn;
        (uint256 reserveIn, uint256 reserveOut) =
            zeroIn ? (uint256(reserve0), uint256(reserve1)) : (uint256(reserve1), uint256(reserve0));

        // 0.3% standard V2 fee. Camelot V2 has dynamic fees but for v1 we
        // pre-compute amountIn off-chain accounting for them; using 30 bps
        // here is a conservative under-estimate that the callback's
        // minProfit gate catches if wrong.
        uint256 amountInWithFee = amountIn * 9970;
        uint256 amountOut =
            (amountInWithFee * reserveOut) / (reserveIn * 10_000 + amountInWithFee);

        (uint256 amount0Out, uint256 amount1Out) =
            zeroIn ? (uint256(0), amountOut) : (amountOut, uint256(0));
        pair.swap(amount0Out, amount1Out, address(this), new bytes(0));
    }

    /// V3 direct-pool swap. Implements `uniswapV3SwapCallback`.
    function _swapV3(Hop memory h) internal {
        IUniswapV3Pool pool = IUniswapV3Pool(h.pool);
        bool zeroForOne = pool.token0() == h.tokenIn;
        uint256 amountIn = IERC20(h.tokenIn).balanceOf(address(this));
        // Encode the input token + pool so the callback can settle.
        bytes memory data = abi.encode(h.pool, h.tokenIn);
        pool.swap(
            address(this),
            zeroForOne,
            amountIn.toInt256(),
            zeroForOne ? MIN_SQRT_RATIO_PLUS_ONE : MAX_SQRT_RATIO_MINUS_ONE,
            data
        );
    }

    function uniswapV3SwapCallback(int256 amount0Delta, int256 amount1Delta, bytes calldata data)
        external
    {
        (address pool, address tokenIn) = abi.decode(data, (address, address));
        if (msg.sender != pool) revert UnauthorisedCallback(msg.sender);
        // Whichever delta is positive is what the pool is owed; SafeCast
        // rejects the (impossible-here) negative branch.
        uint256 amountToPay =
            amount0Delta > 0 ? amount0Delta.toUint256() : amount1Delta.toUint256();
        IERC20(tokenIn).safeTransfer(pool, amountToPay);
    }

    /// Owner-only fund recovery. Handles non-loan dust (e.g. residual USDC).
    function withdraw(address token, uint256 amount) external onlyOwner nonReentrant {
        IERC20(token).safeTransfer(owner(), amount);
    }

    function rescueETH() external onlyOwner nonReentrant {
        (bool ok,) = owner().call{value: address(this).balance}("");
        require(ok, "eth rescue failed");
    }

    receive() external payable {}
}
