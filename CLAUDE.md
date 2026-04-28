# CLAUDE.md

> Source-of-truth context for Claude Code. Read this in full at the start of every session before writing or modifying code. When you change project-level decisions (architecture, dependencies, conventions), update this file in the same commit.

---

## 1. Project Identity

- **Name:** `arbi-bot` (rename in `Cargo.toml` if needed)
- **Owner:** Dagl (Darjan)
- **Goal:** A Rust MEV bot that funds atomic DEX arbitrage with Aave V3 flash loans on **Arbitrum One**.
- **Strategies (locked-in scope):** pair arbitrage across Uniswap V2/V3 forks, plus triangular/cyclic arbitrage. **No sandwich, no liquidations, no JIT** in v1.
- **Stage:** learning + working hobby bot, single developer, small capital. Profit target is "clears gas + Aave fee + a small margin," not "compete with jaredfromsubway."
- **Sister project:** the Python Arbitrum Sepolia scanner. This Rust bot replaces and extends it; do not import Python code, but do mirror its structural conventions (a `scripts/` directory, a scanner-like subsystem, an explicit deploy step).

---

## 2. Mental Model

**One transaction = borrow + arb + repay + profit.** If any step inside the transaction fails, the whole thing reverts and we only lose gas. There is no partial state.

**Off-chain bot** (this Rust binary):
1. Subscribes to new blocks and pool sync/swap events on Arbitrum.
2. Maintains an in-memory registry of pool reserves / sqrt prices.
3. On each new block (or significant event), scans for arbitrage opportunities.
4. Simulates the candidate trade end-to-end in REVM against forked mainnet state.
5. If simulated net profit ≥ threshold, builds calldata and submits the transaction.

**On-chain executor** (`FlashExecutor.sol`):
1. Receives `flashLoanSimple` callback from Aave V3.
2. Executes the pre-encoded swap path through the specified DEX routers/pools.
3. Asserts repayment + minimum profit, otherwise reverts.

**Speed reality check.** This is Arbitrum, not mainnet. We are not in microsecond territory. Single-block latency (≈250ms sequencer block time) is enough to be competitive on small/mid-size opportunities. Don't waste effort on optimizations that buy us microseconds.

---

## 3. Tech Stack (2026)

Pin these. Do not silently drift to alternatives.

| Layer | Choice | Notes |
|---|---|---|
| Language | Rust ≥ 1.85, edition 2024 | MSRV is 1.85 to match alloy. |
| EVM client lib | **alloy-rs ≥ 1.0** | NOT ethers-rs. ethers-rs is deprecated; alloy is 35–60% faster on U256 arithmetic and provides the `sol!` macro for compile-time Solidity bindings. |
| MEV / bundle submission | `alloy-mev` | Replaces `ethers-flashbots`. Used minimally on Arbitrum. |
| EVM simulator | `revm` (latest stable) | Forking simulator for pre-flight profit checks. Sub-50ms simulations are the standard. |
| Async runtime | `tokio` (full features) | |
| Logging | `tracing` + `tracing-subscriber` (env-filter, json) | No `println!` anywhere in production paths. |
| Error handling | `anyhow` at the binary boundary, `thiserror` for library-style errors | Never `.unwrap()` outside tests. `.expect("reason")` only with a real reason. |
| Config | `serde` + `toml` for files, `dotenvy` for `.env` | |
| HTTP fallback | `reqwest` (rustls) | |
| Smart contracts | Solidity ≥ 0.8.24, **Foundry** (`forge`, `cast`, `anvil`) | |
| Static analysis | `clippy` (clean, `-D warnings`), `rustfmt`, optional `slither` for Solidity | |

**Alloy crates we will pull in:**
`alloy` (meta-crate with `full` feature), `alloy-sol-types`, `alloy-mev`, plus `alloy-provider` with `ws` and `ipc` transports.

---

## 4. Infrastructure

| Component | Choice | Detail |
|---|---|---|
| Compute | **AWS EC2, us-east-1 (N. Virginia)** | Geographically close to many Arbitrum sequencer endpoints. Start with `t3.medium` for dev, upgrade to `t3.large` or `c7g.large` for prod (~$60–100/mo on-demand). |
| OS | Ubuntu 24.04 LTS | |
| Disk | 100 GB gp3 SSD | REVM forks and event caches grow. |
| RPC | **Chainstack Arbitrum One ($45/mo plan)** | WebSocket for subscriptions, HTTPS for fallback. **Chainstack at this tier does not provide private sequencer submission** — that's the $99 Trader Node tier. For pair/triangular arb on Arbitrum that is fine; see §13. |
| Wallet | Dedicated EOA, generated fresh, **never reused from any other project** | Funded with ARB-ETH for gas + a small USDC float for the contract to absorb non-loan dust. |
| Secrets | `.env` file, never committed; `EXECUTOR_PRIVATE_KEY` lives only here | |
| Monitoring | Stage 1: structured logs to file + journalctl. Stage 2 (later): Prometheus + Grafana. | |

---

## 5. Architecture

Artemis-inspired three-stage pipeline, decoupled via `tokio::mpsc` channels:

```
 ┌─────────────────┐       ┌───────────────┐       ┌──────────────────┐
 │   Collectors    │ ───▶  │   Strategy    │ ───▶  │    Executor      │
 │ (block, events) │       │ (registry +   │       │ (sim, sign, send)│
 │                 │       │  detection +  │       │                  │
 │                 │       │  optimisation)│       │                  │
 └─────────────────┘       └───────────────┘       └──────────────────┘
        ▲                          │                       │
        │                          ▼                       ▼
   Chainstack WSS         REVM (forking)             Chainstack HTTP
                                                    (eth_sendRawTransaction)
```

**Collectors** turn external events into our internal `Event` enum. Two collectors at v1: a `BlockCollector` for `newHeads`, and a `PoolEventCollector` filtering Sync (V2) and Swap (V3) logs from registered pools.

**Strategy** owns the `PoolRegistry` (in-memory state of all tracked pools), the opportunity detectors (`pair_arb`, `triangular`), and the optimal-input solver. It emits `Action::Execute(Opportunity)` to the executor. It is single-threaded and synchronous internally — the channel is the concurrency boundary.

**Executor** does final REVM simulation, signs the transaction, and submits. On failure it logs and drops; on success it records to the trade log.

---

## 6. Directory Structure

Single binary crate (no workspace yet — add one only when a second binary appears).

```
arbi-bot/
├── Cargo.toml
├── CLAUDE.md                          ← this file
├── README.md                          ← human-facing quickstart
├── .env.example
├── .gitignore                         ← MUST exclude .env, target/, data/cache/
├── rust-toolchain.toml                ← pin to 1.85
├── src/
│   ├── main.rs                        ← arg parsing, runtime bootstrap
│   ├── config.rs                      ← typed config from .env + TOML
│   ├── types.rs                       ← Event, Action, Opportunity enums
│   ├── collectors/
│   │   ├── mod.rs                     ← Collector trait
│   │   ├── block.rs
│   │   └── pool_event.rs
│   ├── strategy/
│   │   ├── mod.rs                     ← Strategy trait + dispatcher
│   │   ├── pool_registry.rs           ← in-memory pool state, tracked-pool list
│   │   ├── pair_arb.rs                ← V2/V3 cross-DEX detector
│   │   ├── triangular.rs              ← cyclic arb detector
│   │   └── path_finder.rs             ← graph search, depth ≤ 3 in v1
│   ├── math/
│   │   ├── mod.rs
│   │   ├── uniswap_v2.rs              ← closed-form get_amount_out / in
│   │   ├── uniswap_v3.rs              ← REVM-backed quoter wrapper (v1)
│   │   └── optimal.rs                 ← V2-V2 closed-form optimal input
│   ├── simulator/
│   │   ├── mod.rs
│   │   └── revm_fork.rs               ← forking REVM with cached state
│   ├── executor/
│   │   ├── mod.rs
│   │   ├── builder.rs                 ← calldata construction
│   │   └── sender.rs                  ← signing + RPC submission
│   ├── bindings/
│   │   ├── mod.rs
│   │   ├── erc20.rs                   ← sol! macros, one file per contract family
│   │   ├── uniswap_v2.rs
│   │   ├── uniswap_v3.rs
│   │   ├── aave_v3.rs
│   │   └── flash_executor.rs          ← bindings for our contract
│   └── util/
│       ├── mod.rs
│       └── logging.rs
├── contracts/                         ← independent Foundry project
│   ├── foundry.toml
│   ├── remappings.txt
│   ├── src/
│   │   └── FlashExecutor.sol
│   ├── script/
│   │   └── Deploy.s.sol
│   └── test/
│       └── FlashExecutor.t.sol
├── data/
│   ├── pools.json                     ← cached tracked-pool registry (committed)
│   └── cache/                         ← runtime caches (NOT committed)
├── scripts/
│   ├── deploy.sh                      ← wraps forge script, sanity-checks env
│   ├── fund.sh                        ← sends initial ETH/USDC to executor
│   └── verify_setup.sh                ← preflight: checks .env, RPC, balance
└── tests/
    └── integration/
        └── pair_arb_fork.rs           ← Anvil-fork integration tests
```

---

## 7. Smart Contract: `FlashExecutor.sol`

**Decision rationale.** We deploy our own minimal contract using Aave V3's `flashLoanSimple` (single-asset). Flashbots' generic flash-loan contract is Ethereum-mainnet only. A custom contract gives us lower per-trade gas (pre-approved routers, no abstract router hops) and is a tractable ~250-line learning artifact.

**Interface contract:**

```solidity
function executeArbitrage(
    address asset,            // token to flash-borrow (usually USDC.e or WETH)
    uint256 amount,           // flash-loan size
    bytes   calldata path,    // ABI-encoded ArbPath struct
    uint256 minProfit         // revert if final balance < amount + premium + minProfit
) external onlyOwner;

function executeOperation(   // Aave V3 callback
    address asset,
    uint256 amount,
    uint256 premium,
    address initiator,
    bytes   calldata params
) external returns (bool);

function withdraw(address token, uint256 amount) external onlyOwner;
function rescueETH() external onlyOwner;
```

**Implementation requirements:**

- Inherits `FlashLoanSimpleReceiverBase` from `aave-v3-core`.
- `executeOperation` decodes `params` into an `ArbPath` describing each hop: `(dexKind, poolAddress, tokenIn, tokenOut, fee)`. `dexKind` is an enum: `UniV2`, `UniV3`, `CamelotV2`.
- For UniV3 hops, call the pool's `swap()` directly and implement `uniswapV3SwapCallback`. Do **not** route through `SwapRouter`.
- Pre-approves the Aave Pool and each known router for `type(uint256).max` in the constructor.
- `nonReentrant` on every external function (use OpenZeppelin's `ReentrancyGuard`).
- `onlyOwner` is a single EOA in v1; document the multisig migration path in code comments.
- Final assertion: `IERC20(asset).balanceOf(address(this)) >= amount + premium + minProfit` before returning `true`.
- Emits a single event `ArbExecuted(uint256 grossOut, uint256 fee, uint256 profit)` for off-chain reconciliation.
- Target ≤ 300 lines, single file.

---

## 8. Math & Algorithms

### Uniswap V2 (and V2 forks: SushiSwap, Camelot V2 stable curves excluded for v1)

Closed-form, no RPC call once reserves are cached:

```
amount_out = (amount_in * (10000 - fee_bps) * reserve_out)
           / (reserve_in * 10000 + amount_in * (10000 - fee_bps))
```

Default `fee_bps = 30`. **Camelot V2 has dynamic fees** — read `pair.stableFee()` and `pair.volatileFee()` per pool at registry-load time and refresh on a slow timer.

### Optimal input for V2 → V2 atomic arb

Closed-form (derived from the constant-product formula). Reference implementation: Flashbots `simple-blind-arbitrage`. Live in `src/math/optimal.rs`. Do **not** use iterative search for V2-V2; the closed form is faster and exact.

### Uniswap V3

Tick-based math is painful off-chain (tick bitmap + crossing logic). **v1 strategy: REVM-backed quoter.** We fork at the latest block and call `Quoter.quoteExactInputSingle()` inside REVM. Slower per-quote (~5–20ms) but correct.

Move to native off-chain V3 math only if the simulator becomes the bottleneck on a profiled run. Don't preoptimize.

### Triangular / cyclic arb

- Build a directed multigraph: nodes = tokens, edges = pools (with direction implied by which-token-in/out).
- Restrict to cycles of length 3 in v1 (token A → B → C → A).
- For each newly updated pool, evaluate cycles passing through that pool (avoid full graph rescan per event).
- Use log-space rate sums to detect candidates (`log(r1) + log(r2) + log(r3) > 0` ⇒ candidate), then run exact arithmetic on candidates only.
- Filter pools by minimum reserve value (USD-equivalent ≥ $50K) to avoid honeypots and dust-quote noise.

---

## 9. Profitability Gate

A trade is submitted only when **all** of the following hold, in this order:

1. Off-chain math says gross profit > 0.
2. Optimal input is computed and is below 50% of pool reserves (avoid catastrophic price impact).
3. REVM simulation against the latest forked block returns success and a measured `profit_after_repay`.
4. `profit_after_repay` ≥ `gas_cost_estimate + flash_loan_premium + safety_margin`.
5. Current gas price ≤ `MAX_GAS_PRICE_GWEI`.

`safety_margin` defaults to **$0.50** (configurable). Tune up in early days, down with confidence.

`gas_cost_estimate` on Arbitrum = L1 calldata cost + L2 execution cost. Use `eth_estimateGas` against the simulated tx, then add a 20% buffer.

---

## 10. Operating Modes

The binary takes a `--mode` flag:

| Mode | What it does |
|---|---|
| `scan-only` | Subscribes, detects, simulates, **logs** would-be profit. Sends nothing. **Default** when `NETWORK=arbitrum_sepolia`. |
| `dry-run` | Same as scan-only, plus writes simulated transactions to `data/dry_run.jsonl` for offline analysis. |
| `live` | Actually submits transactions. Requires `--confirm-live` AND `NETWORK=arbitrum`. Refuses to run if `EXECUTOR_PRIVATE_KEY` isn't set. |

A live-mode launch with no `--confirm-live` flag prints a one-line warning and exits 2.

---

## 11. Environment Variables (`.env`)

```bash
# === RPC: Chainstack Arbitrum One ===
ARBITRUM_WSS_URL=wss://arbitrum-mainnet.core.chainstack.com/<KEY>
ARBITRUM_HTTP_URL=https://arbitrum-mainnet.core.chainstack.com/<KEY>

# === Testnet (public RPC is fine here) ===
ARBITRUM_SEPOLIA_WSS_URL=wss://sepolia-rollup.arbitrum.io/feed
ARBITRUM_SEPOLIA_HTTP_URL=https://sepolia-rollup.arbitrum.io/rpc

# === Wallet (NEVER commit, NEVER reuse a key from elsewhere) ===
EXECUTOR_PRIVATE_KEY=0x...
EXECUTOR_ADDRESS=0x...

# === Contract addresses ===
FLASH_EXECUTOR_ADDRESS=                          # filled after first deploy
AAVE_V3_POOL_ARBITRUM=0x794a61358D6845594F94dc1DB02A252b5b4814aD
UNISWAP_V3_FACTORY_ARBITRUM=0x1F98431c8aD98523631AE4a59f267346ea31F984
UNISWAP_V3_QUOTER_V2_ARBITRUM=0x61fFE014bA17989E743c5F6cB21bF9697530B21e
SUSHI_V2_FACTORY_ARBITRUM=0xc35DADB65012eC5796536bD9864eD8773aBc74C4
CAMELOT_V2_FACTORY_ARBITRUM=0x6EcCab422D763aC031210895C81787E87B43A652

# === Runtime config ===
NETWORK=arbitrum_sepolia                         # arbitrum_sepolia | arbitrum
MIN_PROFIT_USD=0.50
MAX_GAS_PRICE_GWEI=5
SAFETY_MARGIN_ETH=0.0002
LOG_LEVEL=info,arbi_bot=debug
```

**Verify every address against an Arbiscan lookup before first use.** Hardcoded addresses rot; treat them as needing a quick sanity check at the start of any session that touches them.

---

## 12. Build, Run, Test

```bash
# Build
cargo build --release

# Run (testnet, scan-only, default)
cargo run --release -- --mode scan-only

# Run live (mainnet only, after confidence built)
NETWORK=arbitrum cargo run --release -- --mode live --confirm-live

# Tests
cargo test                                       # unit tests
cargo test --test integration -- --ignored       # fork tests (require ARBITRUM_HTTP_URL)
cargo clippy --all-targets -- -D warnings        # lint, must be clean
cargo fmt --check                                # formatting

# Contracts
cd contracts
forge build
forge test -vvv
forge script script/Deploy.s.sol \
  --rpc-url $ARBITRUM_SEPOLIA_HTTP_URL \
  --broadcast --verify
```

---

## 13. Arbitrum-Specific Notes

These differ meaningfully from Ethereum mainnet — do not reuse mainnet mental models blindly.

- **Single sequencer.** Arbitrum One has one centralized sequencer. There is no public mempool to be frontrun in the way mainnet has. This is good for us: a flash-loan arb tx submitted via standard `eth_sendRawTransaction` to the sequencer will not be sandwiched.
- **No Flashbots-style PBS yet.** Don't waste effort on bundle submission. Just send normal transactions.
- **Block times ≈ 250 ms.** The sequencer batches frequently. We have effectively one shot per block at any opportunity created in that block.
- **Gas is L1 calldata + L2 execution.** Calldata dominates for short trades. Keep our `executeArbitrage` calldata tight; don't pad path encoding.
- **Reorgs are rare but exist** during sequencer outages. Don't assume `n` block confirmations means anything; rely on `eth_getTransactionReceipt` polling.
- **Sequencer outages happen.** Have a watchdog that detects RPC silence > 30s and flips the bot into degraded mode (logs only, no submission).

---

## 14. Workflow Rules — Always

1. **Read this file fully** before writing or modifying code in this repo.
2. **Run `cargo check` after every meaningful edit.** Catch compile errors early.
3. **Pre-flight every live transaction in REVM.** No exceptions.
4. **Schema-validate config on startup.** Crash loudly with a clear message if `.env` is malformed; never silently default a critical value.
5. **Log structured, not stringly-typed.** `tracing::info!(target: "strategy", pool = %addr, profit_usd = profit, "opportunity detected")`.
6. **Test on Arbitrum Sepolia before mainnet.** Even when the change feels trivial.
7. **Update `data/pools.json` checked-in copy** when registry logic changes; treat it as a fixture.
8. **Commit smart-contract changes and Rust binding regeneration in the same commit.** They must move together.
9. **Cite every numeric magic constant** with a comment pointing to its source (Aave docs, fee tier table, etc.).

## 15. Workflow Rules — Never

1. ❌ **Never use `ethers-rs`.** It is deprecated. Alloy only.
2. ❌ **Never use HTTP for the event stream.** WebSocket only. HTTP is fine for one-shot calls (`eth_call`, `eth_estimateGas`).
3. ❌ **Never trust off-chain Uniswap V3 math without REVM verification.** Tick crossings will burn you.
4. ❌ **Never skip pre-flight simulation** on a path that will spend gas.
5. ❌ **Never put a private key in code, in a config file, or in a commit.** `.env` only, gitignored.
6. ❌ **Never run two bot instances against the same wallet.** Nonce collisions waste gas.
7. ❌ **Never `.unwrap()` outside tests.** Use `?` with proper error context, or `.expect("reason that explains the invariant")`.
8. ❌ **Never add sandwich, JIT, or liquidation strategies** without an explicit scope-change conversation. v1 scope is locked.
9. ❌ **Never silently retry RPC calls forever.** Bound retries (3 attempts, exponential backoff), then surface the error.
10. ❌ **Never bypass the profitability gate** (§9) for a "looks obviously profitable" trade. Trust the gate.

---

## 16. Testing Strategy

| Layer | What | Tool |
|---|---|---|
| Math purity | Golden tests for V2 `get_amount_out` against known on-chain quotes | `cargo test math` |
| Optimal-input | V2-V2 closed-form vs. iterative search agreement on 1000 random fixtures | `cargo test optimal` |
| Strategy | Inject fabricated `PoolUpdate` events, assert `Action` emitted | `cargo test strategy` |
| Integration | Anvil-fork Arbitrum at a recent block, run pair-arb against historical state with a known opportunity | `cargo test --test integration -- --ignored` |
| Contract | `FlashExecutor.t.sol` happy path, revert paths, reentrancy attempt, owner gate, balance checks | `forge test` |
| Shadow run | `scan-only` on mainnet for 24h+, compare logged opportunities against Arbiscan | manual |

---

## 17. Deployment Workflow (Testnet → Mainnet)

1. Deploy `FlashExecutor` to **Arbitrum Sepolia** via `forge script`.
2. Fund the executor wallet with Sepolia ETH from the official faucet (note: this faucet is harder to extract from than mainnet Sepolia; the Python bot was blocked here previously — budget time for it).
3. Run bot in `scan-only` mode against testnet for ≥ 1 hour. Confirm the opportunity log is sane (non-zero, non-flooded, prices roughly match Arbiscan).
4. Run a forced-opportunity integration test against an Anvil-forked mainnet snapshot.
5. Audit the contract: `forge inspect FlashExecutor abi`, then run `slither contracts/src/FlashExecutor.sol` if installed.
6. Deploy to **Arbitrum One mainnet**. Save the deployment block to `data/deployment.json`.
7. Fund the executor wallet with $50 in ARB-ETH.
8. Run `scan-only` on mainnet for ≥ 24 hours. Open Arbiscan, randomly verify a few logged opportunities looked real at that block.
9. Switch to `live` mode with `MIN_PROFIT_USD=2.00` (high threshold initially, conservative).
10. After ≥ 10 successful trades and no reverts, lower the threshold incrementally toward the floor we can support.

---

## 18. Bootstrapping — What Claude Code Does First

When opening this repo (or starting a fresh session):

1. `cat CLAUDE.md` — re-read in full.
2. `cat README.md` if present.
3. `git status && git log -5 --oneline` — current branch state.
4. `cargo metadata --format-version 1 | jq '.packages[].name'` — confirm crates.
5. `cargo check` — confirm build state without compiling fully.
6. `ls -la data/ scripts/` — note what's checked in.
7. Open `src/main.rs` and trace the runtime entry path before proposing any change.
8. **Then** ask the user what they want to work on.

Skipping any of these for a "small change" routinely surfaces a stale assumption five tool calls later. Don't.

---

## 19. References (use these, don't guess)

- Alloy book: https://alloy.rs
- alloy-mev docs: https://docs.rs/alloy-mev
- REVM book: https://github.com/bluealloy/revm
- Aave V3 flash loans: https://aave.com/docs/aave-v3/guides/flash-loans
- Aave V3 Pool address registry: https://aave.com/docs/resources/addresses
- Uniswap V3 SDK math (TS, algorithmically authoritative): https://github.com/Uniswap/v3-sdk
- Artemis (architecture inspiration): https://github.com/paradigmxyz/artemis
- Flashbots `simple-blind-arbitrage` (V2 closed-form optimal input): https://github.com/flashbots/simple-blind-arbitrage
- Camelot V2 docs (dynamic fees, stable pools): https://docs.camelot.exchange

When something on the network has changed since this file was last edited (addresses, fee parameters, deprecated endpoints), trust the source above and update this file in the same PR.

---

## 20. Deferred Decisions

These are intentionally not solved yet. Don't solve them speculatively.

- **Curve / Balancer integration** — defer until V2/V3 paths are proven net-profitable for ≥ 1 month.
- **Multisig ownership** for `FlashExecutor` — single EOA in v1; revisit when contract holds > $1K of accumulated profit.
- **Postgres trade log** — start with append-only JSONL (`data/trades.jsonl`); migrate when querying becomes painful.
- **Native off-chain V3 tick math** — keep REVM-backed v1; revisit when profiling shows the simulator as the bottleneck.
- **Path length > 3** for triangular arb — keep depth ≤ 3 until v1 is profitable.
- **Cross-chain expansion to Base** — keep Arbitrum-only until v1 is stable; Base port is mostly RPC + address swaps if and when.

---

*Last updated: when this file was first written. Update the date here and the relevant section in the same commit whenever the project's truth changes.*
