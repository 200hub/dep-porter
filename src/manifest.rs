//! 解析主流打包工具的依赖清单与锁定文件，提取直接依赖。
//!
//! 支持的文件（按文件名识别）：
//!
//! | 文件                | 生态   | 说明                                   |
//! | ------------------- | ------ | -------------------------------------- |
//! | `pom.xml`           | maven  | `<dependencies>` 直接依赖              |
//! | `package.json`      | npm    | `dependencies`（可选含 `devDependencies`）|
//! | `package-lock.json` | npm    | 锁定文件，解析为精确版本的完整依赖集   |
//! | `requirements.txt`  | pypi   | `name==version` 固定版本               |
//! | `Cargo.toml`        | cargo  | `[dependencies]`（可选含 dev/build）   |
//! | `Cargo.lock`        | cargo  | 锁定文件，解析为精确版本的完整依赖集   |
//! | `conanfile.txt`     | conan  | `[requires]` 段                        |
//!
//! 清单文件（pom.xml、package.json、Cargo.toml、requirements.txt、conanfile.txt）
//! 提取“直接依赖”。锁定文件（package-lock.json、Cargo.lock）版本已固定，解析为
//! 完整的已解析依赖集合（每一项都视为可直接下载的精确坐标）。

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use log::warn;
use serde::Deserialize;

use crate::model::{DepKind, DepSpec};

/// 解析选项。
#[derive(Debug, Clone, Copy, Default)]
pub struct ParseOptions {
    /// 是否包含开发/构建/测试期依赖（npm 的 devDependencies、
    /// cargo 的 dev-dependencies / build-dependencies）。默认 false。
    pub include_dev: bool,
}

/// 根据文件名识别清单类型并解析其直接依赖。
///
/// 返回去重后的依赖列表。无法识别的文件名会返回错误。
pub fn parse_file(path: &Path, options: ParseOptions) -> Result<Vec<DepSpec>> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("无法识别文件名: {}", path.display()))?
        .to_lowercase();

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("无法读取依赖文件 '{}'", path.display()))?;

    let specs = parse_named(&file_name, &content, options)
        .with_context(|| format!("解析依赖文件 '{}' 失败", path.display()))?;

    Ok(dedup(specs))
}

/// 根据（小写的）文件名与内容解析依赖，便于单元测试。
pub fn parse_named(file_name: &str, content: &str, options: ParseOptions) -> Result<Vec<DepSpec>> {
    match file_name {
        "pom.xml" => parse_pom(content),
        "package-lock.json" => parse_package_lock(content),
        "package.json" => parse_package_json(content, options),
        "requirements.txt" => Ok(parse_requirements_txt(content)),
        "cargo.lock" => parse_cargo_lock(content),
        "cargo.toml" => parse_cargo_toml(content, options),
        "conanfile.txt" => Ok(parse_conanfile(content)),
        other => Err(anyhow!(
            "不支持的依赖文件 '{}'。支持: pom.xml、package.json、package-lock.json、requirements.txt、Cargo.toml、Cargo.lock、conanfile.txt",
            other
        )),
    }
}

/// 去重（保持首次出现顺序）。
fn dedup(specs: Vec<DepSpec>) -> Vec<DepSpec> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(specs.len());
    for s in specs {
        let key = (s.kind, s.name.clone(), s.version.clone());
        if seen.insert(key) {
            out.push(s);
        }
    }
    out
}

// ── Maven: pom.xml ──────────────────────────────────────────────────────────

/// 解析 pom.xml 中 `project > dependencies` 的直接依赖。
///
/// 支持 `<properties>` 与内置 `${project.version}` / `${project.groupId}`
/// 属性替换。跳过版本无法解析（例如由父 POM / dependencyManagement 管理）的依赖。
fn parse_pom(content: &str) -> Result<Vec<DepSpec>> {
    let doc = roxmltree::Document::parse(content).context("pom.xml 不是合法的 XML")?;
    let project = doc.root_element();

    // 收集属性用于 ${...} 替换。
    let mut props = std::collections::HashMap::new();
    if let Some(project_version) = child_text(&project, "version") {
        props.insert("project.version".to_string(), project_version.clone());
        props.insert("version".to_string(), project_version);
    }
    if let Some(project_group) = child_text(&project, "groupId") {
        props.insert("project.groupId".to_string(), project_group);
    }
    if let Some(properties) = child(&project, "properties") {
        for p in properties.children().filter(|n| n.is_element()) {
            if let Some(text) = node_text(&p) {
                props.insert(p.tag_name().name().to_string(), text);
            }
        }
    }

    let mut specs = Vec::new();
    // 仅取顶层 project > dependencies（忽略 dependencyManagement 与 build/plugins）。
    if let Some(deps) = child(&project, "dependencies") {
        for dep in deps.children().filter(|n| n.has_tag_name("dependency")) {
            let group = child_text(&dep, "groupId");
            let artifact = child_text(&dep, "artifactId");
            let version = child_text(&dep, "version");

            let (group, artifact) = match (group, artifact) {
                (Some(g), Some(a)) => (g, a),
                _ => continue,
            };

            let version = match version.and_then(|v| resolve_props(&v, &props)) {
                Some(v) if !v.is_empty() && !v.contains("${") => v,
                _ => {
                    warn!(
                        "跳过 Maven 依赖 {}:{}（无法确定版本，可能由父 POM 或 dependencyManagement 管理）",
                        group, artifact
                    );
                    continue;
                }
            };

            specs.push(DepSpec::new(
                DepKind::Maven,
                format!("{}:{}", group, artifact),
                version,
            ));
        }
    }
    Ok(specs)
}

/// 将 `${prop}` 替换为已知属性值。若存在未知属性则返回 None。
fn resolve_props(value: &str, props: &std::collections::HashMap<String, String>) -> Option<String> {
    let mut result = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find('}')?;
        let key = &after[..end];
        let replacement = props.get(key)?;
        result.push_str(replacement);
        rest = &after[end + 1..];
    }
    result.push_str(rest);
    Some(result)
}

fn child<'a, 'input>(
    node: &roxmltree::Node<'a, 'input>,
    tag: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children().find(|n| n.has_tag_name(tag))
}

fn node_text(node: &roxmltree::Node) -> Option<String> {
    let text = node.text().unwrap_or("").trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn child_text(node: &roxmltree::Node, tag: &str) -> Option<String> {
    child(node, tag).and_then(|n| node_text(&n))
}

// ── npm: package.json ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct PackageJson {
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, String>,
    #[serde(default, rename = "devDependencies")]
    dev_dependencies: std::collections::BTreeMap<String, String>,
    #[serde(default, rename = "optionalDependencies")]
    optional_dependencies: std::collections::BTreeMap<String, String>,
    #[serde(default, rename = "peerDependencies")]
    peer_dependencies: std::collections::BTreeMap<String, String>,
}

fn parse_package_json(content: &str, options: ParseOptions) -> Result<Vec<DepSpec>> {
    let pkg: PackageJson = serde_json::from_str(content).context("package.json 不是合法的 JSON")?;
    let mut specs = Vec::new();

    let mut groups: Vec<&std::collections::BTreeMap<String, String>> = vec![
        &pkg.dependencies,
        &pkg.optional_dependencies,
        &pkg.peer_dependencies,
    ];
    if options.include_dev {
        groups.push(&pkg.dev_dependencies);
    }

    for group in groups {
        for (name, range) in group {
            match clean_version_range(range) {
                Some(version) => specs.push(DepSpec::new(DepKind::Npm, name.clone(), version)),
                None => warn!(
                    "跳过 npm 依赖 {}（版本 '{}' 不是精确版本，建议改用 package-lock.json）",
                    name, range
                ),
            }
        }
    }
    Ok(specs)
}

// ── npm: package-lock.json ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct PackageLock {
    #[serde(default)]
    packages: std::collections::BTreeMap<String, LockPackage>,
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, LockDependencyV1>,
}

#[derive(Debug, Deserialize)]
struct LockPackage {
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LockDependencyV1 {
    #[serde(default)]
    version: Option<String>,
}

/// 解析 package-lock.json（v1/v2/v3），返回精确版本的完整依赖集。
fn parse_package_lock(content: &str) -> Result<Vec<DepSpec>> {
    let lock: PackageLock =
        serde_json::from_str(content).context("package-lock.json 不是合法的 JSON")?;
    let mut specs = Vec::new();

    // lockfile v2/v3: "packages" 以 node_modules 路径为键。
    for (path, pkg) in &lock.packages {
        if path.is_empty() {
            continue; // 根项目自身
        }
        let name = match path.rsplit_once("node_modules/") {
            Some((_, name)) => name,
            None => continue,
        };
        if let Some(version) = &pkg.version {
            if !version.is_empty() {
                specs.push(DepSpec::new(
                    DepKind::Npm,
                    name.to_string(),
                    version.clone(),
                ));
            }
        }
    }

    // lockfile v1: 顶层 "dependencies"。
    for (name, dep) in &lock.dependencies {
        if let Some(version) = &dep.version {
            if !version.is_empty() {
                specs.push(DepSpec::new(DepKind::Npm, name.clone(), version.clone()));
            }
        }
    }

    Ok(specs)
}

// ── pypi: requirements.txt ──────────────────────────────────────────────────

fn parse_requirements_txt(content: &str) -> Vec<DepSpec> {
    let mut specs = Vec::new();
    for raw_line in content.lines() {
        // 去掉注释与空白。
        let line = match raw_line.split('#').next() {
            Some(l) => l.trim(),
            None => continue,
        };
        if line.is_empty() || line.starts_with('-') {
            continue; // 跳过空行与 -r/-e/--option 指令
        }
        // 仅支持精确固定版本 name==version。
        if let Some((name, version)) = line.split_once("==") {
            let name = name.trim();
            // 去掉可能存在的 extras，如 requests[security]
            let name = name.split('[').next().unwrap_or(name).trim();
            // 去掉版本上的环境标记 / 尾随分号。
            let version = version
                .split(';')
                .next()
                .unwrap_or(version)
                .trim()
                .trim_end_matches('.')
                .trim();
            if !name.is_empty() && !version.is_empty() && !version.contains('*') {
                specs.push(DepSpec::new(
                    DepKind::Pypi,
                    name.to_string(),
                    version.to_string(),
                ));
            }
        } else {
            warn!("跳过 PyPI 依赖 '{}'（仅支持 name==version 固定版本）", line);
        }
    }
    specs
}

// ── cargo: Cargo.toml ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CargoToml {
    #[serde(default)]
    dependencies: toml::Table,
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: toml::Table,
    #[serde(default, rename = "build-dependencies")]
    build_dependencies: toml::Table,
}

fn parse_cargo_toml(content: &str, options: ParseOptions) -> Result<Vec<DepSpec>> {
    let manifest: CargoToml = toml::from_str(content).context("Cargo.toml 不是合法的 TOML")?;
    let mut specs = Vec::new();

    let mut tables: Vec<&toml::Table> = vec![&manifest.dependencies];
    if options.include_dev {
        tables.push(&manifest.dev_dependencies);
        tables.push(&manifest.build_dependencies);
    }

    for table in tables {
        for (name, value) in table {
            let version = match value {
                toml::Value::String(v) => Some(v.clone()),
                toml::Value::Table(t) => {
                    t.get("version").and_then(|v| v.as_str()).map(String::from)
                }
                _ => None,
            };
            match version.as_deref().and_then(clean_version_range) {
                Some(version) => specs.push(DepSpec::new(DepKind::Cargo, name.clone(), version)),
                None => warn!(
                    "跳过 Cargo 依赖 {}（无精确版本，git/path 依赖或版本范围无法离线下载，建议改用 Cargo.lock）",
                    name
                ),
            }
        }
    }
    Ok(specs)
}

// ── cargo: Cargo.lock ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CargoLock {
    #[serde(default)]
    package: Vec<CargoLockPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoLockPackage {
    name: String,
    version: String,
    #[serde(default)]
    source: Option<String>,
}

/// 解析 Cargo.lock，返回来自 crates.io 的精确版本依赖集。
fn parse_cargo_lock(content: &str) -> Result<Vec<DepSpec>> {
    let lock: CargoLock = toml::from_str(content).context("Cargo.lock 不是合法的 TOML")?;
    let mut specs = Vec::new();
    for pkg in lock.package {
        // 仅下载来自 registry 的包；本地 path 依赖（source 为 None）与 git 依赖跳过。
        match pkg.source.as_deref() {
            Some(src) if src.starts_with("registry+") => {
                specs.push(DepSpec::new(DepKind::Cargo, pkg.name, pkg.version));
            }
            _ => {}
        }
    }
    Ok(specs)
}

// ── conan: conanfile.txt ────────────────────────────────────────────────────

fn parse_conanfile(content: &str) -> Vec<DepSpec> {
    let mut specs = Vec::new();
    let mut in_requires = false;
    for raw_line in content.lines() {
        let line = match raw_line.split('#').next() {
            Some(l) => l.trim(),
            None => continue,
        };
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = line.trim_start_matches('[').trim_end_matches(']').trim();
            in_requires = section == "requires";
            continue;
        }
        if in_requires {
            // 形如 name/version 或 name/version@user/channel
            if let Some((name, rest)) = line.split_once('/') {
                let version = rest.split('@').next().unwrap_or(rest).trim();
                let name = name.trim();
                if !name.is_empty() && !version.is_empty() {
                    specs.push(DepSpec::new(
                        DepKind::Conan,
                        name.to_string(),
                        version.to_string(),
                    ));
                }
            }
        }
    }
    specs
}

// ── 版本范围清理 ────────────────────────────────────────────────────────────

/// 尝试将一个版本范围规约为可离线下载的精确版本。
///
/// - 剥离前缀操作符：`^ ~ = v` 及空白。
/// - 拒绝无法确定单一版本的范围：`* x latest`、空格分隔的复合范围、
///   `|| , - < >` 等区间操作符、URL / git / file 引用。
fn clean_version_range(range: &str) -> Option<String> {
    let range = range.trim();
    if range.is_empty() {
        return None;
    }
    // 明显不是精确版本的形式。
    let lowered = range.to_lowercase();
    if lowered == "*"
        || lowered == "latest"
        || lowered.contains("://")
        || lowered.starts_with("git")
        || lowered.starts_with("file:")
        || lowered.starts_with("npm:")
        || lowered.starts_with("workspace:")
    {
        return None;
    }
    // 复合范围 / 区间操作符。
    if range.contains("||")
        || range.contains(',')
        || range.contains(' ')
        || range.contains('<')
        || range.contains('>')
        || range.contains('-')
        || range.contains('*')
        || range.contains('x')
        || range.contains('X')
    {
        return None;
    }

    let cleaned = range.trim_start_matches(['^', '~', '=', 'v', 'V']).trim();
    if cleaned.is_empty() {
        return None;
    }
    // 精确版本至少要以数字开头。
    if !cleaned
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        return None;
    }
    Some(cleaned.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(specs: &[DepSpec]) -> Vec<(DepKind, String, String)> {
        specs
            .iter()
            .map(|s| (s.kind, s.name.clone(), s.version.clone()))
            .collect()
    }

    #[test]
    fn clean_version_range_strips_operators() {
        assert_eq!(clean_version_range("^1.2.3").as_deref(), Some("1.2.3"));
        assert_eq!(clean_version_range("~4.17.21").as_deref(), Some("4.17.21"));
        assert_eq!(clean_version_range("=2.0.0").as_deref(), Some("2.0.0"));
        assert_eq!(clean_version_range("v1.0.0").as_deref(), Some("1.0.0"));
        assert_eq!(clean_version_range("1.0.0").as_deref(), Some("1.0.0"));
    }

    #[test]
    fn clean_version_range_rejects_non_exact() {
        assert_eq!(clean_version_range("*"), None);
        assert_eq!(clean_version_range("latest"), None);
        assert_eq!(clean_version_range("1.x"), None);
        assert_eq!(clean_version_range(">=1.0.0"), None);
        assert_eq!(clean_version_range("1.0.0 - 2.0.0"), None);
        assert_eq!(clean_version_range("^1 || ^2"), None);
        assert_eq!(clean_version_range("git+https://x/y.git"), None);
        assert_eq!(clean_version_range("workspace:*"), None);
        assert_eq!(clean_version_range(""), None);
    }

    #[test]
    fn parse_pom_extracts_direct_dependencies_with_properties() {
        let pom = r#"<?xml version="1.0"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>demo</artifactId>
  <version>1.0.0</version>
  <properties>
    <lang3.version>3.14.0</lang3.version>
  </properties>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>managed</groupId>
        <artifactId>managed-lib</artifactId>
        <version>9.9.9</version>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>org.apache.commons</groupId>
      <artifactId>commons-lang3</artifactId>
      <version>${lang3.version}</version>
    </dependency>
    <dependency>
      <groupId>com.google.guava</groupId>
      <artifactId>guava</artifactId>
      <version>33.0.0-jre</version>
    </dependency>
    <dependency>
      <groupId>no.version</groupId>
      <artifactId>managed</artifactId>
    </dependency>
  </dependencies>
</project>"#;
        let specs = parse_pom(pom).unwrap();
        assert_eq!(
            names(&specs),
            vec![
                (
                    DepKind::Maven,
                    "org.apache.commons:commons-lang3".into(),
                    "3.14.0".into()
                ),
                (
                    DepKind::Maven,
                    "com.google.guava:guava".into(),
                    "33.0.0-jre".into()
                ),
            ]
        );
    }

    #[test]
    fn parse_package_json_uses_dependencies_only_by_default() {
        let json = r#"{
          "name": "demo",
          "dependencies": { "lodash": "^4.17.21", "left-pad": "1.3.0" },
          "devDependencies": { "jest": "^29.0.0" }
        }"#;
        let specs = parse_package_json(json, ParseOptions::default()).unwrap();
        let mut got = names(&specs);
        got.sort();
        assert_eq!(
            got,
            vec![
                (DepKind::Npm, "left-pad".into(), "1.3.0".into()),
                (DepKind::Npm, "lodash".into(), "4.17.21".into()),
            ]
        );
    }

    #[test]
    fn parse_package_json_include_dev() {
        let json = r#"{
          "dependencies": { "lodash": "4.17.21" },
          "devDependencies": { "jest": "29.0.0" }
        }"#;
        let specs = parse_package_json(json, ParseOptions { include_dev: true }).unwrap();
        let mut got = names(&specs);
        got.sort();
        assert_eq!(
            got,
            vec![
                (DepKind::Npm, "jest".into(), "29.0.0".into()),
                (DepKind::Npm, "lodash".into(), "4.17.21".into()),
            ]
        );
    }

    #[test]
    fn parse_package_lock_v3_resolves_exact_versions() {
        let lock = r#"{
          "name": "demo",
          "lockfileVersion": 3,
          "packages": {
            "": { "name": "demo", "version": "1.0.0" },
            "node_modules/lodash": { "version": "4.17.21" },
            "node_modules/@scope/pkg": { "version": "2.3.4" }
          }
        }"#;
        let specs = parse_package_lock(lock).unwrap();
        let mut got = names(&specs);
        got.sort();
        assert_eq!(
            got,
            vec![
                (DepKind::Npm, "@scope/pkg".into(), "2.3.4".into()),
                (DepKind::Npm, "lodash".into(), "4.17.21".into()),
            ]
        );
    }

    #[test]
    fn parse_package_lock_v1_resolves_exact_versions() {
        let lock = r#"{
          "name": "demo",
          "lockfileVersion": 1,
          "dependencies": {
            "lodash": { "version": "4.17.21" }
          }
        }"#;
        let specs = parse_package_lock(lock).unwrap();
        assert_eq!(
            names(&specs),
            vec![(DepKind::Npm, "lodash".into(), "4.17.21".into())]
        );
    }

    #[test]
    fn parse_requirements_txt_pinned_versions() {
        let req = r#"
# comment
requests==2.32.3
flask==3.0.0  # inline comment
urllib3>=1.0    # skipped, not pinned
-r other.txt
django[argon2]==5.0.1
"#;
        let specs = parse_requirements_txt(req);
        let mut got = names(&specs);
        got.sort();
        assert_eq!(
            got,
            vec![
                (DepKind::Pypi, "django".into(), "5.0.1".into()),
                (DepKind::Pypi, "flask".into(), "3.0.0".into()),
                (DepKind::Pypi, "requests".into(), "2.32.3".into()),
            ]
        );
    }

    #[test]
    fn parse_cargo_toml_extracts_dependencies() {
        let cargo = r#"
[package]
name = "demo"

[dependencies]
serde = "1.0.203"
anyhow = { version = "1.0.86", features = ["backtrace"] }
local = { path = "../local" }

[dev-dependencies]
tempfile = "3.10.0"
"#;
        let specs = parse_cargo_toml(cargo, ParseOptions::default()).unwrap();
        let mut got = names(&specs);
        got.sort();
        assert_eq!(
            got,
            vec![
                (DepKind::Cargo, "anyhow".into(), "1.0.86".into()),
                (DepKind::Cargo, "serde".into(), "1.0.203".into()),
            ]
        );
    }

    #[test]
    fn parse_cargo_lock_registry_only() {
        let lock = r#"
version = 3

[[package]]
name = "demo"
version = "0.1.0"

[[package]]
name = "serde"
version = "1.0.203"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "local-dep"
version = "0.1.0"
"#;
        let specs = parse_cargo_lock(lock).unwrap();
        assert_eq!(
            names(&specs),
            vec![(DepKind::Cargo, "serde".into(), "1.0.203".into())]
        );
    }

    #[test]
    fn parse_conanfile_requires_section() {
        let conan = r#"
[requires]
zlib/1.2.13
openssl/3.4.1@user/channel

[generators]
CMakeToolchain
"#;
        let specs = parse_conanfile(conan);
        let mut got = names(&specs);
        got.sort();
        assert_eq!(
            got,
            vec![
                (DepKind::Conan, "openssl".into(), "3.4.1".into()),
                (DepKind::Conan, "zlib".into(), "1.2.13".into()),
            ]
        );
    }

    #[test]
    fn parse_named_dedups() {
        let json =
            r#"{ "dependencies": { "a": "1.0.0" }, "optionalDependencies": { "a": "1.0.0" } }"#;
        let specs = dedup(parse_named("package.json", json, ParseOptions::default()).unwrap());
        assert_eq!(specs.len(), 1);
    }

    #[test]
    fn parse_named_rejects_unknown_file() {
        assert!(parse_named("Gemfile", "", ParseOptions::default()).is_err());
    }
}
