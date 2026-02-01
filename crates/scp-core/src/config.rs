use serde::Deserialize;
use std::path::Path;
use crate::Result;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub node: NodeConfig,
    pub vault: VaultConfig,
    pub agent: AgentConfig,
    pub economics: EconomicsConfig,
    pub lbo: LboConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NodeConfig {
    pub network: String,
    pub esplora_url: Option<String>,
    pub data_dir: String,
    pub log_level: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct VaultConfig {
    pub min_deposit: u64,
    pub max_utilization: f64,
    pub base_spread: f64,
    pub withdrawal_delay_blocks: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub update_interval_secs: u64,
    pub circuit_breaker_threshold: f64,
    pub auto_adjust_spread: bool,
    pub toxicity_threshold: f64,
    pub use_rss_sentiment: Option<bool>,
    pub rss_url: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EconomicsConfig {
    pub trading_fee_rate: f64,
    pub distribution_interval_secs: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LboConfig {
    pub tokens_per_sat: f64,
    pub min_contribution: u64,
    pub max_contribution: u64,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)
            .map_err(|e| crate::Error::Config(e.to_string()))?;
        Ok(config)
    }
}
