use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Supported dependency kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DepKind {
    Maven,
    Npm,
    Pypi,
    Cargo,
    Conan,
}

impl DepKind {
    /// Returns the string representation used in CLI and directory names.
    pub fn as_str(&self) -> &'static str {
        match self {
            DepKind::Maven => "maven",
            DepKind::Npm => "npm",
            DepKind::Pypi => "pypi",
            DepKind::Cargo => "cargo",
            DepKind::Conan => "conan",
        }
    }
}

impl fmt::Display for DepKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DepKind {
    type Err = DepError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "maven" => Ok(DepKind::Maven),
            "npm" => Ok(DepKind::Npm),
            "pypi" | "pip" | "python" => Ok(DepKind::Pypi),
            "cargo" | "rust" => Ok(DepKind::Cargo),
            "conan" | "cpp" | "c++" => Ok(DepKind::Conan),
            other => Err(DepError::UnsupportedKind(other.to_string())),
        }
    }
}

/// A parsed Maven coordinate (group:artifact).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoordinate {
    pub group_id: String,
    pub artifact_id: String,
}

impl MavenCoordinate {
    /// Parse a Maven coordinate string like `org.apache.commons:commons-lang3`.
    pub fn parse(s: &str) -> Result<Self, DepError> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(DepError::InvalidMavenCoord(s.to_string()));
        }
        Ok(MavenCoordinate {
            group_id: parts[0].to_string(),
            artifact_id: parts[1].to_string(),
        })
    }

    /// Returns the directory path segment (group dots replaced with `/`).
    pub fn group_path(&self) -> String {
        self.group_id.replace('.', "/")
    }
}

impl fmt::Display for MavenCoordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.group_id, self.artifact_id)
    }
}

/// A fully-resolved dependency specification.
#[derive(Debug, Clone)]
pub struct DepSpec {
    pub kind: DepKind,
    pub name: String,
    pub version: String,
}

impl DepSpec {
    pub fn new(kind: DepKind, name: String, version: String) -> Self {
        Self { kind, name, version }
    }
}

/// Errors specific to dependency operations.
#[derive(Debug, Error)]
pub enum DepError {
    #[error("Unsupported dependency kind: {0}")]
    UnsupportedKind(String),

    #[error(
        "Invalid Maven coordinate '{0}'. Expected format: groupId:artifactId"
    )]
    InvalidMavenCoord(String),

    #[error("Download directory not found: {0}")]
    DownloadDirNotFound(String),

    #[error("Download directory is empty: {0}")]
    DownloadDirEmpty(String),

    #[error("Docker is not installed or not in PATH")]
    DockerNotFound,

    #[error("Docker image '{0}' not found. Build it first: docker build -f Dockerfile.downloader -t {0} .")]
    DockerImageNotFound(String),

    #[error("Docker command failed: {0}")]
    DockerCommandFailed(String),

    #[error("Nexus upload failed for {url}: HTTP {status}")]
    NexusUploadFailed { url: String, status: u16 },

    #[error("Configuration error: {0}")]
    ConfigError(String),
}
