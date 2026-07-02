use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::model::DepError;

/// Top-level configuration read from a TOML file.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub nexus: NexusConfig,
    pub repositories: RepositoryConfig,
}

/// Nexus connection settings.
#[derive(Debug, Clone, Deserialize)]
pub struct NexusConfig {
    pub base_url: String,
    pub username: String,
    pub password: String,
}

/// Repository name mappings for each dependency kind.
#[derive(Debug, Clone, Deserialize)]
pub struct RepositoryConfig {
    /// Maven hosted-releases / hosted-snapshots repository name.
    pub maven: String,
    /// npm hosted repository name.
    pub npm: String,
    /// PyPI hosted repository name.
    pub pypi: String,
    /// Raw hosted repository name (fallback for cargo, conan).
    pub raw: String,
}

impl AppConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).with_context(|| {
            format!(
                "Cannot read config file '{}'. Make sure the file exists.",
                path.display()
            )
        })?;
        let config: AppConfig = toml::from_str(&content).with_context(|| {
            format!(
                "Failed to parse config file '{}'. Check TOML syntax.",
                path.display()
            )
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Validate that required fields are non-empty.
    fn validate(&self) -> Result<()> {
        if self.nexus.base_url.is_empty() {
            return Err(DepError::ConfigError(
                "[nexus] base_url is empty".into(),
            )
            .into());
        }
        if self.nexus.username.is_empty() {
            return Err(DepError::ConfigError(
                "[nexus] username is empty".into(),
            )
            .into());
        }
        if self.nexus.password.is_empty() {
            return Err(DepError::ConfigError(
                "[nexus] password is empty".into(),
            )
            .into());
        }
        if self.repositories.maven.is_empty() {
            return Err(DepError::ConfigError(
                "[repositories] maven is empty".into(),
            )
            .into());
        }
        if self.repositories.npm.is_empty() {
            return Err(DepError::ConfigError(
                "[repositories] npm is empty".into(),
            )
            .into());
        }
        if self.repositories.pypi.is_empty() {
            return Err(DepError::ConfigError(
                "[repositories] pypi is empty".into(),
            )
            .into());
        }
        if self.repositories.raw.is_empty() {
            return Err(DepError::ConfigError(
                "[repositories] raw is empty".into(),
            )
            .into());
        }
        Ok(())
    }
}
