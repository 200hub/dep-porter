use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::model::DepError;

/// 从TOML文件读取的顶级配置。
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub nexus: NexusConfig,
    pub repositories: RepositoryConfig,
}

/// Nexus连接设置。
#[derive(Debug, Clone, Deserialize)]
pub struct NexusConfig {
    pub base_url: String,
    pub username: String,
    pub password: String,
}

/// 每种依赖类型的仓库名称映射。
#[derive(Debug, Clone, Deserialize)]
pub struct RepositoryConfig {
    /// Maven托管发布仓库名称。
    pub maven: String,
    /// Maven托管快照仓库名称（可选，回退到maven）。
    pub maven_snapshots: Option<String>,
    /// npm托管仓库名称。
    pub npm: String,
    /// PyPI托管仓库名称。
    pub pypi: String,
    /// Cargo托管仓库名称（可选，回退到raw）。
    pub cargo: Option<String>,
    /// 原始托管仓库名称（cargo、conan的回退选项）。
    pub raw: String,
}

impl AppConfig {
    /// 从TOML文件加载配置。
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("无法读取配置文件'{}'。请确保文件存在。", path.display()))?;
        let config: AppConfig = toml::from_str(&content)
            .with_context(|| format!("解析配置文件'{}'失败。请检查TOML语法。", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    /// 验证必填字段是否非空。
    pub fn validate(&self) -> Result<()> {
        if self.nexus.base_url.is_empty() {
            return Err(DepError::ConfigError("[nexus] base_url is empty".into()).into());
        }
        if self.nexus.username.is_empty() {
            return Err(DepError::ConfigError("[nexus] username is empty".into()).into());
        }
        if self.nexus.password.is_empty() {
            return Err(DepError::ConfigError("[nexus] password is empty".into()).into());
        }
        if self.repositories.maven.is_empty() {
            return Err(DepError::ConfigError("[repositories] maven is empty".into()).into());
        }
        if self.repositories.npm.is_empty() {
            return Err(DepError::ConfigError("[repositories] npm is empty".into()).into());
        }
        if self.repositories.pypi.is_empty() {
            return Err(DepError::ConfigError("[repositories] pypi is empty".into()).into());
        }
        if self.repositories.raw.is_empty() {
            return Err(DepError::ConfigError("[repositories] raw is empty".into()).into());
        }
        Ok(())
    }
}
