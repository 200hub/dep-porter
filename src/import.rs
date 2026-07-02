use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use log::{info, warn};

use crate::config::AppConfig;
use crate::model::{DepError, DepKind, DepSpec, MavenCoordinate};
use crate::util::{collect_files_sorted, relative_path};

/// Import a downloaded dependency into the Nexus repository.
pub fn import_to_nexus(spec: &DepSpec, download_dir: &Path, config: &AppConfig) -> Result<()> {
    if !download_dir.exists() {
        return Err(
            DepError::DownloadDirNotFound(download_dir.display().to_string()).into()
        );
    }

    match spec.kind {
        DepKind::Maven => import_maven(spec, download_dir, config),
        DepKind::Npm => import_npm(spec, download_dir, config),
        DepKind::Pypi => import_pypi(spec, download_dir, config),
        DepKind::Cargo => import_raw(spec, download_dir, config, "cargo"),
        DepKind::Conan => import_raw(spec, download_dir, config, "conan"),
    }
}

/// Import Maven artifacts by uploading each file in the local repository
/// to the corresponding path in the Nexus Maven repository.
fn import_maven(spec: &DepSpec, download_dir: &Path, config: &AppConfig) -> Result<()> {
    let coord = MavenCoordinate::parse(&spec.name)?;
    let repo_dir = download_dir.join("repository");

    if !repo_dir.exists() {
        return Err(DepError::DownloadDirNotFound(format!(
            "{} (expected 'repository/' subdirectory)",
            repo_dir.display()
        ))
        .into());
    }

    let base_path = repo_dir.join(coord.group_path()).join(&coord.artifact_id).join(&spec.version);

    if !base_path.exists() {
        return Err(DepError::DownloadDirNotFound(format!(
            "{}",
            base_path.display()
        ))
        .into());
    }

    let files = collect_files_sorted(&base_path)?;
    if files.is_empty() {
        return Err(
            DepError::DownloadDirEmpty(base_path.display().to_string()).into()
        );
    }

    let client = reqwest::blocking::Client::new();
    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let repo_name = &config.repositories.maven;

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
fn import_npm(spec: &DepSpec, download_dir: &Path, config: &AppConfig) -> Result<()> {
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

/// Import files into a Nexus raw repository as a fallback for unsupported types.
fn import_raw(
    spec: &DepSpec,
    download_dir: &Path,
    config: &AppConfig,
    kind_prefix: &str,
) -> Result<()> {
    let files = collect_files_sorted(download_dir)?;
    if files.is_empty() {
        return Err(
            DepError::DownloadDirEmpty(download_dir.display().to_string()).into()
        );
    }

    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let repo_name = &config.repositories.raw;
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

        info!("Uploading raw: {}", url);
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
        "{} import complete: {} file(s) uploaded for {}@{} (via raw repository)",
        kind_prefix,
        files.len(),
        spec.name,
        spec.version
    );
    Ok(())
}
