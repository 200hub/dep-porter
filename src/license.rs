use std::time::Duration;

use anyhow::{Context, Result};
use log::{info, warn};
use serde::Deserialize;

use crate::model::DepKind;

const DEPS_DEV_API: &str = "https://api.deps.dev/v3/";

#[derive(Debug, Deserialize)]
struct DepsDevVersion {
    #[serde(default)]
    licenses: Vec<String>,
}

/// deps.dev返回的许可证检测结果。
#[derive(Debug, PartialEq, Eq)]
pub struct LicenseFinding {
    pub licenses: Vec<String>,
    pub assessment: LicenseAssessment,
}

/// 商用许可证评估。该结果只用于风险提醒，不能替代法律审查。
#[derive(Debug, PartialEq, Eq)]
pub enum LicenseAssessment {
    /// 常见宽松许可证，未发现明确的商用限制。
    Permissive,
    /// 允许商用，但分发或网络服务等场景可能触发开源义务。
    Copyleft,
    /// 许可证可能限制商用，或数据不足以自动判断。
    ReviewRequired(String),
}

fn to_deps_dev_system(kind: DepKind) -> Option<&'static str> {
    match kind {
        DepKind::Maven => Some("maven"),
        DepKind::Npm => Some("npm"),
        DepKind::Pypi => Some("pypi"),
        DepKind::Cargo => Some("cargo"),
        DepKind::Conan => None,
    }
}

fn version_url(system: &str, name: &str, version: &str) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(DEPS_DEV_API)?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("invalid deps.dev API base URL"))?
        .pop_if_empty()
        .extend(["systems", system, "packages", name, "versions", version]);
    Ok(url)
}

/// 通过deps.dev查询指定依赖版本的许可证并评估商用风险。
///
/// 如果生态系统不被deps.dev支持，返回`Ok(None)`。
pub fn check_license(kind: DepKind, name: &str, version: &str) -> Result<Option<LicenseFinding>> {
    let system = match to_deps_dev_system(kind) {
        Some(system) => system,
        None => return Ok(None),
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("dep-porter/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let response = client
        .get(version_url(system, name, version)?)
        .send()
        .context("请求deps.dev许可证数据失败")?;

    if !response.status().is_success() {
        anyhow::bail!("deps.dev API returned HTTP {}", response.status());
    }

    let version: DepsDevVersion = response.json().context("解析deps.dev许可证数据失败")?;
    let assessment = assess_licenses(&version.licenses);
    Ok(Some(LicenseFinding {
        licenses: version.licenses,
        assessment,
    }))
}

/// 根据deps.dev提供的SPDX表达式进行保守的商用风险分类。
fn assess_licenses(licenses: &[String]) -> LicenseAssessment {
    if licenses.is_empty() {
        return LicenseAssessment::ReviewRequired("未获取到许可证信息，无法确认是否可商用".into());
    }

    let combined = licenses.join(" OR ").to_ascii_uppercase();
    let non_commercial_markers = [
        "NONCOMMERCIAL",
        "NON-COMMERCIAL",
        "-NC-",
        "-NC",
        "POLYFORM-NONCOMMERCIAL",
        "PROSPERITY",
        "BUSL-",
        "BUSINESS SOURCE",
    ];
    if non_commercial_markers
        .iter()
        .any(|marker| combined.contains(marker))
    {
        return LicenseAssessment::ReviewRequired("许可证可能包含非商用或商业使用限制".into());
    }

    let unknown_markers = ["NON-STANDARD", "LICENSEREF-", "SEE LICENSE", "UNKNOWN"];
    if unknown_markers
        .iter()
        .any(|marker| combined.contains(marker))
    {
        return LicenseAssessment::ReviewRequired("许可证不是可自动判断的标准SPDX许可证".into());
    }

    let copyleft_markers = [
        "AGPL-", "GPL-", "LGPL-", "MPL-", "EPL-", "EUPL-", "CDDL-", "OSL-", "CPL-",
    ];
    if copyleft_markers
        .iter()
        .any(|marker| combined.contains(marker))
    {
        return LicenseAssessment::Copyleft;
    }

    const PERMISSIVE_IDS: &[&str] = &[
        "0BSD",
        "APACHE-2.0",
        "BSD-2-CLAUSE",
        "BSD-3-CLAUSE",
        "BSD-3-CLAUSE-CLEAR",
        "BSL-1.0",
        "CC0-1.0",
        "ISC",
        "MIT",
        "MIT-0",
        "UNLICENSE",
        "ZLIB",
    ];
    let identifiers: Vec<&str> = combined
        .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')'))
        .filter(|token| !token.is_empty() && !matches!(*token, "AND" | "OR" | "WITH"))
        .collect();

    if identifiers
        .iter()
        .all(|id| PERMISSIVE_IDS.contains(id) || id.ends_with("-EXCEPTION"))
    {
        LicenseAssessment::Permissive
    } else {
        LicenseAssessment::ReviewRequired("许可证不在内置的常见宽松许可证列表中".into())
    }
}

/// 输出许可证结果；返回`true`表示需要用户确认后再继续。
pub fn print_finding(kind: DepKind, name: &str, version: &str, finding: &LicenseFinding) -> bool {
    let licenses = if finding.licenses.is_empty() {
        "（缺失）".to_string()
    } else {
        finding.licenses.join(", ")
    };

    match &finding.assessment {
        LicenseAssessment::Permissive => {
            info!(
                "许可证检测：{} {}@{} 使用 {}，未发现明确的商用限制。",
                kind, name, version, licenses
            );
            false
        }
        LicenseAssessment::Copyleft => {
            warn!("=== 许可证提醒 ===");
            warn!("  {} {}@{}", kind, name, version);
            warn!("  许可证: {}", licenses);
            warn!("  该许可证允许商用，但分发、修改或网络服务等场景可能触发开源义务。");
            warn!("  请结合实际使用方式进行合规审查；本检测不构成法律意见。");
            warn!("===================");
            true
        }
        LicenseAssessment::ReviewRequired(reason) => {
            warn!("=== 许可证提醒 ===");
            warn!("  {} {}@{}", kind, name, version);
            warn!("  许可证: {}", licenses);
            warn!("  {}。", reason);
            warn!("  请在商用前人工核验许可证原文；本检测不构成法律意见。");
            warn!("===================");
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn licenses(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn encodes_scoped_and_maven_package_names() {
        assert_eq!(
            version_url("npm", "@scope/pkg", "1.0.0").unwrap().as_str(),
            "https://api.deps.dev/v3/systems/npm/packages/@scope%2Fpkg/versions/1.0.0"
        );
        assert_eq!(
            version_url("maven", "org.example:demo", "1.0.0")
                .unwrap()
                .as_str(),
            "https://api.deps.dev/v3/systems/maven/packages/org.example:demo/versions/1.0.0"
        );
    }

    #[test]
    fn recognizes_common_permissive_expressions() {
        assert_eq!(
            assess_licenses(&licenses(&["Apache-2.0 OR MIT"])),
            LicenseAssessment::Permissive
        );
    }

    #[test]
    fn flags_copyleft_obligations_without_claiming_commercial_use_is_forbidden() {
        assert_eq!(
            assess_licenses(&licenses(&["AGPL-3.0-only"])),
            LicenseAssessment::Copyleft
        );
    }

    #[test]
    fn flags_non_commercial_and_missing_licenses() {
        assert!(matches!(
            assess_licenses(&licenses(&["CC-BY-NC-4.0"])),
            LicenseAssessment::ReviewRequired(_)
        ));
        assert!(matches!(
            assess_licenses(&[]),
            LicenseAssessment::ReviewRequired(_)
        ));
    }
}
