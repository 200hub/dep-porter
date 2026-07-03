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
    assert_eq!(config.repositories.maven_snapshots, None);
    assert_eq!(config.repositories.cargo, None);
    assert_eq!(config.repositories.raw, "raw-hosted");
}

#[test]
fn test_config_parse_with_optional_fields() {
    let toml_str = r#"
[nexus]
base_url = "http://nexus.example.com"
username = "admin"
password = "secret"

[repositories]
maven = "maven-releases"
maven_snapshots = "maven-snapshots"
npm = "npm-hosted"
pypi = "pypi-hosted"
cargo = "cargo-hosted"
raw = "raw-hosted"
"#;
    let config: dep_porter::config::AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.repositories.maven_snapshots.as_deref(),
        Some("maven-snapshots")
    );
    assert_eq!(config.repositories.cargo.as_deref(), Some("cargo-hosted"));
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
    let names: Vec<_> = files
        .iter()
        .map(|f| f.file_name().unwrap().to_str().unwrap())
        .collect();
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
fn test_docker_pull_image() {
    if !docker_e2e_enabled() {
        eprintln!("Skipping Docker E2E test (set RUN_DOCKER_E2E=1 to enable)");
        return;
    }

    dep_porter::docker::pull_image().expect("docker pull should succeed");
}

#[test]
fn test_docker_download_maven() {
    if !docker_e2e_enabled() {
        eprintln!("Skipping Docker E2E test (set RUN_DOCKER_E2E=1 to enable)");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let spec = dep_porter::model::DepSpec::new(
        DepKind::Maven,
        "junit:junit".to_string(),
        "4.13.2".to_string(),
    );
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
    let spec =
        dep_porter::model::DepSpec::new(DepKind::Npm, "lodash".to_string(), "4.17.21".to_string());
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
    let spec = dep_porter::model::DepSpec::new(
        DepKind::Pypi,
        "requests".to_string(),
        "2.32.3".to_string(),
    );
    dep_porter::docker::run_downloader(&spec, tmp.path()).expect("docker run should succeed");

    let packages_dir = tmp.path().join("packages");
    assert!(packages_dir.exists(), "packages/ directory should exist");
    let files = dep_porter::util::collect_files(&packages_dir).unwrap();
    assert!(!files.is_empty(), "packages/ should not be empty");
}

// ── Nexus E2E tests (opt-in: RUN_NEXUS_E2E=1) ───────────────────────
//
// These tests publish artifacts to a *live* Nexus and verify that they are
// retrievable in the correct native format (i.e. actually usable by the
// corresponding package manager). They use the real `config.toml` in the
// project root by default, or `NEXUS_*` environment variables as an override.
//
// Enable with:
//   RUN_NEXUS_E2E=1 cargo test --test e2e -- --test-threads=1
//
// The default `config.toml` points at http://localhost:8081 with repositories
// named maven-releases / npm / pypi / cargo / raw.

use dep_porter::config::{AppConfig, NexusConfig, RepositoryConfig};
use dep_porter::model::DepSpec;

/// Load the Nexus configuration for E2E tests, preferring `NEXUS_*` env vars and
/// falling back to the project's `config.toml`.
fn nexus_e2e_config() -> Option<AppConfig> {
    if std::env::var("RUN_NEXUS_E2E").unwrap_or_default() != "1" {
        return None;
    }

    // Env-var override (keeps the historical behaviour working).
    if let (Ok(base_url), Ok(username), Ok(password)) = (
        std::env::var("NEXUS_BASE_URL"),
        std::env::var("NEXUS_USERNAME"),
        std::env::var("NEXUS_PASSWORD"),
    ) {
        return Some(AppConfig {
            nexus: NexusConfig {
                base_url,
                username,
                password,
            },
            repositories: RepositoryConfig {
                maven: std::env::var("NEXUS_MAVEN_REPO")
                    .unwrap_or_else(|_| "maven-releases".into()),
                maven_snapshots: std::env::var("NEXUS_MAVEN_SNAPSHOTS_REPO").ok(),
                npm: std::env::var("NEXUS_NPM_REPO").unwrap_or_else(|_| "npm".into()),
                pypi: std::env::var("NEXUS_PYPI_REPO").unwrap_or_else(|_| "pypi".into()),
                cargo: std::env::var("NEXUS_CARGO_REPO")
                    .ok()
                    .or_else(|| Some("cargo".into())),
                raw: std::env::var("NEXUS_RAW_REPO").unwrap_or_else(|_| "raw".into()),
            },
        });
    }

    // Fall back to the checked-in config.toml (a real, usable Nexus).
    let config_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("config.toml");
    match AppConfig::from_file(&config_path) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            eprintln!("RUN_NEXUS_E2E=1 but could not load config.toml: {:#}", e);
            None
        }
    }
}

/// A unique, monotonically-increasing version suffix so repeated test runs do
/// not collide with already-published artifacts.
fn unique_build_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        % 1_000_000
}

fn http() -> reqwest::blocking::Client {
    reqwest::blocking::Client::new()
}

fn get_status(cfg: &AppConfig, url: &str) -> reqwest::StatusCode {
    http()
        .get(url)
        .basic_auth(&cfg.nexus.username, Some(&cfg.nexus.password))
        .send()
        .expect("HTTP GET failed")
        .status()
}

fn get_text(cfg: &AppConfig, url: &str) -> (reqwest::StatusCode, String) {
    let resp = http()
        .get(url)
        .basic_auth(&cfg.nexus.username, Some(&cfg.nexus.password))
        .send()
        .expect("HTTP GET failed");
    let status = resp.status();
    (status, resp.text().unwrap_or_default())
}

// ── Fixture builders (produce the exact on-disk layout the importer expects) ──

/// Write a gzipped tar (`.crate` / `.tgz`) from a list of (path, contents).
fn write_targz(path: &Path, entries: &[(String, Vec<u8>)]) {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    let file = fs::File::create(path).unwrap();
    let enc = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(enc);
    for (name, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, name, data.as_slice())
            .unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap();
}

/// Build a Cargo download fixture (`crates/` + `index/`) for a synthetic crate.
/// `deps` is a list of `(dep_name, version_req)` recorded both in the crate's
/// `Cargo.toml` and in the sidecar index metadata.
fn make_cargo_crate(dir: &Path, name: &str, version: &str, deps: &[(&str, &str)]) {
    let crates = dir.join("crates");
    let index = dir.join("index");
    fs::create_dir_all(&crates).unwrap();
    fs::create_dir_all(&index).unwrap();

    let mut cargo_toml = format!(
        "[package]\nname = \"{}\"\nversion = \"{}\"\nedition = \"2021\"\ndescription = \"dep-porter e2e\"\nlicense = \"MIT\"\n",
        name, version
    );
    if !deps.is_empty() {
        cargo_toml.push_str("\n[dependencies]\n");
        for (d, req) in deps {
            cargo_toml.push_str(&format!("{} = \"{}\"\n", d, req));
        }
    }

    let top = format!("{}-{}", name, version);
    write_targz(
        &crates.join(format!("{}.crate", top)),
        &[
            (format!("{}/Cargo.toml", top), cargo_toml.into_bytes()),
            (format!("{}/src/lib.rs", top), b"// e2e\n".to_vec()),
        ],
    );

    let index_deps: Vec<serde_json::Value> = deps
        .iter()
        .map(|(d, req)| {
            serde_json::json!({
                "name": d, "req": req, "features": [],
                "optional": false, "default_features": true,
                "target": serde_json::Value::Null, "kind": "normal"
            })
        })
        .collect();
    let index_line = serde_json::json!({
        "name": name, "vers": version, "deps": index_deps,
        "cksum": "0".repeat(64), "features": {}, "yanked": false
    });
    fs::write(
        index.join(format!("{}.json", top)),
        serde_json::to_string(&index_line).unwrap(),
    )
    .unwrap();
}

/// Build an npm download fixture (`tarballs/`) for a synthetic package.
fn make_npm_tarball(dir: &Path, name: &str, version: &str, deps: &[(&str, &str)]) {
    let tarballs = dir.join("tarballs");
    fs::create_dir_all(&tarballs).unwrap();

    let mut pkg = serde_json::json!({
        "name": name,
        "version": version,
        "description": "dep-porter e2e",
        "main": "index.js",
        "license": "MIT",
    });
    if !deps.is_empty() {
        let mut map = serde_json::Map::new();
        for (d, req) in deps {
            map.insert((*d).to_string(), serde_json::json!(req));
        }
        pkg["dependencies"] = serde_json::Value::Object(map);
    }

    write_targz(
        &tarballs.join(format!("{}-{}.tgz", name, version)),
        &[
            (
                "package/package.json".to_string(),
                serde_json::to_vec(&pkg).unwrap(),
            ),
            (
                "package/index.js".to_string(),
                b"module.exports = 1;\n".to_vec(),
            ),
        ],
    );
}

/// A minimal but valid (empty) ZIP file — usable as a Maven `.jar` even when
/// strict content-type validation is enabled.
fn empty_zip() -> Vec<u8> {
    // End-of-central-directory record for an empty archive.
    vec![
        0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ]
}

/// Build a Maven download fixture (`repository/` layout) for a synthetic artifact.
fn make_maven_fixture(dir: &Path, group: &str, artifact: &str, version: &str) {
    let group_path = group.replace('.', "/");
    let art_dir = dir
        .join("repository")
        .join(&group_path)
        .join(artifact)
        .join(version);
    fs::create_dir_all(&art_dir).unwrap();

    fs::write(
        art_dir.join(format!("{}-{}.jar", artifact, version)),
        empty_zip(),
    )
    .unwrap();
    let pom = format!(
        "<project>\n<modelVersion>4.0.0</modelVersion>\n<groupId>{}</groupId>\n<artifactId>{}</artifactId>\n<version>{}</version>\n</project>\n",
        group, artifact, version
    );
    fs::write(
        art_dir.join(format!("{}-{}.pom", artifact, version)),
        pom.into_bytes(),
    )
    .unwrap();
}

// ── Cargo: publish + verify usable via the sparse index & download endpoint ──

#[test]
fn test_e2e_cargo_publish_and_resolve() {
    let cfg = match nexus_e2e_config() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: set RUN_NEXUS_E2E=1 (and provide config.toml or NEXUS_* env)");
            return;
        }
    };
    let cargo_repo = cfg
        .repositories
        .cargo
        .clone()
        .expect("config must define a cargo repository for this test");
    let base = cfg.nexus.base_url.trim_end_matches('/').to_string();

    let id = unique_build_id();
    let dep_name = format!("dpe2e-cargo-dep-{}", id);
    let root_name = format!("dpe2e-cargo-{}", id);
    let version = "0.0.1";

    let tmp = tempfile::tempdir().unwrap();
    let dl = tmp
        .path()
        .join(build_dir_name(DepKind::Cargo, &root_name, version));
    // Root crate depends on the leaf crate — verifies transitive metadata.
    make_cargo_crate(&dl, &dep_name, version, &[]);
    make_cargo_crate(
        &dl,
        &root_name,
        version,
        &[(&dep_name, &format!("={}", version))],
    );

    let spec = DepSpec::new(DepKind::Cargo, root_name.clone(), version.to_string());
    dep_porter::import::import_to_nexus(&spec, &dl, &cfg, true)
        .expect("cargo publish to Nexus should succeed");

    // Both crates must be downloadable via the Cargo download endpoint.
    for name in [&dep_name, &root_name] {
        let url = format!(
            "{}/repository/{}/crates/{}/{}/download",
            base, cargo_repo, name, version
        );
        assert!(
            get_status(&cfg, &url).is_success(),
            "crate {} must be downloadable at {}",
            name,
            url
        );
    }

    // The root crate's sparse-index entry must list the version and its dep.
    let idx_url = format!(
        "{}/repository/{}/{}",
        base,
        cargo_repo,
        dep_porter::registry::cargo_sparse_index_path(&root_name)
    );
    let (status, body) = get_text(&cfg, &idx_url);
    assert!(
        status.is_success(),
        "sparse index for {} missing",
        root_name
    );
    assert!(
        body.contains(&format!("\"vers\":\"{}\"", version)),
        "index missing version: {}",
        body
    );
    assert!(
        body.contains(&dep_name),
        "index missing transitive dependency {}: {}",
        dep_name,
        body
    );
}

// ── npm: publish + verify usable via the packument & tarball endpoint ────────

#[test]
fn test_e2e_npm_publish_and_resolve() {
    let cfg = match nexus_e2e_config() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: set RUN_NEXUS_E2E=1 (and provide config.toml or NEXUS_* env)");
            return;
        }
    };
    let npm_repo = cfg.repositories.npm.clone();
    let base = cfg.nexus.base_url.trim_end_matches('/').to_string();

    let id = unique_build_id();
    let dep_name = format!("dpe2e-npm-dep-{}", id);
    let root_name = format!("dpe2e-npm-{}", id);
    let version = "1.0.0";

    let tmp = tempfile::tempdir().unwrap();
    let dl = tmp
        .path()
        .join(build_dir_name(DepKind::Npm, &root_name, version));
    make_npm_tarball(&dl, &dep_name, version, &[]);
    make_npm_tarball(&dl, &root_name, version, &[(&dep_name, version)]);

    let spec = DepSpec::new(DepKind::Npm, root_name.clone(), version.to_string());
    dep_porter::import::import_to_nexus(&spec, &dl, &cfg, true)
        .expect("npm publish to Nexus should succeed");

    // The packument must list the version and record the dependency.
    let packument_url = format!("{}/repository/{}/{}", base, npm_repo, root_name);
    let (status, body) = get_text(&cfg, &packument_url);
    assert!(status.is_success(), "packument for {} missing", root_name);
    let doc: serde_json::Value = serde_json::from_str(&body).expect("packument is JSON");
    assert!(
        doc["versions"].get(version).is_some(),
        "packument missing version {}: {}",
        version,
        body
    );
    assert_eq!(
        doc["versions"][version]["dependencies"][&dep_name], version,
        "packument missing dependency mapping"
    );

    // The tarball recorded in dist must be downloadable.
    let tarball_url = doc["versions"][version]["dist"]["tarball"]
        .as_str()
        .expect("dist.tarball present");
    assert!(
        get_status(&cfg, tarball_url).is_success(),
        "tarball not retrievable at {}",
        tarball_url
    );
}

// ── Maven: publish + verify the artifact is retrievable ──────────────────────

#[test]
fn test_e2e_maven_publish_and_resolve() {
    let cfg = match nexus_e2e_config() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: set RUN_NEXUS_E2E=1 (and provide config.toml or NEXUS_* env)");
            return;
        }
    };
    let maven_repo = cfg.repositories.maven.clone();
    let base = cfg.nexus.base_url.trim_end_matches('/').to_string();

    let id = unique_build_id();
    let group = "io.depporter.e2e";
    let artifact = format!("probe-{}", id);
    let version = "0.0.1";

    let tmp = tempfile::tempdir().unwrap();
    let dl = tmp.path().join(build_dir_name(
        DepKind::Maven,
        &format!("{}:{}", group, artifact),
        version,
    ));
    make_maven_fixture(&dl, group, &artifact, version);

    let spec = DepSpec::new(
        DepKind::Maven,
        format!("{}:{}", group, artifact),
        version.to_string(),
    );
    dep_porter::import::import_to_nexus(&spec, &dl, &cfg, true)
        .expect("maven upload to Nexus should succeed");

    let jar_url = format!(
        "{}/repository/{}/{}/{}/{}/{}-{}.jar",
        base,
        maven_repo,
        group.replace('.', "/"),
        artifact,
        version,
        artifact,
        version
    );
    assert!(
        get_status(&cfg, &jar_url).is_success(),
        "maven jar not retrievable at {}",
        jar_url
    );
}

// ── Conan / raw fallback: publish + verify retrievable ───────────────────────

#[test]
fn test_e2e_raw_upload() {
    let cfg = match nexus_e2e_config() {
        Some(c) => c,
        None => {
            eprintln!("SKIP: set RUN_NEXUS_E2E=1 (and provide config.toml or NEXUS_* env)");
            return;
        }
    };
    let raw_repo = cfg.repositories.raw.clone();
    let base = cfg.nexus.base_url.trim_end_matches('/').to_string();

    let id = unique_build_id();
    let name = format!("zlib-{}", id);
    let version = "1.2.13";

    let tmp = tempfile::tempdir().unwrap();
    let dl = tmp
        .path()
        .join(build_dir_name(DepKind::Conan, &name, version));
    fs::create_dir_all(&dl).unwrap();
    fs::write(
        dl.join("conanfile.txt"),
        format!("[requires]\n{}/{}\n", name, version),
    )
    .unwrap();

    let spec = DepSpec::new(DepKind::Conan, name.clone(), version.to_string());
    dep_porter::import::import_to_nexus(&spec, &dl, &cfg, true)
        .expect("conan/raw upload to Nexus should succeed");

    let url = format!(
        "{}/repository/{}/conan/{}/{}/conanfile.txt",
        base, raw_repo, name, version
    );
    assert!(
        get_status(&cfg, &url).is_success(),
        "raw artifact not retrievable at {}",
        url
    );
}
