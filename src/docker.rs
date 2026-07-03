use std::env;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use log::info;

use crate::model::{DepError, DepSpec};
use crate::util::{to_docker_mount_path, DOWNLOADER_IMAGE};

/// 需要传递到 Docker 容器的镜像源环境变量
const MIRROR_ENV_VARS: &[&str] = &[
    "MAVEN_MIRROR",
    "NPM_MIRROR",
    "PYPI_MIRROR",
    "CARGO_MIRROR",
];

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

/// 从远程仓库拉取下载器镜像。
pub fn pull_image() -> Result<()> {
    info!("正在拉取Docker镜像{} ...", DOWNLOADER_IMAGE);

    let status = Command::new("docker")
        .args(["pull", DOWNLOADER_IMAGE])
        .status()
        .context("执行docker pull失败")?;

    if !status.success() {
        return Err(
            DepError::DockerCommandFailed(format!("docker pull {}失败", DOWNLOADER_IMAGE)).into()
        );
    }

    info!("Docker镜像{}拉取成功。", DOWNLOADER_IMAGE);
    Ok(())
}

/// 确保下载器镜像可用。如果不存在，则从远程仓库拉取。
pub fn ensure_image() -> Result<()> {
    ensure_docker_installed()?;

    if image_exists() {
        return Ok(());
    }

    pull_image()
}

/// 运行下载器容器以下载依赖项。
///
/// 如果Docker镜像不存在，将自动从远程仓库拉取。
/// 会自动将宿主机的镜像源环境变量传递到容器中。
pub fn run_downloader(spec: &DepSpec, output_dir: &Path) -> Result<()> {
    ensure_image()?;

    let mount_arg = format!(
        "{}:/workspace/out",
        to_docker_mount_path(output_dir)
    );

    let kind_str = spec.kind.as_str();

    // 构建 docker run 命令
    let mut cmd = Command::new("docker");
    cmd.args(["run", "--rm", "-v", &mount_arg]);

    // 传递镜像源环境变量到容器
    for var in MIRROR_ENV_VARS {
        if let Ok(val) = env::var(var) {
            if !val.is_empty() {
                cmd.args(["-e", &format!("{}={}", var, val)]);
                info!("传递环境变量: {}={}", var, val);
            }
        }
    }

    cmd.args([
        DOWNLOADER_IMAGE,
        kind_str,
        &spec.name,
        &spec.version,
        "/workspace/out",
    ]);

    info!(
        "正在运行: docker run --rm -v {} {} dep-download {} {} {} /workspace/out",
        mount_arg, DOWNLOADER_IMAGE, kind_str, spec.name, spec.version
    );

    let status = cmd.status().context("执行docker run失败")?;

    if !status.success() {
        return Err(DepError::DockerCommandFailed(format!(
            "docker run退出状态: {}",
            status
        ))
        .into());
    }

    Ok(())
}
