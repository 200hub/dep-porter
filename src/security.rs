use anyhow::Result;
use log::warn;
use serde::Deserialize;

use crate::model::DepKind;

/// OSV.dev漏洞查询请求。
#[derive(Debug, serde::Serialize)]
struct OsvQuery {
    package: OsvPackage,
    version: String,
}

#[derive(Debug, serde::Serialize)]
struct OsvPackage {
    name: String,
    ecosystem: String,
}

/// OSV.dev漏洞查询响应。
#[derive(Debug, Deserialize)]
struct OsvResponse {
    #[serde(default)]
    vulns: Vec<OsvVuln>,
}

#[derive(Debug, Deserialize)]
struct OsvVuln {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    severity: Vec<OsvSeverity>,
}

#[derive(Debug, Deserialize)]
struct OsvSeverity {
    score: String,
    #[serde(rename = "type")]
    _severity_type: String,
}

/// 单个漏洞发现。
#[derive(Debug)]
pub struct VulnFinding {
    pub id: String,
    pub summary: String,
    pub score: String,
}

/// 将DepKind映射到OSV.dev生态系统名称。
fn to_ecosystem(kind: DepKind) -> Option<&'static str> {
    match kind {
        DepKind::Maven => Some("Maven"),
        DepKind::Npm => Some("npm"),
        DepKind::Pypi => Some("PyPI"),
        DepKind::Cargo => Some("crates.io"),
        DepKind::Conan => None, // OSV.dev不支持Conan
    }
}

/// 查询OSV.dev以获取依赖项的已知漏洞。
///
/// 返回发现列表。空列表表示没有已知漏洞。
/// 如果生态系统不被OSV.dev支持，则返回`Ok(None)`。
pub fn check_vulnerabilities(
    kind: DepKind,
    name: &str,
    version: &str,
) -> Result<Option<Vec<VulnFinding>>> {
    let ecosystem = match to_ecosystem(kind) {
        Some(e) => e,
        None => return Ok(None),
    };

    let query = OsvQuery {
        package: OsvPackage {
            name: name.to_string(),
            ecosystem: ecosystem.to_string(),
        },
        version: version.to_string(),
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let resp = client
        .post("https://api.osv.dev/v1/query")
        .json(&query)
        .send()?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "OSV.dev API returned HTTP {}",
            resp.status()
        );
    }

    let osv: OsvResponse = resp.json()?;
    let findings: Vec<VulnFinding> = osv
        .vulns
        .into_iter()
        .map(|v| {
            let score = v
                .severity
                .first()
                .map(|s| s.score.clone())
                .unwrap_or_else(|| "unknown".to_string());
            VulnFinding {
                id: v.id,
                summary: v.summary,
                score,
            }
        })
        .collect();

    Ok(Some(findings))
}

/// 通过log crate打印漏洞发现。
pub fn print_findings(kind: DepKind, name: &str, version: &str, findings: &[VulnFinding]) {
    warn!("=== 安全警告 ===");
    warn!("  {} {}@{}", kind, name, version);
    warn!("  Found {} known vulnerability(ies):", findings.len());
    for (i, f) in findings.iter().enumerate() {
        if f.summary.is_empty() && f.score == "unknown" {
            warn!("  [{}] {}", i + 1, f.id);
        } else if f.score == "unknown" {
            warn!("  [{}] {} — {}", i + 1, f.id, f.summary);
        } else if f.summary.is_empty() {
            warn!("  [{}] {} (CVSS: {})", i + 1, f.id, f.score);
        } else {
            warn!("  [{}] {} (CVSS: {}) — {}", i + 1, f.id, f.score, f.summary);
        }
    }
    warn!("=========================");
}

/// 发现漏洞时提示用户继续或中止。
/// 返回`true`表示继续，`false`表示中止。
pub fn prompt_continue() -> bool {
    use std::io::{self, Write};

    eprint!("是否继续下载？[y/N] ");
    io::stderr().flush().ok();

    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let trimmed = input.trim().to_lowercase();
    trimmed == "y" || trimmed == "yes"
}
