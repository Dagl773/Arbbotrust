//! `arbi-bot` — Aave V3 flash-loan arbitrage on Arbitrum One.
//!
//! Entry point: parses CLI args, loads config, initialises tracing, then runs
//! the collectors → strategy → executor pipeline. See `CLAUDE.md` for the
//! full architecture.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use arbi_bot::{
    collectors::{Collector, block::BlockCollector, pool_event::PoolEventCollector},
    config::{Config, Mode, Network},
    executor::{ExecutorService, GateParams, LiveSender},
    simulator::revm_fork::{ForkSim, ForkSimConfig},
    strategy::{
        StrategyRunner, pair_arb::PairArbDetector, pool_registry::PoolRegistry,
        triangular::TriangularDetector,
    },
    types, util,
};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "arbi-bot", version, about = "Aave V3 flash-loan arb bot")]
struct Cli {
    /// Operating mode. See CLAUDE.md §10.
    #[arg(long, value_enum, default_value_t = Mode::ScanOnly)]
    mode: Mode,

    /// Required to actually submit transactions in `live` mode.
    #[arg(long)]
    confirm_live: bool,

    /// Optional path to a TOML config file with non-secret defaults.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Path to the pool registry JSON. Defaults to `data/pools.json`.
    #[arg(long, default_value = "data/pools.json")]
    pools: PathBuf,
}

fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();
    util::logging::init().context("failed to initialise tracing subscriber")?;

    let config = Config::load(cli.config.as_deref(), cli.mode, cli.confirm_live)
        .context("failed to load config")?;

    enforce_live_safety(&config, cli.confirm_live)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("arbi-bot")
        .build()
        .context("failed to build tokio runtime")?;

    runtime.block_on(async move {
        tracing::info!(
            target: "boot",
            mode = ?config.mode,
            network = ?config.network,
            "arbi-bot starting"
        );
        run(config, cli.pools).await
    })
}

/// CLAUDE.md §10: live mode requires `--confirm-live` AND `NETWORK=arbitrum`,
/// and refuses to run without `EXECUTOR_PRIVATE_KEY`.
fn enforce_live_safety(config: &Config, confirm_live: bool) -> Result<()> {
    if config.mode != Mode::Live {
        return Ok(());
    }
    if !confirm_live {
        eprintln!("refusing to run live without --confirm-live; exiting");
        std::process::exit(2);
    }
    if config.network != Network::Arbitrum {
        bail!(
            "live mode requires NETWORK=arbitrum (got {:?})",
            config.network
        );
    }
    if config.executor_private_key.is_none() {
        bail!("live mode requires EXECUTOR_PRIVATE_KEY in .env");
    }
    Ok(())
}

async fn run(config: Config, pools_path: PathBuf) -> Result<()> {
    let registry = PoolRegistry::from_json_file(&pools_path)
        .with_context(|| format!("load pool registry from {}", pools_path.display()))?;
    let tracked: Vec<_> = registry.iter().map(|(a, _)| *a).collect();
    tracing::info!(target: "boot", pools = tracked.len(), "loaded pool registry");

    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<types::Event>(1024);
    let (action_tx, action_rx) = tokio::sync::mpsc::channel::<types::Action>(256);

    // Spawn collectors.
    let block_collector: Box<dyn Collector> =
        Box::new(BlockCollector::new(config.endpoints.wss_url.clone()));
    let pool_collector: Box<dyn Collector> = Box::new(PoolEventCollector::new(
        config.endpoints.wss_url.clone(),
        tracked,
    ));
    let block_handle = tokio::spawn({
        let tx = event_tx.clone();
        async move { block_collector.run(tx).await }
    });
    let pool_handle = tokio::spawn({
        let tx = event_tx.clone();
        async move { pool_collector.run(tx).await }
    });
    drop(event_tx); // strategy gets EOF when both collectors finish

    // Spawn strategy.
    let strategy_runner = StrategyRunner::new(
        registry,
        vec![
            Box::new(PairArbDetector::default()),
            Box::new(TriangularDetector::default()),
        ],
    );
    let strategy_handle =
        tokio::spawn(async move { strategy_runner.run(event_rx, action_tx).await });

    // Build simulator + (optional) sender.
    let sim = match ForkSim::new(
        ForkSimConfig::new(config.endpoints.http_url.clone()),
        config.addresses.uniswap_v3_quoter_v2,
    )
    .await
    {
        Ok(s) => Some(Arc::new(s)),
        Err(err) => {
            tracing::warn!(target: "boot", ?err, "simulator unavailable; running without pre-flight");
            None
        }
    };
    let sender = if matches!(config.mode, Mode::Live) {
        let pk = config
            .executor_private_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing EXECUTOR_PRIVATE_KEY in live mode"))?;
        let s = LiveSender::new(&config.endpoints.http_url, pk, config.max_gas_price_gwei).await?;
        Some(Arc::new(s))
    } else {
        None
    };

    let gate = GateParams {
        max_gas_price_wei: u128::from(config.max_gas_price_gwei) * 1_000_000_000,
        ..Default::default()
    };

    let executor_service = ExecutorService {
        mode: config.mode,
        executor_address: config.addresses.flash_executor,
        sim,
        sender,
        gate,
    };
    let executor_handle = tokio::spawn(async move { executor_service.run(action_rx).await });

    // Wait for any task to exit; bot is healthy as long as all are running.
    tokio::select! {
        r = block_handle => log_exit("block collector", r),
        r = pool_handle => log_exit("pool collector", r),
        r = strategy_handle => log_exit("strategy", r),
        r = executor_handle => log_exit("executor", r),
    }
    Ok(())
}

fn log_exit(name: &str, r: Result<Result<()>, tokio::task::JoinError>) {
    match r {
        Ok(Ok(())) => tracing::info!(target: "boot", task = name, "task ended cleanly"),
        Ok(Err(err)) => tracing::error!(target: "boot", task = name, ?err, "task failed"),
        Err(err) => tracing::error!(target: "boot", task = name, ?err, "task join error"),
    }
}
