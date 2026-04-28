//! Typed config loaded from `.env` (+ optional TOML overrides).
//!
//! CLAUDE.md §14.4: schema-validate on startup. Crash loudly on malformed
//! input; never silently default a critical value.

use std::path::Path;
use std::str::FromStr;

use alloy::primitives::Address;
use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// Detect + simulate, log opportunities. Sends nothing.
    ScanOnly,
    /// Same as scan-only, plus appends to `data/dry_run.jsonl`.
    DryRun,
    /// Submits transactions. Requires `--confirm-live` AND `NETWORK=arbitrum`.
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Network {
    Arbitrum,
    ArbitrumSepolia,
}

impl FromStr for Network {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "arbitrum" | "arbitrum_one" => Ok(Self::Arbitrum),
            "arbitrum_sepolia" | "sepolia" => Ok(Self::ArbitrumSepolia),
            other => bail!("unknown NETWORK={other:?}; want arbitrum | arbitrum_sepolia"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub wss_url: String,
    pub http_url: String,
}

#[derive(Debug, Clone)]
pub struct Addresses {
    pub aave_v3_pool: Address,
    pub uniswap_v3_factory: Address,
    pub uniswap_v3_quoter_v2: Address,
    pub sushi_v2_factory: Address,
    pub camelot_v2_factory: Address,
    /// Filled in after first `forge script` deploy.
    pub flash_executor: Option<Address>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub mode: Mode,
    pub network: Network,
    pub endpoints: Endpoints,
    pub addresses: Addresses,
    /// Hex string with `0x` prefix; held as `String` so we don't pin alloy's
    /// signer types into config.
    pub executor_private_key: Option<String>,
    pub executor_address: Option<Address>,
    /// CLAUDE.md §9 thresholds.
    pub min_profit_usd: f64,
    pub max_gas_price_gwei: u64,
    pub safety_margin_eth: f64,
}

impl Config {
    pub fn load(_toml_path: Option<&Path>, mode: Mode, _confirm_live: bool) -> Result<Self> {
        let network = required_env("NETWORK")?.parse::<Network>()?;

        let endpoints = match network {
            Network::Arbitrum => Endpoints {
                wss_url: required_env("ARBITRUM_WSS_URL")?,
                http_url: required_env("ARBITRUM_HTTP_URL")?,
            },
            Network::ArbitrumSepolia => Endpoints {
                wss_url: required_env("ARBITRUM_SEPOLIA_WSS_URL")?,
                http_url: required_env("ARBITRUM_SEPOLIA_HTTP_URL")?,
            },
        };

        let addresses = Addresses {
            aave_v3_pool: parse_addr_env("AAVE_V3_POOL_ARBITRUM")?,
            uniswap_v3_factory: parse_addr_env("UNISWAP_V3_FACTORY_ARBITRUM")?,
            uniswap_v3_quoter_v2: parse_addr_env("UNISWAP_V3_QUOTER_V2_ARBITRUM")?,
            sushi_v2_factory: parse_addr_env("SUSHI_V2_FACTORY_ARBITRUM")?,
            camelot_v2_factory: parse_addr_env("CAMELOT_V2_FACTORY_ARBITRUM")?,
            flash_executor: optional_addr_env("FLASH_EXECUTOR_ADDRESS")?,
        };

        let executor_private_key = optional_env("EXECUTOR_PRIVATE_KEY").filter(|s| {
            !s.is_empty()
                && s != "0x0000000000000000000000000000000000000000000000000000000000000000"
        });
        let executor_address = optional_addr_env("EXECUTOR_ADDRESS")?;

        let min_profit_usd = required_env("MIN_PROFIT_USD")?
            .parse::<f64>()
            .context("MIN_PROFIT_USD must be a float")?;
        let max_gas_price_gwei = required_env("MAX_GAS_PRICE_GWEI")?
            .parse::<u64>()
            .context("MAX_GAS_PRICE_GWEI must be an integer")?;
        let safety_margin_eth = required_env("SAFETY_MARGIN_ETH")?
            .parse::<f64>()
            .context("SAFETY_MARGIN_ETH must be a float")?;

        if min_profit_usd < 0.0 {
            bail!("MIN_PROFIT_USD must be ≥ 0");
        }
        if max_gas_price_gwei == 0 {
            bail!("MAX_GAS_PRICE_GWEI must be > 0");
        }

        Ok(Self {
            mode,
            network,
            endpoints,
            addresses,
            executor_private_key,
            executor_address,
            min_profit_usd,
            max_gas_price_gwei,
            safety_margin_eth,
        })
    }
}

fn required_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var {key}"))
}

fn optional_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

fn parse_addr_env(key: &str) -> Result<Address> {
    let raw = required_env(key)?;
    raw.parse::<Address>()
        .with_context(|| format!("env var {key} is not a valid address: {raw:?}"))
}

fn optional_addr_env(key: &str) -> Result<Option<Address>> {
    match optional_env(key) {
        None => Ok(None),
        Some(raw) => raw
            .parse::<Address>()
            .map(Some)
            .with_context(|| format!("env var {key} is not a valid address: {raw:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_parse_roundtrip() {
        assert_eq!(Network::from_str("arbitrum").unwrap(), Network::Arbitrum);
        assert_eq!(
            Network::from_str("arbitrum_sepolia").unwrap(),
            Network::ArbitrumSepolia
        );
        assert!(Network::from_str("ethereum").is_err());
    }
}
