#!/usr/bin/env bash
# deploy.sh — deploys FlashExecutor.sol to the chosen network via forge script.
#
# Usage:
#   scripts/deploy.sh sepolia       # Arbitrum Sepolia
#   scripts/deploy.sh mainnet       # Arbitrum One
#
# Requires .env with PRIVATE_KEY, AAVE_ADDRESSES_PROVIDER, and the target
# network's RPC URL. Refuses to deploy to mainnet without an extra
# confirmation prompt.

set -euo pipefail

cd "$(dirname "$0")/.."

if [ -f .env ]; then
    # shellcheck disable=SC1091
    set -a; . ./.env; set +a
fi

target="${1:-}"
case "$target" in
    sepolia)
        : "${ARBITRUM_SEPOLIA_HTTP_URL:?ARBITRUM_SEPOLIA_HTTP_URL must be set}"
        rpc="$ARBITRUM_SEPOLIA_HTTP_URL"
        # Aave V3 PoolAddressesProvider on Arbitrum Sepolia
        # https://aave.com/docs/resources/addresses
        addresses_provider="${AAVE_ADDRESSES_PROVIDER:-0xB25a5D144626a0D488e52AE717A051a2E9997076}"
        ;;
    mainnet)
        : "${ARBITRUM_HTTP_URL:?ARBITRUM_HTTP_URL must be set}"
        rpc="$ARBITRUM_HTTP_URL"
        addresses_provider="${AAVE_ADDRESSES_PROVIDER:-0xa97684ead0e402dC232d5A977953DF7ECBaB3CDb}"
        printf '\033[33mYou are about to deploy to Arbitrum One mainnet. Type DEPLOY to confirm: \033[0m'
        read -r confirm
        if [ "$confirm" != "DEPLOY" ]; then
            echo "aborted"; exit 1
        fi
        ;;
    *)
        echo "usage: $0 {sepolia|mainnet}" >&2
        exit 2
        ;;
esac

: "${PRIVATE_KEY:?PRIVATE_KEY (deployer) must be set in .env}"

export PRIVATE_KEY
export AAVE_ADDRESSES_PROVIDER="$addresses_provider"

cd contracts
forge script script/Deploy.s.sol \
    --rpc-url "$rpc" \
    --broadcast \
    --slow

echo "deployed; copy the address into FLASH_EXECUTOR_ADDRESS in .env"
