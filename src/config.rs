use crate::alerts::ThresholdConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_refresh_ms")]
    pub refresh_interval_ms: u64,

    #[serde(default = "default_conn_refresh_ms")]
    pub connection_refresh_ms: u64,

    #[serde(default)]
    pub thresholds: Vec<ThresholdConfig>,
}

fn default_refresh_ms() -> u64 {
    1000
}
fn default_conn_refresh_ms() -> u64 {
    2000
}

impl Default for Config {
    fn default() -> Self {
        Self {
            refresh_interval_ms: default_refresh_ms(),
            connection_refresh_ms: default_conn_refresh_ms(),
            thresholds: Vec::new(),
        }
    }
}

/// Try to load config from (in order):
///   1. ./config.toml
///   2. ~/.network_monitor.toml
/// Falls back to defaults if neither exists or fails to parse.
pub fn load_config() -> Config {
    let candidates: Vec<PathBuf> = vec![
        PathBuf::from("config.toml"),
        dirs_home()
            .map(|h| h.join(".network_monitor.toml"))
            .unwrap_or_default(),
    ];

    for path in candidates {
        if path.exists() {
            if let Ok(contents) = fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str::<Config>(&contents) {
                    return cfg;
                }
            }
        }
    }

    Config::default()
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(PathBuf::from)
}
