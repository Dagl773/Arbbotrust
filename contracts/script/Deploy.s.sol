// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {IPoolAddressesProvider} from "@aave/interfaces/IPoolAddressesProvider.sol";
import {FlashExecutor} from "../src/FlashExecutor.sol";

/// @notice Deploys `FlashExecutor` against the Aave V3 PoolAddressesProvider
/// for the active network.
///
/// Required env:
///   - PRIVATE_KEY            (deployer)
///   - AAVE_ADDRESSES_PROVIDER (Aave V3 PoolAddressesProvider on the target chain)
///   - EXECUTOR_OWNER         (initial owner; defaults to deployer if unset)
contract DeployFlashExecutor is Script {
    function run() external returns (FlashExecutor executor) {
        uint256 pk = vm.envUint("PRIVATE_KEY");
        address provider = vm.envAddress("AAVE_ADDRESSES_PROVIDER");
        address deployer = vm.addr(pk);
        address owner = vm.envOr("EXECUTOR_OWNER", deployer);

        vm.startBroadcast(pk);
        executor = new FlashExecutor(IPoolAddressesProvider(provider), owner);
        vm.stopBroadcast();

        console.log("FlashExecutor deployed at:", address(executor));
        console.log("Owner:", owner);
        console.log("Aave Pool:", address(executor.POOL()));
    }
}
