use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use log::{info, warn};

use crate::config::AppConfig;
use crate::model::{DepError, DepKind, DepSpec, MavenCoordinate};
use crate::util::{collect_files_sorted, relative_path};

/// Import a downloaded dependency into the Nexus repository.
pub fn import_to_nexus(
    spec: &DepSpec,
    download_dir: &Path,
    config: &AppConfig,
    overwrite: bool,
) -> Result<()> {
    if !download_dir.exists() {
        return Err(
            DepError::DownloadDirNotFound(download_dir.display().to_string()).into()
        );
    }

    match spec.kind {
        DepKind::Maven => import_maven(spec, download_dir, config, overwrite),
        DepKind::Npm => import_npm(spec, download_dir, config, overwrite),
        DepKind::Pypi => import_pypi(spec, download_dir, config),
        DepKind::Cargo => import_cargo(spec, download_dir, config, overwrite),
        DepKind::Conan => import_raw(spec, download_dir, config, "conan", overwrite),
    }
}

/// Import Maven artifacts by uploading each file in the local repository
/// to the corresponding path in the Nexus Maven repository.
fn import_maven(spec: &DepSpec, download_dir: &Path, config: &AppConfig, overwrite: bool) -> Result<()> {
    let coord = MavenCoordinate::parse(&spec.name)?;
    let repo_dir = download_dir.join("repository");

    if !repo_dir.exists() {
        return Err(DepError::DownloadDirNotFound(format!(
            "{} (expected 'repository/' subdirectory)",
            repo_dir.display()
        ))
        .into());
    }

    // Verify the target artifact exists
    let artifact_path = repo_dir.join(coord.group_path()).join(&coord.artifact_id).join(&spec.version);
    if !artifact_path.exists() {
        return Err(DepError::DownloadDirNotFound(format!(
            "{}",
            artifact_path.display()
        ))
        .into());
    }

    // Upload ALL files in the repository (including transitive dependencies)
    let files = collect_files_sorted(&repo_dir)?;
    if files.is_empty() {
        return Err(
            DepError::DownloadDirEmpty(repo_dir.display().to_string()).into()
        );
    }

    // Filter out Maven metadata files (not artifacts, Nexus rejects them)
    let files: Vec<_> = files
        .into_iter()
        .filter(|f| {
            let name = f.file_name().unwrap_or_default().to_string_lossy();
            name != "_remote.repositories"
                && !name.starts_with("maven-metadata-")
                && name != "maven-metadata.xml"
        })
        .collect();

    info!("Uploading {} files (including transitive dependencies)", files.len());

    let client = reqwest::blocking::Client::new();
    let nexus_base = config.nexus.base_url.trim_end_matches('/');

    // Route: -SNAPSHOT → maven_snapshots repo, otherwise → maven (releases)
    let is_snapshot = spec.version.to_uppercase().contains("-SNAPSHOT");
    let repo_name = if is_snapshot {
        config.repositories.maven_snapshots.as_deref()
            .unwrap_or(&config.repositories.maven)
    } else {
        &config.repositories.maven
    };
    if is_snapshot {
        info!("SNAPSHOT version detected, uploading to '{}'", repo_name);
    }

    for file in &files {
        let full_rel = file
            .strip_prefix(&repo_dir)
            .context("Failed to compute relative path")?
            .to_string_lossy()
            .replace('\\', "/");
        let url = format!(
            "{}/repository/{}/{}",
            nexus_base, repo_name, full_rel
        );

        // If not overwriting, check if artifact already exists (HEAD)
        if !overwrite {
            let head = client
                .head(&url)
                .basic_auth(&config.nexus.username, Some(&config.nexus.password))
                .send();
            if let Ok(resp) = head {
                if resp.status().is_success() {
                    info!("Skipping (already exists): {}", url);
                    continue;
                }
            }
        }

        info!("Uploading: {}", url);
        let content = fs::read(file)
            .with_context(|| format!("Failed to read {}", file.display()))?;

        let resp = client
            .put(&url)
            .basic_auth(&config.nexus.username, Some(&config.nexus.password))
            .body(content)
            .send()
            .with_context(|| format!("Failed to PUT {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(DepError::NexusUploadFailed {
                url: url.clone(),
                status: status.as_u16(),
            }
            .into());
        }
        info!("  -> {}", status);
    }

    info!(
        "Maven import complete: {} files uploaded for {}:{}:{}",
        files.len(),
        coord.group_id,
        coord.artifact_id,
        spec.version
    );
    Ok(())
}

/// Import npm packages by publishing to the Nexus npm repository.
///
/// If the download directory contains a standard npm package structure
/// (package.json + tarball), we attempt `npm publish`.
/// Otherwise, we fall back to uploading the tarball to the raw repository.
fn import_npm(spec: &DepSpec, download_dir: &Path, config: &AppConfig, overwrite: bool) -> Result<()> {
    // Look for .tgz files in the download directory (npm pack output or cached tarballs)
    let files = collect_files_sorted(download_dir)?;
    let tgz_files: Vec<_> = files
        .iter()
        .filter(|f| {
            f.extension()
                .map(|e| e == "tgz" || e == "gz")
                .unwrap_or(false)
        })
        .collect();

    if tgz_files.is_empty() {
        warn!(
            "No .tgz files found in {}. npm import requires a package tarball.",
            download_dir.display()
        );
        return Err(DepError::DownloadDirEmpty(format!(
            "No .tgz files in {}",
            download_dir.display()
        ))
        .into());
    }

    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let repo_name = &config.repositories.npm;
    let client = reqwest::blocking::Client::new();

    for file in &tgz_files {
        let filename = file
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let url = format!(
            "{}/repository/{}/{}",
            nexus_base, repo_name, filename
        );

        if !overwrite {
            let head = client
                .head(&url)
                .basic_auth(&config.nexus.username, Some(&config.nexus.password))
                .send();
            if let Ok(resp) = head {
                if resp.status().is_success() {
                    info!("Skipping (already exists): {}", url);
                    continue;
                }
            }
        }

        info!("Uploading npm tarball: {}", url);
        let content = fs::read(file)
            .with_context(|| format!("Failed to read {}", file.display()))?;

        let resp = client
            .put(&url)
            .basic_auth(&config.nexus.username, Some(&config.nexus.password))
            .header("Content-Type", "application/octet-stream")
            .body(content)
            .send()
            .with_context(|| format!("Failed to PUT {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(DepError::NexusUploadFailed {
                url: url.clone(),
                status: status.as_u16(),
            }
            .into());
        }
        info!("  -> {}", status);
    }

    info!(
        "npm import complete: {} tarball(s) uploaded for {}@{}",
        tgz_files.len(),
        spec.name,
        spec.version
    );
    Ok(())
}

/// Import PyPI packages using `twine upload`.
fn import_pypi(spec: &DepSpec, download_dir: &Path, config: &AppConfig) -> Result<()> {
    let packages_dir = download_dir.join("packages");
    if !packages_dir.exists() {
        return Err(DepError::DownloadDirNotFound(format!(
            "{} (expected 'packages/' subdirectory)",
            packages_dir.display()
        ))
        .into());
    }

    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let repo_name = &config.repositories.pypi;
    let repo_url = format!("{}/repository/{}", nexus_base, repo_name);

    info!(
        "Uploading PyPI packages from {} to {}",
        packages_dir.display(),
        repo_url
    );

    let status = std::process::Command::new("twine")
        .args([
            "upload",
            "--repository-url",
            &repo_url,
            "-u",
            &config.nexus.username,
            "-p",
            &config.nexus.password,
            &format!("{}/*", packages_dir.display()),
        ])
        .status()
        .context(
            "Failed to execute twine. Make sure twine is installed: pip install twine",
        )?;

    if !status.success() {
        return Err(DepError::DockerCommandFailed(format!(
            "twine upload exited with status: {}",
            status
        ))
        .into());
    }

    info!(
        "PyPI import complete for {}=={}",
        spec.name, spec.version
    );
    Ok(())
}

/// Import Cargo crates into Nexus.
///
/// Strategy:
///   1. If `crates/` dir exists (generated by download.sh), upload `.crate` files
///      to the cargo repository via Nexus cargo API (`/{name}/{version}/download`).
///   2. If cargo repo fails or not configured, fall back to uploading `vendor/` to raw.
fn import_cargo(spec: &DepSpec, download_dir: &Path, config: &AppConfig, overwrite: bool) -> Result<()> {
    let crates_dir = download_dir.join("crates");

    if crates_dir.exists() {
        // Upload .crate files to cargo repository
        if let Some(cargo_repo) = &config.repositories.cargo {
            info!("Uploading .crate files to cargo repository '{}'", cargo_repo);
            match upload_crate_files(spec, &crates_dir, config, cargo_repo, overwrite) {
                Ok(()) => {
                    info!("Cargo import complete via '{}'", cargo_repo);
                    return Ok(());
                }
                Err(e) => {
                    warn!("Cargo repo '{}' failed ({}), falling back to raw", cargo_repo, e);
                }
            }
        }
    } else {
        info!("No crates/ directory found, falling back to vendor/ upload to raw");
    }

    // Fallback: upload vendor/ to raw repository
    info!("Using raw repository as fallback for cargo");
    import_raw(spec, download_dir, config, "cargo", overwrite)
}

/// Upload .crate files to a Nexus cargo repository via direct PUT.
///
/// URL format: PUT /repository/{repo}/{name}/{version}/{name}-{version}.crate
fn upload_crate_files(
    _spec: &DepSpec,
    crates_dir: &Path,
    config: &AppConfig,
    repo_name: &str,
    overwrite: bool,
) -> Result<()> {
    let files = collect_files_sorted(crates_dir)?;
    if files.is_empty() {
        return Err(DepError::DownloadDirEmpty(crates_dir.display().to_string()).into());
    }

    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let client = reqwest::blocking::Client::new();
    let mut uploaded = 0u32;

    for file in &files {
        let filename = file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let crate_name_version = match filename.strip_suffix(".crate") {
            Some(s) => s.to_string(),
            None => {
                info!("Skipping non-crate file: {}", filename);
                continue;
            }
        };

        let (crate_name, crate_version) = match crate_name_version.rsplit_once('-') {
            Some((name, ver)) => (name.to_string(), ver.to_string()),
            None => {
                info!("Skipping malformed crate filename: {}", filename);
                continue;
            }
        };

        let url = format!(
            "{}/repository/{}/{}/{}/{}",
            nexus_base, repo_name, crate_name, crate_version, filename
        );

        // Skip-if-exists check
        if !overwrite {
            let head = client
                .head(&url)
                .basic_auth(&config.nexus.username, Some(&config.nexus.password))
                .send();
            if let Ok(resp) = head {
                if resp.status().is_success() {
                    info!("Skipping (already exists): {} {}", crate_name, crate_version);
                    continue;
                }
            }
        }

        info!("Uploading crate: {} {}", crate_name, crate_version);
        let content = fs::read(file)
            .with_context(|| format!("Failed to read {}", file.display()))?;

        let resp = client
            .put(&url)
            .basic_auth(&config.nexus.username, Some(&config.nexus.password))
            .header("Content-Type", "application/octet-stream")
            .body(content)
            .send()
            .with_context(|| format!("Failed to PUT {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(DepError::NexusUploadFailed {
                url: url.clone(),
                status: status.as_u16(),
            }
            .into());
        }
        info!("  -> {}", status);
        uploaded += 1;
    }

    if uploaded == 0 {
        anyhow::bail!("No .crate files found in {}", crates_dir.display());
    }

    Ok(())
}

/// Upload all files under `download_dir` to a named Nexus repository.
///
/// `kind_prefix` is used in the URL path (e.g. `cargo/serde/1.0.0/...`).
fn upload_files_to_repo(
    spec: &DepSpec,
    download_dir: &Path,
    config: &AppConfig,
    repo_name: &str,
    kind_prefix: &str,
    overwrite: bool,
) -> Result<()> {
    let files = collect_files_sorted(download_dir)?;
    if files.is_empty() {
        return Err(
            DepError::DownloadDirEmpty(download_dir.display().to_string()).into()
        );
    }

    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let client = reqwest::blocking::Client::new();

    for file in &files {
        let rel = relative_path(download_dir, file)
            .context("Failed to compute relative path")?
            .to_string_lossy()
            .replace('\\', "/");

        let url = format!(
            "{}/repository/{}/{}/{}/{}/{}",
            nexus_base, repo_name, kind_prefix, spec.name, spec.version, rel
        );

        if !overwrite {
            let head = client
                .head(&url)
                .basic_auth(&config.nexus.username, Some(&config.nexus.password))
                .send();
            if let Ok(resp) = head {
                if resp.status().is_success() {
                    info!("Skipping (already exists): {}", url);
                    continue;
                }
            }
        }

        info!("Uploading: {}", url);
        let content = fs::read(file)
            .with_context(|| format!("Failed to read {}", file.display()))?;

        let resp = client
            .put(&url)
            .basic_auth(&config.nexus.username, Some(&config.nexus.password))
            .header("Content-Type", "application/octet-stream")
            .body(content)
            .send()
            .with_context(|| format!("Failed to PUT {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(DepError::NexusUploadFailed {
                url: url.clone(),
                status: status.as_u16(),
            }
            .into());
        }
        info!("  -> {}", status);
    }

    Ok(())
}

/// Import files into a Nexus raw repository as a fallback for unsupported types.
fn import_raw(
    spec: &DepSpec,
    download_dir: &Path,
    config: &AppConfig,
    kind_prefix: &str,
    overwrite: bool,
) -> Result<()> {
    let repo_name = &config.repositories.raw;
    upload_files_to_repo(spec, download_dir, config, repo_name, kind_prefix, overwrite)?;

    info!(
        "{} import complete for {}@{} (via raw repository '{}')",
        kind_prefix, spec.name, spec.version, repo_name
    );
    Ok(())
}
