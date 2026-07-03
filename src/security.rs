use anyhow::Result;
use log::warn;
use serde::Deserialize;

use crate::model::DepKind;

/// OSV.dev vulnerability query request.
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

/// OSV.dev vulnerability query response.
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

/// A single vulnerability finding.
#[derive(Debug)]
pub struct VulnFinding {
    pub id: String,
    pub summary: String,
    pub score: String,
}

/// Map DepKind to OSV.dev ecosystem name.
fn to_ecosystem(kind: DepKind) -> Option<&'static str> {
    match kind {
        DepKind::Maven => Some("Maven"),
        DepKind::Npm => Some("npm"),
        DepKind::Pypi => Some("PyPI"),
        DepKind::Cargo => Some("crates.io"),
        DepKind::Conan => None, // OSV.dev does not support Conan
    }
}

/// Query OSV.dev for known vulnerabilities of a dependency.
///
/// Returns a list of findings. An empty list means no known vulnerabilities.
/// Returns `Ok(None)` if the ecosystem is not supported by OSV.dev.
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

/// Print vulnerability findings via the log crate.
pub fn print_findings(kind: DepKind, name: &str, version: &str, findings: &[VulnFinding]) {
    warn!("=== Security Advisory ===");
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

/// Prompt the user to continue or abort when vulnerabilities are found.
/// Returns `true` to continue, `false` to abort.
pub fn prompt_continue() -> bool {
    use std::io::{self, Write};

    eprint!("Continue download anyway? [y/N] ");
    io::stderr().flush().ok();

    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let trimmed = input.trim().to_lowercase();
    trimmed == "y" || trimmed == "yes"
}
