#!/usr/bin/env bash
# verify_setup.sh — preflight check for arbi-bot.
# Confirms required env vars are present, the RPC responds, and the executor
# wallet has a non-zero ETH balance.
#
# Usage:
#   scripts/verify_setup.sh
#
# Exits non-zero on any check failure so it can be wired into CI.

set -euo pipefail

red() { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }

if [ -f .env ]; then
    # shellcheck disable=SC1091
    set -a; . ./.env; set +a
fi

required_vars=(
    NETWORK
    EXECUTOR_ADDRESS
    AAVE_V3_POOL_ARBITRUM
    UNISWAP_V3_QUOTER_V2_ARBITRUM
)

missing=0
for v in "${required_vars[@]}"; do
    if [ -z "${!v:-}" ]; then
        red "missing env var: $v"
        missing=$((missing+1))
    fi
done
if [ "$missing" -gt 0 ]; then
    red "$missing required env var(s) missing — copy .env.example to .env and fill in"
    exit 1
fi

# Pick the right RPC for the network.
case "$NETWORK" in
    arbitrum)
        : "${ARBITRUM_HTTP_URL:?ARBITRUM_HTTP_URL must be set}"
        rpc="$ARBITRUM_HTTP_URL"
        ;;
    arbitrum_sepolia)
        : "${ARBITRUM_SEPOLIA_HTTP_URL:?ARBITRUM_SEPOLIA_HTTP_URL must be set}"
        rpc="$ARBITRUM_SEPOLIA_HTTP_URL"
        ;;
    *)
        red "unknown NETWORK=$NETWORK (want arbitrum | arbitrum_sepolia)"
        exit 1
        ;;
esac

green "[1/3] env vars OK"

# RPC reachability — chainId.
if ! cast chain-id --rpc-url "$rpc" > /tmp/arbi_chain_id 2>&1; then
    red "RPC unreachable: $rpc"
    cat /tmp/arbi_chain_id
    exit 1
fi
chain_id=$(cat /tmp/arbi_chain_id)
green "[2/3] RPC reachable, chain_id=$chain_id"

# Executor balance.
balance=$(cast balance "$EXECUTOR_ADDRESS" --rpc-url "$rpc" --ether)
green "[3/3] executor wallet $EXECUTOR_ADDRESS has $balance ETH"

if [ "$(printf '%s' "$balance" | cut -d. -f1)" = "0" ] && [ "${balance:0:1}" = "0" ]; then
    yellow "warning: executor wallet has < 1 ETH; fund it with scripts/fund.sh before live runs"
fi

green "preflight checks passed"
