#!/usr/bin/env bash
# install_contracts.sh — pulls in the Foundry dependencies (aave-v3-origin,
# openzeppelin-contracts, forge-std). The lib/ tree is gitignored so contributors
# rehydrate it after a fresh checkout.

set -euo pipefail

cd "$(dirname "$0")/../contracts"

forge install foundry-rs/forge-std --no-git --shallow
forge install aave-dao/aave-v3-origin --no-git --shallow
forge install OpenZeppelin/openzeppelin-contracts --no-git --shallow

echo "contracts deps installed under contracts/lib/"
