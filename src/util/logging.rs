//! `tracing` setup. JSON output if `LOG_FORMAT=json`, otherwise human-readable.
//!
//! Filter is read from `LOG_LEVEL`; defaults to `info,arbi_bot=debug`.

use anyhow::Result;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub fn init() -> Result<()> {
    let filter = EnvFilter::try_from_env("LOG_LEVEL")
        .unwrap_or_else(|_| EnvFilter::new("info,arbi_bot=debug"));

    let json = std::env::var("LOG_FORMAT").as_deref() == Ok("json");

    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry
            .with(fmt::layer().json().with_current_span(true))
            .try_init()
            .map_err(|e| anyhow::anyhow!("tracing init failed: {e}"))?;
    } else {
        registry
            .with(fmt::layer().with_target(true))
            .try_init()
            .map_err(|e| anyhow::anyhow!("tracing init failed: {e}"))?;
    }
    Ok(())
}
