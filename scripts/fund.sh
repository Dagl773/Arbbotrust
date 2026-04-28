#!/usr/bin/env bash
# fund.sh — sends ETH from the deployer wallet to the executor contract.
#
# Usage:
#   scripts/fund.sh sepolia 0.05          # send 0.05 ETH on Sepolia
#   scripts/fund.sh mainnet 0.01          # send 0.01 ETH on mainnet (will prompt)

set -euo pipefail

cd "$(dirname "$0")/.."

if [ -f .env ]; then
    # shellcheck disable=SC1091
    set -a; . ./.env; set +a
fi

target="${1:-}"
amount_eth="${2:-}"
if [ -z "$target" ] || [ -z "$amount_eth" ]; then
    echo "usage: $0 {sepolia|mainnet} <amount_eth>" >&2
    exit 2
fi

case "$target" in
    sepolia)
        : "${ARBITRUM_SEPOLIA_HTTP_URL:?ARBITRUM_SEPOLIA_HTTP_URL must be set}"
        rpc="$ARBITRUM_SEPOLIA_HTTP_URL"
        ;;
    mainnet)
        : "${ARBITRUM_HTTP_URL:?ARBITRUM_HTTP_URL must be set}"
        rpc="$ARBITRUM_HTTP_URL"
        printf '\033[33mAbout to send %s ETH on Arbitrum One mainnet. Type FUND to confirm: \033[0m' "$amount_eth"
        read -r confirm
        if [ "$confirm" != "FUND" ]; then
            echo "aborted"; exit 1
        fi
        ;;
    *)
        echo "usage: $0 {sepolia|mainnet} <amount_eth>" >&2
        exit 2
        ;;
esac

: "${PRIVATE_KEY:?PRIVATE_KEY (deployer) must be set in .env}"
: "${FLASH_EXECUTOR_ADDRESS:?FLASH_EXECUTOR_ADDRESS must be set after deploy}"

cast send "$FLASH_EXECUTOR_ADDRESS" \
    --rpc-url "$rpc" \
    --private-key "$PRIVATE_KEY" \
    --value "${amount_eth}ether"

echo "sent $amount_eth ETH to $FLASH_EXECUTOR_ADDRESS"
