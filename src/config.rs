//! Strongly-typed application configuration.
//!
//! This module provides [`AppConfig`], parsing host, port, storage directory,
//! and API key settings from environment maps with strict validation and
//! deterministic unit testability.

use std::collections::HashMap;
use std::env;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;

use crate::domain::ApiKey;
use crate::error::AppError;

/// Default bind IP address (`127.0.0.1`).
const DEFAULT_HOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// Default TCP port (`8085`).
const DEFAULT_PORT: u16 = 8085;

/// Default local filesystem storage directory (`"./data"`).
const DEFAULT_STORAGE_DIR: &str = "./data";

/// Default local development API key (`"dev-secret-key"`).
const DEFAULT_API_KEY: &str = "dev-secret-key";

/// Validated runtime configuration for the `cdd-storage` server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    /// Network interface IP address to bind to.
    pub host: IpAddr,
    /// TCP port to listen on.
    pub port: u16,
    /// Root filesystem directory for local artifact persistence.
    pub storage_dir: PathBuf,
    /// Secret API key required for mutating endpoints (`POST` / `DELETE`).
    pub api_key: ApiKey,
}

impl AppConfig {
    /// Parses [`AppConfig`] from an explicit key-value map of environment variables.
    ///
    /// Recognized keys:
    /// - `HOST`: IP address (defaults to `127.0.0.1`).
    /// - `PORT`: 16-bit unsigned integer port (defaults to `8085`).
    /// - `STORAGE_DIR`: Non-empty directory path (defaults to `./data`).
    /// - `API_KEY`: Non-empty secret token (defaults to `dev-secret-key`).
    ///
    /// # Arguments
    ///
    /// * `vars` - Map of environment variable names to string values.
    ///
    /// # Returns
    ///
    /// Returns `Ok(AppConfig)` when all provided values are valid.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::ConfigurationError`] if `HOST` is not a valid IP address,
    /// `PORT` is not a valid `u16`, `STORAGE_DIR` is empty, or `API_KEY` is empty.
    pub fn from_map<S: std::hash::BuildHasher>(
        vars: &HashMap<String, String, S>,
    ) -> Result<Self, AppError> {
        let host = match vars.get("HOST") {
            Some(raw_host) => raw_host.parse::<IpAddr>().map_err(|e| {
                AppError::ConfigurationError(format!("invalid HOST '{raw_host}': {e}"))
            })?,
            None => DEFAULT_HOST,
        };

        let port = match vars.get("PORT") {
            Some(raw_port) => raw_port.parse::<u16>().map_err(|e| {
                AppError::ConfigurationError(format!("invalid PORT '{raw_port}': {e}"))
            })?,
            None => DEFAULT_PORT,
        };

        let storage_dir_str = vars
            .get("STORAGE_DIR")
            .map_or(DEFAULT_STORAGE_DIR, String::as_str);
        if storage_dir_str.trim().is_empty() {
            return Err(AppError::ConfigurationError(String::from(
                "STORAGE_DIR must not be empty",
            )));
        }

        let raw_api_key = vars.get("API_KEY").map_or(DEFAULT_API_KEY, String::as_str);
        let api_key = ApiKey::try_from(raw_api_key)?;

        Ok(Self {
            host,
            port,
            storage_dir: PathBuf::from(storage_dir_str),
            api_key,
        })
    }

    /// Loads [`AppConfig`] from the current process environment (`std::env::vars`).
    ///
    /// # Returns
    ///
    /// Returns `Ok(AppConfig)` parsed from the process environment variables.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::ConfigurationError`] if any environment variable is invalid.
    pub fn from_env() -> Result<Self, AppError> {
        let vars: HashMap<String, String> = env::vars().collect();
        Self::from_map(&vars)
    }
}

#[cfg(test)]
/// Unit tests for [`AppConfig`].
mod tests {
    use super::*;

    /// Verifies default values when no environment variables are set.
    #[test]
    fn test_config_defaults_and_from_env() -> Result<(), AppError> {
        let empty = HashMap::new();
        let cfg = AppConfig::from_map(&empty)?;
        assert_eq!(cfg.host, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(cfg.port, 8085);
        assert_eq!(cfg.storage_dir, PathBuf::from("./data"));
        assert!(cfg.api_key.verify_bearer("Bearer dev-secret-key"));

        let env_cfg = AppConfig::from_env()?;
        assert_eq!(env_cfg.port, 8085);
        Ok(())
    }

    /// Verifies custom overrides and error handling for invalid configuration values.
    #[test]
    fn test_config_custom_and_invalid_values() -> Result<(), AppError> {
        let mut vars = HashMap::new();
        vars.insert(String::from("HOST"), String::from("0.0.0.0"));
        vars.insert(String::from("PORT"), String::from("9090"));
        vars.insert(String::from("STORAGE_DIR"), String::from("/var/lib/cdd"));
        vars.insert(String::from("API_KEY"), String::from("prod-key-123"));

        let cfg = AppConfig::from_map(&vars)?;
        assert_eq!(cfg.host, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(cfg.port, 9090);
        assert_eq!(cfg.storage_dir, PathBuf::from("/var/lib/cdd"));
        assert!(cfg.api_key.verify_bearer("Bearer prod-key-123"));

        let mut bad_host = vars.clone();
        bad_host.insert(String::from("HOST"), String::from("not-an-ip"));
        assert!(matches!(
            AppConfig::from_map(&bad_host),
            Err(AppError::ConfigurationError(_))
        ));

        let mut bad_port = vars.clone();
        bad_port.insert(String::from("PORT"), String::from("not-a-port"));
        assert!(matches!(
            AppConfig::from_map(&bad_port),
            Err(AppError::ConfigurationError(_))
        ));

        let mut empty_dir = vars.clone();
        empty_dir.insert(String::from("STORAGE_DIR"), String::from("   "));
        assert!(matches!(
            AppConfig::from_map(&empty_dir),
            Err(AppError::ConfigurationError(_))
        ));

        let mut empty_key = vars;
        empty_key.insert(String::from("API_KEY"), String::new());
        assert!(matches!(
            AppConfig::from_map(&empty_key),
            Err(AppError::ConfigurationError(_))
        ));

        Ok(())
    }
}
