use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use log::{info, warn};

use crate::model::{DepError, DepSpec};
use crate::util::{to_docker_mount_path, DOWNLOADER_IMAGE};

/// 检查Docker是否已安装且在PATH中可用。
pub fn ensure_docker_installed() -> Result<()> {
    which::which("docker").map_err(|_| DepError::DockerNotFound)?;
    Ok(())
}

/// 检查下载器镜像是否存在于本地。
pub fn image_exists() -> bool {
    Command::new("docker")
        .args(["image", "inspect", DOWNLOADER_IMAGE])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 在磁盘上定位`Dockerfile.downloader`。
///
/// 搜索顺序：
///   1. 当前工作目录
///   2. 当前可执行文件的父目录
///   3. 从当前工作目录向上遍历查找
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

    // 从当前工作目录向上遍历
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

/// 从Dockerfile.downloader构建下载器Docker镜像。
///
/// 在当前目录、可执行文件的父目录以及向上遍历目录树时搜索`Dockerfile.downloader`。
pub fn build_image(dockerfile_dir: &Path) -> Result<()> {
    let dockerfile = dockerfile_dir.join("Dockerfile.downloader");
    if !dockerfile.is_file() {
        return Err(anyhow::anyhow!(
            "在{}中未找到Dockerfile.downloader",
            dockerfile_dir.display()
        ));
    }

    info!(
        "正在从{}构建Docker镜像{} ...",
        dockerfile.display(),
        DOWNLOADER_IMAGE
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
        .context("执行docker build失败")?;

    if !status.success() {
        return Err(
            DepError::DockerCommandFailed("docker build失败".to_string()).into()
        );
    }

    info!("Docker镜像{}构建成功。", DOWNLOADER_IMAGE);
    Ok(())
}

/// 确保下载器镜像可用。如果不存在，
/// 尝试定位`Dockerfile.downloader`并自动构建。
pub fn ensure_image() -> Result<()> {
    ensure_docker_installed()?;

    if image_exists() {
        return Ok(());
    }

    warn!(
        "未找到Docker镜像'{}'。尝试自动构建...",
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

/// 运行下载器容器以下载依赖项。
///
/// 如果Docker镜像不存在，将自动从`Dockerfile.downloader`构建。
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
        "正在运行: docker run --rm -v {} {} dep-download {} {} {} /workspace/out",
        mount_arg, DOWNLOADER_IMAGE, kind_str, spec.name, spec.version
    );

    let status = Command::new("docker")
        .args(&args)
        .status()
        .context("执行docker run失败")?;

    if !status.success() {
        return Err(DepError::DockerCommandFailed(format!(
            "docker run退出状态: {}",
            status
        ))
        .into());
    }

    Ok(())
}
