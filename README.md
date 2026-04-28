# arbi-bot

A Rust MEV bot that funds atomic DEX arbitrage with Aave V3 flash loans on
**Arbitrum One**. Strategies: pair arbitrage across Uniswap V2/V3 forks plus
triangular/cyclic arbitrage. No sandwich, no JIT, no liquidations.

For the canonical project spec — architecture, scope, math, deployment workflow
— see [`CLAUDE.md`](./CLAUDE.md). This README is the operational quickstart.

## Layout

```
src/                Rust binary (collectors, strategy, executor pipeline)
contracts/          Foundry project for FlashExecutor.sol
data/pools.json     Seed registry of tracked pools
scripts/            deploy / fund / verify-setup helpers
tests/integration/  Anvil-fork integration tests (require ARBITRUM_HTTP_URL)
```

## Prerequisites

- Rust ≥ 1.91 (pinned via `rust-toolchain.toml`)
- Foundry (`forge`, `cast`, `anvil`) — `curl -L https://foundry.paradigm.xyz | bash && foundryup`
- A Chainstack (or equivalent) Arbitrum WSS + HTTPS endpoint

## Setup

```bash
cp .env.example .env
# Fill in ARBITRUM_WSS_URL, ARBITRUM_HTTP_URL, EXECUTOR_PRIVATE_KEY, EXECUTOR_ADDRESS

./scripts/install_contracts.sh   # rehydrate Foundry deps (lib/ is gitignored)
./scripts/verify_setup.sh        # sanity-checks env + RPC + balance
```

## Build, run, test

```bash
# Build
cargo build --release

# Run (testnet, scan-only — the safe default)
cargo run --release -- --mode scan-only

# Run live (mainnet only, after confidence is built)
NETWORK=arbitrum cargo run --release -- --mode live --confirm-live

# Tests
cargo test                                          # unit
cargo test --test pair_arb_fork -- --ignored        # fork (needs ARBITRUM_HTTP_URL)
cargo clippy --all-targets -- -D warnings
cargo fmt --check

# Smart contracts
cd contracts
forge build
forge test -vvv
```

## Operating modes

| Mode | What it does |
|---|---|
| `scan-only` | Detect + simulate, log opportunities. **Sends nothing.** Default on Sepolia. |
| `dry-run` | Same as scan-only, plus appends to `data/dry_run.jsonl`. |
| `live` | Submits transactions. Requires `--confirm-live` AND `NETWORK=arbitrum`. |

## Deployment

See `CLAUDE.md` §17 for the full testnet → mainnet workflow. Short version:

```bash
./scripts/deploy.sh sepolia       # deploy FlashExecutor to Arbitrum Sepolia
./scripts/fund.sh sepolia 0.05    # fund executor with 0.05 ETH
./scripts/deploy.sh mainnet       # only after testnet runs clean for ≥ 1h
```

## Safety rails

- `live` mode refuses to run without `--confirm-live` AND `NETWORK=arbitrum`.
- Profitability gate (CLAUDE.md §9) is enforced before every submission.
- Every live transaction is pre-flight-simulated in REVM. No exceptions.
- Private keys live in `.env` only; `.env` is gitignored.

## License

Personal/educational. Not audited. Use at your own risk on mainnet.
