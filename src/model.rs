use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 支持的依赖类型。
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
    /// 返回CLI和目录名称中使用的字符串表示。
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

/// 解析后的Maven坐标（group:artifact）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoordinate {
    pub group_id: String,
    pub artifact_id: String,
}

impl MavenCoordinate {
    /// 解析Maven坐标字符串，如`org.apache.commons:commons-lang3`。
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

    /// 返回目录路径段（组点替换为`/`）。
    pub fn group_path(&self) -> String {
        self.group_id.replace('.', "/")
    }
}

impl fmt::Display for MavenCoordinate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.group_id, self.artifact_id)
    }
}

/// 完全解析的依赖规范。
#[derive(Debug, Clone)]
pub struct DepSpec {
    pub kind: DepKind,
    pub name: String,
    pub version: String,
}

impl DepSpec {
    pub fn new(kind: DepKind, name: String, version: String) -> Self {
        Self {
            kind,
            name,
            version,
        }
    }
}

/// 依赖操作特定的错误。
#[derive(Debug, Error)]
pub enum DepError {
    #[error("不支持的依赖类型: {0}")]
    UnsupportedKind(String),

    #[error("无效的Maven坐标'{0}'。预期格式: groupId:artifactId")]
    InvalidMavenCoord(String),

    #[error("未找到下载目录: {0}")]
    DownloadDirNotFound(String),

    #[error("下载目录为空: {0}")]
    DownloadDirEmpty(String),

    #[error("Docker未安装或不在PATH中")]
    DockerNotFound,

    #[error("未找到Docker镜像'{0}'。请先构建: docker build -f Dockerfile.downloader -t {0} .")]
    DockerImageNotFound(String),

    #[error("Docker命令失败: {0}")]
    DockerCommandFailed(String),

    #[error("Nexus上传失败 {url}: HTTP {status}\nNexus响应: {details}")]
    NexusUploadFailed {
        url: String,
        status: u16,
        details: String,
    },

    #[error("配置错误: {0}")]
    ConfigError(String),
}

#[cfg(test)]
mod tests {
    use super::DepError;

    #[test]
    fn nexus_upload_error_displays_response_details() {
        let error = DepError::NexusUploadFailed {
            url: "http://nexus/repository/maven/a.pom".into(),
            status: 400,
            details: "Repository version policy: RELEASE does not allow SNAPSHOT".into(),
        };

        let message = error.to_string();
        assert!(message.contains("HTTP 400"));
        assert!(message.contains("Repository version policy"));
    }
}
