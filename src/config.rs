/*
 * @Descripttion: js
 * @Version: 1.0
 * @Author: Yulin
 * @Date: 2024-11-20 15:17:09
 * @LastEditors: Yulin
 * @LastEditTime: 2024-11-21 10:39:19
 */
//! Configuration management for the relayer.
//! Handles loading and parsing of configuration from TOML files.

use anyhow::{Error, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::Path;

/// Configuration structure for the relayer
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RelayerConfig {
    pub l1_url: String,
    pub l2_url: String,
    pub watched_account: String,
    pub wallet_path: String,
    pub l1_program_id: String,
    pub l2_program_id: String,
    pub nonce_account: String,
}

impl RelayerConfig {
    /// Loads configuration from a TOML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config_path = path.as_ref();
        if !config_path.exists() {
            return Err(Error::msg(format!(
                "Configuration file not found: {}",
                config_path.display()
            )));
        }

        let settings = config::Config::builder()
            .add_source(config::File::with_name(config_path.to_str().unwrap()))
            .build()?;

        let mut config: RelayerConfig = settings.try_deserialize()?;

        if config.wallet_path.starts_with('~') {
            let home = env::var("HOME")
                .map_err(|_| Error::msg("Failed to get HOME environment variable"))?;
            config.wallet_path = config.wallet_path.replace('~', &home);
        }

        Ok(config)
    }
}
