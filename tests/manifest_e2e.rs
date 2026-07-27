//! 批量依赖文件解析的端到端测试：将真实的清单 / 锁定文件写入磁盘，
//! 通过 `manifest::parse_file`（会自动按文件名识别类型）解析出直接依赖。
//!
//! 这些测试不需要 Docker 或 Nexus，覆盖批量功能的核心解析路径。

use std::fs;

use dep_porter::manifest::{self, ParseOptions};
use dep_porter::model::{DepKind, DepSpec};

/// 将内容写入临时目录中的指定文件名并解析。
fn parse(file_name: &str, content: &str, options: ParseOptions) -> Vec<DepSpec> {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(file_name);
    fs::write(&path, content).unwrap();
    manifest::parse_file(&path, options)
        .unwrap_or_else(|e| panic!("parse {file_name} failed: {e:#}"))
}

fn sorted_tuples(specs: &[DepSpec]) -> Vec<(DepKind, String, String)> {
    let mut got: Vec<_> = specs
        .iter()
        .map(|s| (s.kind, s.name.clone(), s.version.clone()))
        .collect();
    got.sort();
    got
}

#[test]
fn e2e_parse_pom_xml() {
    let pom = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>demo</artifactId>
  <version>1.0.0</version>
  <properties>
    <junit.version>4.13.2</junit.version>
  </properties>
  <dependencies>
    <dependency>
      <groupId>org.apache.commons</groupId>
      <artifactId>commons-lang3</artifactId>
      <version>3.14.0</version>
    </dependency>
    <dependency>
      <groupId>junit</groupId>
      <artifactId>junit</artifactId>
      <version>${junit.version}</version>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>"#;
    let specs = parse("pom.xml", pom, ParseOptions::default());
    assert_eq!(
        sorted_tuples(&specs),
        vec![
            (DepKind::Maven, "junit:junit".into(), "4.13.2".into()),
            (
                DepKind::Maven,
                "org.apache.commons:commons-lang3".into(),
                "3.14.0".into()
            ),
        ]
    );
}

#[test]
fn e2e_parse_package_json() {
    let json = r#"{
      "name": "demo",
      "version": "1.0.0",
      "dependencies": { "lodash": "^4.17.21", "left-pad": "1.3.0" },
      "devDependencies": { "jest": "^29.0.0" }
    }"#;
    // 默认不含 devDependencies。
    let specs = parse("package.json", json, ParseOptions::default());
    assert_eq!(
        sorted_tuples(&specs),
        vec![
            (DepKind::Npm, "left-pad".into(), "1.3.0".into()),
            (DepKind::Npm, "lodash".into(), "4.17.21".into()),
        ]
    );

    // include_dev 时包含 devDependencies。
    let specs = parse("package.json", json, ParseOptions { include_dev: true });
    assert!(specs
        .iter()
        .any(|s| s.name == "jest" && s.version == "29.0.0"));
}

#[test]
fn e2e_parse_package_lock_json() {
    let lock = r#"{
      "name": "demo",
      "lockfileVersion": 3,
      "packages": {
        "": { "name": "demo", "version": "1.0.0" },
        "node_modules/lodash": { "version": "4.17.21" }
      }
    }"#;
    let specs = parse("package-lock.json", lock, ParseOptions::default());
    assert_eq!(
        sorted_tuples(&specs),
        vec![(DepKind::Npm, "lodash".into(), "4.17.21".into())]
    );
}

#[test]
fn e2e_parse_requirements_txt() {
    let req = "requests==2.32.3\nflask==3.0.0\n# a comment\nurllib3>=1.0\n";
    let specs = parse("requirements.txt", req, ParseOptions::default());
    assert_eq!(
        sorted_tuples(&specs),
        vec![
            (DepKind::Pypi, "flask".into(), "3.0.0".into()),
            (DepKind::Pypi, "requests".into(), "2.32.3".into()),
        ]
    );
}

#[test]
fn e2e_parse_cargo_toml() {
    let cargo = r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
serde = "1.0.203"
anyhow = { version = "1.0.86" }
"#;
    let specs = parse("Cargo.toml", cargo, ParseOptions::default());
    assert_eq!(
        sorted_tuples(&specs),
        vec![
            (DepKind::Cargo, "anyhow".into(), "1.0.86".into()),
            (DepKind::Cargo, "serde".into(), "1.0.203".into()),
        ]
    );
}

#[test]
fn e2e_parse_cargo_lock() {
    let lock = r#"
version = 3

[[package]]
name = "demo"
version = "0.1.0"

[[package]]
name = "serde"
version = "1.0.203"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;
    let specs = parse("Cargo.lock", lock, ParseOptions::default());
    assert_eq!(
        sorted_tuples(&specs),
        vec![(DepKind::Cargo, "serde".into(), "1.0.203".into())]
    );
}

#[test]
fn e2e_parse_conanfile_txt() {
    let conan = "[requires]\nzlib/1.2.13\n\n[generators]\nCMakeToolchain\n";
    let specs = parse("conanfile.txt", conan, ParseOptions::default());
    assert_eq!(
        sorted_tuples(&specs),
        vec![(DepKind::Conan, "zlib".into(), "1.2.13".into())]
    );
}

#[test]
fn e2e_unsupported_file_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("Gemfile");
    fs::write(&path, "gem 'rails'").unwrap();
    assert!(manifest::parse_file(&path, ParseOptions::default()).is_err());
}
