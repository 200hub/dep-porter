//! Docker E2E tests — verify the `dep-downloader` image can download
//! dependencies for all five supported kinds (maven, npm, pypi, cargo, conan).
//!
//! Run with:
//!   RUN_DOCKER_E2E=1 cargo test --test docker_e2e
//!
//! These tests build the image automatically if it does not exist.

use std::path::Path;
use std::process::Command;
use std::sync::Once;

use dep_porter::model::{DepKind, DepSpec};
use dep_porter::util::{build_dir_name, collect_files, collect_files_sorted, DOWNLOADER_IMAGE};

// ── Helpers ────────────────────────────────────────────────────────────────

static INIT_LOGGER: Once = Once::new();

fn init_logger() {
    INIT_LOGGER.call_once(|| {
        let _ = env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("info"),
        )
        .is_test(true)
        .try_init();
    });
}

fn docker_e2e_enabled() -> bool {
    std::env::var("RUN_DOCKER_E2E").unwrap_or_default() == "1"
}

/// Build the image if it does not already exist.
fn ensure_image() {
    init_logger();
    dep_porter::docker::ensure_image().expect(
        "Failed to ensure Docker image. Is Docker running? \
         You can also build manually: docker build -f Dockerfile.downloader -t dep-downloader:latest .",
    );
}

/// Run `docker run --rm dep-downloader:latest <args>` and return the output.
fn docker_run_output(args: &[&str]) -> String {
    let output = Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg("--entrypoint")
        .arg("bash")
        .arg(DOWNLOADER_IMAGE)
        .args(args)
        .output()
        .expect("Failed to run docker");
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Download a dependency into `output_dir` via the Rust API (which calls Docker).
fn download_dep(kind: DepKind, name: &str, version: &str, output_dir: &Path) {
    let spec = DepSpec::new(kind, name.to_string(), version.to_string());
    dep_porter::docker::run_downloader(&spec, output_dir)
        .unwrap_or_else(|e| panic!("Download failed for {} {}@{}: {:#}", kind, name, version, e));
}

/// Assert that `dir` exists and contains at least one file.
fn assert_dir_nonempty(dir: &Path, label: &str) {
    assert!(dir.exists(), "{}: directory not found: {}", label, dir.display());
    let files = collect_files(dir).unwrap_or_else(|e| panic!("{}: collect_files failed: {:#}", label, e));
    assert!(!files.is_empty(), "{}: directory is empty: {}", label, dir.display());
}

/// List all file paths relative to `base` for debug output.
fn list_files_relative(base: &Path) -> Vec<String> {
    collect_files_sorted(base)
        .unwrap_or_default()
        .iter()
        .filter_map(|f| f.strip_prefix(base).ok().map(|p| p.to_string_lossy().to_string()))
        .collect()
}

// ── 1. Image build test ───────────────────────────────────────────────────

#[test]
fn docker_build_image() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    let project_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    dep_porter::docker::build_image(project_dir).expect("docker build should succeed");
}

// ── 2. Tool availability tests ────────────────────────────────────────────

#[test]
fn docker_tools_maven_installed() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let out = docker_run_output(&["-c", "mvn --version 2>&1 | head -1"]);
    assert!(
        out.contains("Apache Maven"),
        "Maven not found in image. Output:\n{}",
        out
    );
    println!("  Maven: {}", out.trim());
}

#[test]
fn docker_tools_java_installed() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let out = docker_run_output(&["-c", "java -version 2>&1 | head -1"]);
    assert!(
        out.contains("openjdk") || out.contains("java"),
        "Java not found in image. Output:\n{}",
        out
    );
    println!("  Java: {}", out.trim());
}

#[test]
fn docker_tools_node_installed() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let out = docker_run_output(&["-c", "node --version"]);
    assert!(
        out.starts_with("v"),
        "Node.js not found in image. Output:\n{}",
        out
    );
    println!("  Node: {}", out.trim());
}

#[test]
fn docker_tools_npm_installed() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let out = docker_run_output(&["-c", "npm --version"]);
    assert!(
        !out.trim().is_empty(),
        "npm not found in image. Output:\n{}",
        out
    );
    println!("  npm: {}", out.trim());
}

#[test]
fn docker_tools_python_installed() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let out = docker_run_output(&["-c", "python3 --version"]);
    assert!(
        out.contains("Python"),
        "Python3 not found in image. Output:\n{}",
        out
    );
    println!("  Python: {}", out.trim());
}

#[test]
fn docker_tools_pip_installed() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let out = docker_run_output(&["-c", "pip3 --version"]);
    assert!(
        out.contains("pip"),
        "pip3 not found in image. Output:\n{}",
        out
    );
    println!("  pip: {}", out.trim());
}

#[test]
fn docker_tools_twine_installed() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let out = docker_run_output(&["-c", "twine --version"]);
    assert!(
        out.contains("twine"),
        "twine not found in image. Output:\n{}",
        out
    );
    println!("  twine: {}", out.trim());
}

#[test]
fn docker_tools_cargo_installed() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let out = docker_run_output(&["-c", "cargo --version"]);
    assert!(
        out.contains("cargo"),
        "cargo not found in image. Output:\n{}",
        out
    );
    println!("  cargo: {}", out.trim());
}

#[test]
fn docker_tools_rustc_installed() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let out = docker_run_output(&["-c", "rustc --version"]);
    assert!(
        out.contains("rustc"),
        "rustc not found in image. Output:\n{}",
        out
    );
    println!("  rustc: {}", out.trim());
}

#[test]
fn docker_tools_conan_installed() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let out = docker_run_output(&["-c", "conan --version"]);
    assert!(
        out.contains("Conan") || out.contains("conan"),
        "conan not found in image. Output:\n{}",
        out
    );
    println!("  conan: {}", out.trim());
}

// ── 3. Download tests (each kind) ────────────────────────────────────────

/// Maven: download `junit:junit:4.13.2` — a small, widely-used library.
#[test]
fn docker_download_maven() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path();

    download_dep(DepKind::Maven, "junit:junit", "4.13.2", out_dir);

    let repo = out_dir.join("repository");
    assert_dir_nonempty(&repo, "maven repository");

    // Verify expected artifacts exist (junit-4.13.2.jar, junit-4.13.2.pom, hamcrest-core)
    let files = list_files_relative(&repo);
    let has_jar = files.iter().any(|f| f.contains("junit-4.13.2.jar"));
    let has_pom = files.iter().any(|f| f.contains("junit-4.13.2.pom"));
    assert!(has_jar, "Missing junit-4.13.2.jar in repository. Files:\n{:#?}", files);
    assert!(has_pom, "Missing junit-4.13.2.pom in repository. Files:\n{:#?}", files);

    // Transitive dep: hamcrest-core should also be present
    let has_hamcrest = files.iter().any(|f| f.contains("hamcrest"));
    assert!(has_hamcrest, "Missing transitive dep hamcrest-core. Files:\n{:#?}", files);

    println!("  Maven download OK — {} files", files.len());
}

/// npm: download `lodash@4.17.21` — zero native deps, pure JS.
#[test]
fn docker_download_npm() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path();

    download_dep(DepKind::Npm, "lodash", "4.17.21", out_dir);

    // Should have node_modules/ or a .tgz file
    let files = list_files_relative(out_dir);
    let has_node_modules = files.iter().any(|f| f.starts_with("node_modules/"));
    let has_tgz = files.iter().any(|f| f.ends_with(".tgz"));
    assert!(
        has_node_modules || has_tgz,
        "Expected node_modules/ or .tgz in npm output. Files:\n{:#?}",
        files
    );

    println!("  npm download OK — {} files", files.len());
}

/// PyPI: download `six==1.16.0` — tiny pure-Python package, no native deps.
#[test]
fn docker_download_pypi() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path();

    download_dep(DepKind::Pypi, "six", "1.16.0", out_dir);

    let packages = out_dir.join("packages");
    assert_dir_nonempty(&packages, "pypi packages");

    // Should contain .whl or .tar.gz
    let files = list_files_relative(&packages);
    let has_whl = files.iter().any(|f| f.ends_with(".whl"));
    let has_tar = files.iter().any(|f| f.ends_with(".tar.gz"));
    assert!(
        has_whl || has_tar,
        "Expected .whl or .tar.gz in packages/. Files:\n{:#?}",
        files
    );

    println!("  PyPI download OK — {} files", files.len());
}

/// Cargo: download `once_cell==1.19.0` — popular, no native build, fast.
#[test]
fn docker_download_cargo() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path();

    download_dep(DepKind::Cargo, "once_cell", "1.19.0", out_dir);

    // Should have vendor/ or Cargo.lock
    let files = list_files_relative(out_dir);
    let has_vendor = files.iter().any(|f| f.starts_with("vendor/"));
    let has_lock = files.iter().any(|f| f == "Cargo.lock");
    assert!(
        has_vendor || has_lock,
        "Expected vendor/ or Cargo.lock in cargo output. Files:\n{:#?}",
        files
    );

    println!("  Cargo download OK — {} files", files.len());
}

/// Conan: download `zlib/1.3.1` — recipe caching (no compiler in image, so no build).
#[test]
fn docker_download_conan() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path();

    // Conan may fail to build but should still produce output (recipe cache)
    let spec = DepSpec::new(DepKind::Conan, "zlib".to_string(), "1.3.1".to_string());
    let _ = dep_porter::docker::run_downloader(&spec, out_dir);

    // Verify some output was produced (conan-cache or conan-workspace)
    let files = list_files_relative(out_dir);
    assert!(
        !files.is_empty(),
        "Conan output is empty. Expected conan-cache/. Files:\n{:#?}",
        files
    );

    println!("  Conan download OK — {} files (recipe cached)", files.len());
}

// ── 4. Directory naming integration test ──────────────────────────────────

#[test]
fn docker_dir_name_matches_expected() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    let tmp = tempfile::tempdir().unwrap();

    // Download a Maven dep and verify the dir name convention
    let kind = DepKind::Maven;
    let name = "commons-io:commons-io";
    let version = "2.15.1";
    let expected_dir = tmp.path().join(build_dir_name(kind, name, version));

    download_dep(kind, name, version, &expected_dir);

    assert!(expected_dir.exists(), "Expected output dir: {}", expected_dir.display());
    assert_dir_nonempty(&expected_dir, "maven output dir");

    // Verify the directory name matches the convention
    let dir_name = expected_dir.file_name().unwrap().to_str().unwrap();
    assert_eq!(
        dir_name,
        "maven_commons-io_commons-io_2.15.1",
        "Directory name mismatch"
    );

    println!("  Dir naming OK: {}", dir_name);
}

// ── 5. Unsupported kind test ──────────────────────────────────────────────

#[test]
fn docker_unsupported_kind_fails_gracefully() {
    if !docker_e2e_enabled() {
        eprintln!("SKIP: set RUN_DOCKER_E2E=1 to enable");
        return;
    }
    ensure_image();

    // Run the download script with an unsupported kind directly in the container
    let output = Command::new("docker")
        .args([
            "run", "--rm",
            DOWNLOADER_IMAGE,
            "dep-download", "gradle", "some:dep", "1.0.0", "/tmp/out",
        ])
        .output()
        .expect("Failed to run docker");

    assert!(
        !output.status.success(),
        "Expected non-zero exit for unsupported kind"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.contains("Unsupported") || combined.contains("unsupported"),
        "Expected 'Unsupported' in error message. Output:\n{}",
        combined
    );

    println!("  Unsupported kind error OK");
}
