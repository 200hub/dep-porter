use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use log::info;

use crate::model::{DepError, DepSpec};
use crate::util::{to_docker_mount_path, DOWNLOADER_IMAGE};

/// Check that Docker is installed and available in PATH.
pub fn ensure_docker_installed() -> Result<()> {
    which::which("docker").map_err(|_| DepError::DockerNotFound)?;
    Ok(())
}

/// Check that the downloader image exists locally.
pub fn ensure_image_exists() -> Result<()> {
    let output = Command::new("docker")
        .args(["image", "inspect", DOWNLOADER_IMAGE])
        .output()
        .context("Failed to run docker image inspect")?;
    if !output.status.success() {
        return Err(DepError::DockerImageNotFound(DOWNLOADER_IMAGE.to_string()).into());
    }
    Ok(())
}

/// Run the downloader container to download a dependency.
///
/// The container mounts `output_dir` to `/workspace/out` and invokes
/// `dep-download <kind> <name> <version> /workspace/out`.
pub fn run_downloader(spec: &DepSpec, output_dir: &Path) -> Result<()> {
    ensure_docker_installed()?;

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
        "dep-download",
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

/// Build the downloader Docker image from the Dockerfile.downloader.
///
/// This is a convenience function; users can also build manually:
/// `docker build -f Dockerfile.downloader -t dep-downloader:latest .`
pub fn build_image(dockerfile_dir: &Path) -> Result<()> {
    let status = Command::new("docker")
        .args([
            "build",
            "-f",
            &format!(
                "{}/Dockerfile.downloader",
                dockerfile_dir.display()
            ),
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
    Ok(())
}
