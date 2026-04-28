//! `arbi-bot` — Aave V3 flash-loan arbitrage on Arbitrum One.
//!
//! Entry point: parses CLI args, loads config, initialises tracing, then hands
//! off to [`runtime::run`]. See `CLAUDE.md` for the full architecture.

// Removed in Phase 9 once collectors → strategy → executor are wired end-to-end.
#![allow(dead_code)]

use anyhow::{Context, Result, bail};
use clap::Parser;

mod bindings;
mod collectors;
mod config;
mod executor;
mod math;
mod simulator;
mod strategy;
mod types;
mod util;

use config::{Config, Mode, Network};

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
    config: Option<std::path::PathBuf>,
}

fn main() -> Result<()> {
    // Best-effort .env load. Missing file is fine; missing required vars is not.
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
        run(config).await
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

/// Runtime stub: collectors → strategy → executor pipeline lands in Phase 9.
async fn run(_config: Config) -> Result<()> {
    tracing::warn!(
        target: "boot",
        "runtime pipeline not yet wired; this build is the initial skeleton"
    );
    Ok(())
}
