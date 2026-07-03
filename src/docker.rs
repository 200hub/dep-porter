use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use log::{info, warn};

use crate::model::{DepError, DepSpec};
use crate::util::{to_docker_mount_path, DOWNLOADER_IMAGE};

/// Check that Docker is installed and available in PATH.
pub fn ensure_docker_installed() -> Result<()> {
    which::which("docker").map_err(|_| DepError::DockerNotFound)?;
    Ok(())
}

/// Check whether the downloader image exists locally.
pub fn image_exists() -> bool {
    Command::new("docker")
        .args(["image", "inspect", DOWNLOADER_IMAGE])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Locate the `Dockerfile.downloader` on disk.
///
/// Search order:
///   1. Current working directory
///   2. Parent of the current executable
///   3. Walk up from cwd looking for it
fn find_dockerfile() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = {
        let mut v = Vec::new();
        if let Ok(cwd) = env::current_dir() {
            v.push(cwd.join("Dockerfile.downloader"));
        }
        if let Ok(exe) = env::current_exe() {
            if let Some(parent) = exe.parent() {
                v.push(parent.join("Dockerfile.downloader"));
            }
        }
        v
    };

    for c in &candidates {
        if c.is_file() {
            return Some(c.clone());
        }
    }

    // Walk up from cwd
    if let Ok(mut dir) = env::current_dir() {
        loop {
            let candidate = dir.join("Dockerfile.downloader");
            if candidate.is_file() {
                return Some(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
    }

    None
}

/// Build the downloader Docker image from the Dockerfile.downloader.
///
/// Searches for `Dockerfile.downloader` in the current directory, the
/// executable's parent, and walking up the directory tree.
pub fn build_image(dockerfile_dir: &Path) -> Result<()> {
    let dockerfile = dockerfile_dir.join("Dockerfile.downloader");
    if !dockerfile.is_file() {
        return Err(anyhow::anyhow!(
            "Dockerfile.downloader not found in {}",
            dockerfile_dir.display()
        ));
    }

    info!(
        "Building Docker image {} from {} ...",
        DOWNLOADER_IMAGE,
        dockerfile.display()
    );

    let status = Command::new("docker")
        .args([
            "build",
            "-f",
            &dockerfile.to_string_lossy(),
            "-t",
            DOWNLOADER_IMAGE,
            ".",
        ])
        .current_dir(dockerfile_dir)
        .status()
        .context("Failed to execute docker build")?;

    if !status.success() {
        return Err(
            DepError::DockerCommandFailed("docker build failed".to_string()).into()
        );
    }

    info!("Docker image {} built successfully.", DOWNLOADER_IMAGE);
    Ok(())
}

/// Ensure the downloader image is available.  If it does not exist,
/// attempt to locate `Dockerfile.downloader` and build it automatically.
pub fn ensure_image() -> Result<()> {
    ensure_docker_installed()?;

    if image_exists() {
        return Ok(());
    }

    warn!(
        "Docker image '{}' not found. Attempting automatic build...",
        DOWNLOADER_IMAGE
    );

    let dockerfile_dir = find_dockerfile().ok_or_else(|| {
        DepError::DockerImageNotFound(DOWNLOADER_IMAGE.to_string())
    })?;

    let dir = dockerfile_dir
        .parent()
        .unwrap_or(&dockerfile_dir)
        .to_path_buf();

    build_image(&dir)
}

/// Run the downloader container to download a dependency.
///
/// If the Docker image does not exist, it will be built automatically
/// from `Dockerfile.downloader`.
pub fn run_downloader(spec: &DepSpec, output_dir: &Path) -> Result<()> {
    ensure_image()?;

    let mount_arg = format!(
        "{}:/workspace/out",
        to_docker_mount_path(output_dir)
    );

    let kind_str = spec.kind.as_str();
    let args = [
        "run",
        "--rm",
        "-v",
        &mount_arg,
        DOWNLOADER_IMAGE,
        kind_str,
        &spec.name,
        &spec.version,
        "/workspace/out",
    ];

    info!(
        "Running: docker run --rm -v {} {} dep-download {} {} {} /workspace/out",
        mount_arg, DOWNLOADER_IMAGE, kind_str, spec.name, spec.version
    );

    let status = Command::new("docker")
        .args(&args)
        .status()
        .context("Failed to execute docker run")?;

    if !status.success() {
        return Err(DepError::DockerCommandFailed(format!(
            "docker run exited with status: {}",
            status
        ))
        .into());
    }

    Ok(())
}
