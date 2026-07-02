use std::fs;
use std::path::Path;

use dep_porter::model::{DepError, DepKind, MavenCoordinate};
use dep_porter::util::build_dir_name;

// ── Local logic tests (no Docker / Nexus required) ──────────────────

#[test]
fn test_dir_name_maven() {
    let name = build_dir_name(DepKind::Maven, "org.apache.commons:commons-lang3", "3.14.0");
    assert_eq!(name, "maven_org.apache.commons_commons-lang3_3.14.0");
}

#[test]
fn test_dir_name_npm() {
    let name = build_dir_name(DepKind::Npm, "lodash", "4.17.21");
    assert_eq!(name, "npm_lodash_4.17.21");
}

#[test]
fn test_dir_name_pypi() {
    let name = build_dir_name(DepKind::Pypi, "requests", "2.32.3");
    assert_eq!(name, "pypi_requests_2.32.3");
}

#[test]
fn test_dir_name_cargo() {
    let name = build_dir_name(DepKind::Cargo, "serde", "1.0.203");
    assert_eq!(name, "cargo_serde_1.0.203");
}

#[test]
fn test_dir_name_conan() {
    let name = build_dir_name(DepKind::Conan, "zlib", "1.2.13");
    assert_eq!(name, "conan_zlib_1.2.13");
}

#[test]
fn test_dir_name_special_chars() {
    let name = build_dir_name(DepKind::Npm, "@angular/core", "17.0.0");
    assert_eq!(name, "npm__angular_core_17.0.0");
}

#[test]
fn test_dir_name_backslash() {
    let name = build_dir_name(DepKind::Maven, "com.example\\lib", "1.0.0");
    assert_eq!(name, "maven_com.example_lib_1.0.0");
}

#[test]
fn test_maven_coordinate_parse() {
    let coord = MavenCoordinate::parse("org.apache.commons:commons-lang3").unwrap();
    assert_eq!(coord.group_id, "org.apache.commons");
    assert_eq!(coord.artifact_id, "commons-lang3");
    assert_eq!(coord.group_path(), "org/apache/commons");
}

#[test]
fn test_maven_coordinate_parse_single_segment_group() {
    let coord = MavenCoordinate::parse("junit:junit").unwrap();
    assert_eq!(coord.group_id, "junit");
    assert_eq!(coord.artifact_id, "junit");
    assert_eq!(coord.group_path(), "junit");
}

#[test]
fn test_maven_coordinate_parse_invalid_no_colon() {
    let err = MavenCoordinate::parse("no-colon").unwrap_err();
    match err {
        DepError::InvalidMavenCoord(_) => {}
        _ => panic!("Expected InvalidMavenCoord, got {:?}", err),
    }
}

#[test]
fn test_maven_coordinate_parse_invalid_empty_group() {
    let err = MavenCoordinate::parse(":artifact").unwrap_err();
    match err {
        DepError::InvalidMavenCoord(_) => {}
        _ => panic!("Expected InvalidMavenCoord, got {:?}", err),
    }
}

#[test]
fn test_maven_coordinate_parse_invalid_empty_artifact() {
    let err = MavenCoordinate::parse("group:").unwrap_err();
    match err {
        DepError::InvalidMavenCoord(_) => {}
        _ => panic!("Expected InvalidMavenCoord, got {:?}", err),
    }
}

#[test]
fn test_maven_coordinate_parse_invalid_too_many_parts() {
    let err = MavenCoordinate::parse("a:b:c").unwrap_err();
    match err {
        DepError::InvalidMavenCoord(_) => {}
        _ => panic!("Expected InvalidMavenCoord, got {:?}", err),
    }
}

#[test]
fn test_dep_kind_from_str_maven() {
    use std::str::FromStr;
    assert_eq!(DepKind::from_str("maven").unwrap(), DepKind::Maven);
    assert_eq!(DepKind::from_str("Maven").unwrap(), DepKind::Maven);
    assert_eq!(DepKind::from_str("MAVEN").unwrap(), DepKind::Maven);
}

#[test]
fn test_dep_kind_from_str_npm() {
    use std::str::FromStr;
    assert_eq!(DepKind::from_str("npm").unwrap(), DepKind::Npm);
    assert_eq!(DepKind::from_str("NPM").unwrap(), DepKind::Npm);
}

#[test]
fn test_dep_kind_from_str_pypi_aliases() {
    use std::str::FromStr;
    assert_eq!(DepKind::from_str("pypi").unwrap(), DepKind::Pypi);
    assert_eq!(DepKind::from_str("pip").unwrap(), DepKind::Pypi);
    assert_eq!(DepKind::from_str("python").unwrap(), DepKind::Pypi);
}

#[test]
fn test_dep_kind_from_str_cargo_aliases() {
    use std::str::FromStr;
    assert_eq!(DepKind::from_str("cargo").unwrap(), DepKind::Cargo);
    assert_eq!(DepKind::from_str("rust").unwrap(), DepKind::Cargo);
}

#[test]
fn test_dep_kind_from_str_conan_aliases() {
    use std::str::FromStr;
    assert_eq!(DepKind::from_str("conan").unwrap(), DepKind::Conan);
    assert_eq!(DepKind::from_str("cpp").unwrap(), DepKind::Conan);
    assert_eq!(DepKind::from_str("c++").unwrap(), DepKind::Conan);
}

#[test]
fn test_dep_kind_from_str_unknown() {
    use std::str::FromStr;
    let err = DepKind::from_str("gradle").unwrap_err();
    match err {
        DepError::UnsupportedKind(_) => {}
        _ => panic!("Expected UnsupportedKind, got {:?}", err),
    }
}

#[test]
fn test_config_parse_valid() {
    let toml_str = r#"
[nexus]
base_url = "http://nexus.example.com"
username = "admin"
password = "secret"

[repositories]
maven = "maven-releases"
npm = "npm-hosted"
pypi = "pypi-hosted"
raw = "raw-hosted"
"#;
    let config: dep_porter::config::AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.nexus.base_url, "http://nexus.example.com");
    assert_eq!(config.nexus.username, "admin");
    assert_eq!(config.repositories.maven, "maven-releases");
    assert_eq!(config.repositories.raw, "raw-hosted");
}

#[test]
fn test_config_parse_missing_nexus() {
    let toml_str = r#"
[repositories]
maven = "maven-releases"
npm = "npm-hosted"
pypi = "pypi-hosted"
raw = "raw-hosted"
"#;
    let result: Result<dep_porter::config::AppConfig, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}

#[test]
fn test_config_parse_missing_repositories() {
    let toml_str = r#"
[nexus]
base_url = "http://nexus.example.com"
username = "admin"
password = "secret"
"#;
    let result: Result<dep_porter::config::AppConfig, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}

#[test]
fn test_config_validate_empty_base_url() {
    let toml_str = r#"
[nexus]
base_url = ""
username = "admin"
password = "secret"

[repositories]
maven = "maven-releases"
npm = "npm-hosted"
pypi = "pypi-hosted"
raw = "raw-hosted"
"#;
    let config: dep_porter::config::AppConfig = toml::from_str(toml_str).unwrap();
    let err = config.validate().unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("base_url"));
}

#[test]
fn test_config_validate_empty_username() {
    let toml_str = r#"
[nexus]
base_url = "http://nexus.example.com"
username = ""
password = "secret"

[repositories]
maven = "maven-releases"
npm = "npm-hosted"
pypi = "pypi-hosted"
raw = "raw-hosted"
"#;
    let config: dep_porter::config::AppConfig = toml::from_str(toml_str).unwrap();
    let err = config.validate().unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("username"));
}

#[test]
fn test_download_dir_not_found() {
    let dir = Path::new("/nonexistent/path/that/does/not/exist");
    let result = dep_porter::util::collect_files(dir);
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        DepError::DownloadDirNotFound(_) => {}
        _ => panic!("Expected DownloadDirNotFound, got {:?}", err),
    }
}

#[test]
fn test_collect_files_sorted() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path();

    // Create files in reverse order
    fs::write(base.join("c.txt"), "c").unwrap();
    fs::write(base.join("a.txt"), "a").unwrap();
    fs::write(base.join("b.txt"), "b").unwrap();

    let files = dep_porter::util::collect_files_sorted(base).unwrap();
    let names: Vec<_> = files.iter().map(|f| f.file_name().unwrap().to_str().unwrap()).collect();
    assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"]);
}

#[test]
fn test_relative_path() {
    let base = Path::new("/foo/bar");
    let file = Path::new("/foo/bar/baz/qux.txt");
    let rel = dep_porter::util::relative_path(base, file).unwrap();
    assert_eq!(rel, Path::new("baz/qux.txt"));
}

#[test]
fn test_relative_path_not_relative() {
    let base = Path::new("/foo/bar");
    let file = Path::new("/other/path.txt");
    let rel = dep_porter::util::relative_path(base, file);
    assert!(rel.is_none());
}

// ── Docker E2E tests (opt-in: RUN_DOCKER_E2E=1) ─────────────────────

fn docker_e2e_enabled() -> bool {
    std::env::var("RUN_DOCKER_E2E").unwrap_or_default() == "1"
}

#[test]
fn test_docker_build_image() {
    if !docker_e2e_enabled() {
        eprintln!("Skipping Docker E2E test (set RUN_DOCKER_E2E=1 to enable)");
        return;
    }

    let project_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    dep_porter::docker::build_image(project_dir).expect("docker build should succeed");
}

#[test]
fn test_docker_download_maven() {
    if !docker_e2e_enabled() {
        eprintln!("Skipping Docker E2E test (set RUN_DOCKER_E2E=1 to enable)");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let spec = dep_porter::model::DepSpec::new(DepKind::Maven, "junit:junit".to_string(), "4.13.2".to_string());
    dep_porter::docker::run_downloader(&spec, tmp.path()).expect("docker run should succeed");

    let repo_dir = tmp.path().join("repository");
    assert!(repo_dir.exists(), "repository/ directory should exist");
    let files = dep_porter::util::collect_files(&repo_dir).unwrap();
    assert!(!files.is_empty(), "repository/ should not be empty");
}

#[test]
fn test_docker_download_npm() {
    if !docker_e2e_enabled() {
        eprintln!("Skipping Docker E2E test (set RUN_DOCKER_E2E=1 to enable)");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let spec = dep_porter::model::DepSpec::new(DepKind::Npm, "lodash".to_string(), "4.17.21".to_string());
    dep_porter::docker::run_downloader(&spec, tmp.path()).expect("docker run should succeed");

    let files = dep_porter::util::collect_files(tmp.path()).unwrap();
    assert!(!files.is_empty(), "output directory should not be empty");
}

#[test]
fn test_docker_download_pypi() {
    if !docker_e2e_enabled() {
        eprintln!("Skipping Docker E2E test (set RUN_DOCKER_E2E=1 to enable)");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let spec = dep_porter::model::DepSpec::new(DepKind::Pypi, "requests".to_string(), "2.32.3".to_string());
    dep_porter::docker::run_downloader(&spec, tmp.path()).expect("docker run should succeed");

    let packages_dir = tmp.path().join("packages");
    assert!(packages_dir.exists(), "packages/ directory should exist");
    let files = dep_porter::util::collect_files(&packages_dir).unwrap();
    assert!(!files.is_empty(), "packages/ should not be empty");
}

// ── Nexus E2E tests (opt-in: RUN_NEXUS_E2E=1) ───────────────────────

fn nexus_e2e_config() -> Option<dep_porter::config::AppConfig> {
    if std::env::var("RUN_NEXUS_E2E").unwrap_or_default() != "1" {
        return None;
    }

    let base_url = std::env::var("NEXUS_BASE_URL").ok()?;
    let username = std::env::var("NEXUS_USERNAME").ok()?;
    let password = std::env::var("NEXUS_PASSWORD").ok()?;
    let maven_repo = std::env::var("NEXUS_MAVEN_REPO").unwrap_or_else(|_| "maven-releases".to_string());
    let raw_repo = std::env::var("NEXUS_RAW_REPO").unwrap_or_else(|_| "raw-hosted".to_string());

    Some(dep_porter::config::AppConfig {
        nexus: dep_porter::config::NexusConfig {
            base_url,
            username,
            password,
        },
        repositories: dep_porter::config::RepositoryConfig {
            maven: maven_repo,
            npm: "npm-hosted".to_string(),
            pypi: "pypi-hosted".to_string(),
            raw: raw_repo,
        },
    })
}

#[test]
fn test_nexus_upload_raw_file() {
    let config = match nexus_e2e_config() {
        Some(c) => c,
        None => {
            eprintln!("Skipping Nexus E2E test (set RUN_NEXUS_E2E=1 and NEXUS_* env vars to enable)");
            return;
        }
    };

    let tmp = tempfile::tempdir().unwrap();
    let test_file = tmp.path().join("test-upload.txt");
    fs::write(&test_file, "dep-porter test upload").unwrap();

    let spec = dep_porter::model::DepSpec::new(DepKind::Cargo, "test-pkg".to_string(), "0.1.0".to_string());
    dep_porter::import::import_to_nexus(&spec, tmp.path(), &config)
        .expect("Nexus raw upload should succeed");
}
