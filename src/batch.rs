//! 批量操作编排：在解析出的直接依赖之上，一次性做安全 / 许可证风险检查，
//! 并支持批量下载与批量导入。底层完全复用现有的单依赖 download / import 能力。

use crate::license::{self, LicenseFinding};
use crate::model::DepSpec;
use crate::security::{self, VulnFinding};

/// 单个依赖的风险检查结果。
#[derive(Debug)]
pub struct DepReport {
    pub spec: DepSpec,
    /// 安全漏洞发现（为空表示无已知漏洞）。
    pub vulns: Vec<VulnFinding>,
    /// 许可证检查结果（None 表示生态不支持或未检查）。
    pub license: Option<LicenseFinding>,
}

impl DepReport {
    /// 该依赖是否存在需要用户确认的风险（漏洞或需复核的许可证）。
    pub fn requires_confirmation(&self) -> bool {
        !self.vulns.is_empty()
            || self
                .license
                .as_ref()
                .map(license_needs_confirmation)
                .unwrap_or(false)
    }
}

/// 判断许可证结果是否需要用户确认。
pub fn license_needs_confirmation(finding: &LicenseFinding) -> bool {
    use license::LicenseAssessment::*;
    matches!(finding.assessment, Copyleft | ReviewRequired(_))
}

/// 任意一个依赖存在需要确认的风险时返回 true。
pub fn any_requires_confirmation(reports: &[DepReport]) -> bool {
    reports.iter().any(DepReport::requires_confirmation)
}

/// 对一批依赖执行安全与许可证检查，返回逐个依赖的报告。
///
/// 单个依赖检查失败（网络等原因）不会中断整体流程，仅记录为无发现。
pub fn collect_reports(
    specs: &[DepSpec],
    check_security: bool,
    check_license: bool,
) -> Vec<DepReport> {
    use log::warn;

    let mut reports = Vec::with_capacity(specs.len());
    for spec in specs {
        let vulns = if check_security {
            match security::check_vulnerabilities(spec.kind, &spec.name, &spec.version) {
                Ok(Some(findings)) => findings,
                Ok(None) => Vec::new(),
                Err(e) => {
                    warn!(
                        "安全检查失败 {} {}@{}（{}），跳过该项检查。",
                        spec.kind, spec.name, spec.version, e
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        let license = if check_license {
            match license::check_license(spec.kind, &spec.name, &spec.version) {
                Ok(finding) => finding,
                Err(e) => {
                    warn!(
                        "许可证检查失败 {} {}@{}（{}），跳过该项检查。",
                        spec.kind, spec.name, spec.version, e
                    );
                    None
                }
            }
        } else {
            None
        };

        reports.push(DepReport {
            spec: spec.clone(),
            vulns,
            license,
        });
    }
    reports
}

/// 汇总打印所有报告中的风险发现。
pub fn print_reports(reports: &[DepReport]) {
    for report in reports {
        let spec = &report.spec;
        if !report.vulns.is_empty() {
            security::print_findings(spec.kind, &spec.name, &spec.version, &report.vulns);
        }
        if let Some(finding) = &report.license {
            if license_needs_confirmation(finding) {
                license::print_finding(spec.kind, &spec.name, &spec.version, finding);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license::LicenseAssessment;
    use crate::model::DepKind;

    fn spec() -> DepSpec {
        DepSpec::new(DepKind::Npm, "lodash".into(), "4.17.21".into())
    }

    #[test]
    fn report_without_findings_needs_no_confirmation() {
        let report = DepReport {
            spec: spec(),
            vulns: Vec::new(),
            license: Some(LicenseFinding {
                licenses: vec!["MIT".into()],
                assessment: LicenseAssessment::Permissive,
            }),
        };
        assert!(!report.requires_confirmation());
        assert!(!any_requires_confirmation(&[report]));
    }

    #[test]
    fn report_with_vuln_needs_confirmation() {
        let report = DepReport {
            spec: spec(),
            vulns: vec![VulnFinding {
                id: "CVE-1".into(),
                summary: "bad".into(),
                score: "9.8".into(),
            }],
            license: None,
        };
        assert!(report.requires_confirmation());
        assert!(any_requires_confirmation(&[report]));
    }

    #[test]
    fn report_with_copyleft_license_needs_confirmation() {
        let report = DepReport {
            spec: spec(),
            vulns: Vec::new(),
            license: Some(LicenseFinding {
                licenses: vec!["GPL-3.0".into()],
                assessment: LicenseAssessment::Copyleft,
            }),
        };
        assert!(report.requires_confirmation());
    }

    #[test]
    fn report_with_review_required_license_needs_confirmation() {
        let report = DepReport {
            spec: spec(),
            vulns: Vec::new(),
            license: Some(LicenseFinding {
                licenses: vec![],
                assessment: LicenseAssessment::ReviewRequired("unknown".into()),
            }),
        };
        assert!(report.requires_confirmation());
    }
}
