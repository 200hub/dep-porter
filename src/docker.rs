use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use log::info;

use crate::model::{DepError, DepSpec};
use crate::util::{to_docker_mount_path, DOWNLOADER_IMAGE};

pub const CACHE_DIR_ENV: &str = "DEP_PORTER_CACHE_DIR";
const DEFAULT_CACHE_DIR: &str = ".dep-porter-cache";
const CACHE_SCHEMA_VERSION: &str = "v3";

/// 需要传递到 Docker 容器的镜像源环境变量
const MIRROR_ENV_VARS: &[&str] = &["MAVEN_MIRROR", "NPM_MIRROR", "PYPI_MIRROR", "CARGO_MIRROR"];

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
            DepError::DockerCommandFailed(format!("docker pull {}失败", DOWNLOADER_IMAGE)).into(),
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

/// 返回默认下载缓存根目录。
///
/// `DEP_PORTER_CACHE_DIR`非空时优先使用该路径，否则使用当前目录下的
/// `.dep-porter-cache`。
pub fn default_cache_dir() -> PathBuf {
    env::var_os(CACHE_DIR_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CACHE_DIR))
}

fn cache_container_paths(kind: crate::model::DepKind) -> &'static [&'static str] {
    match kind {
        // `/workspace/dep-cache` is used by the current downloader, while
        // `/root/.m2/repository` makes the same global cache work with older
        // images that write directly to Maven's native local repository.
        crate::model::DepKind::Maven => &["/workspace/dep-cache", "/root/.m2/repository"],
        crate::model::DepKind::Npm => &["/root/.npm"],
        crate::model::DepKind::Pypi => &["/root/.cache/pip"],
        crate::model::DepKind::Cargo => &["/root/.cargo/registry"],
        crate::model::DepKind::Conan => &["/root/.conan2"],
    }
}

fn cache_namespace_dir(cache_root: &Path, kind: crate::model::DepKind) -> PathBuf {
    // Package-manager caches already namespace artifacts by coordinates or
    // content hashes. Keep one stable directory per ecosystem so all package
    // versions, mirrors and downloader image revisions can reuse it globally.
    cache_root.join(CACHE_SCHEMA_VERSION).join(kind.as_str())
}

/// 使用默认缓存运行下载器容器。
///
/// 默认缓存可通过`DEP_PORTER_CACHE_DIR`配置。
pub fn run_downloader(spec: &DepSpec, output_dir: &Path) -> Result<()> {
    let cache_dir = default_cache_dir();
    run_downloader_with_cache(spec, output_dir, Some(&cache_dir))
}

/// 运行下载器容器以下载依赖项。
///
/// 如果Docker镜像不存在，将自动从远程仓库拉取。
/// 会自动将宿主机的镜像源环境变量传递到容器中。
/// `cache_root`为`None`时不挂载持久化缓存。
pub fn run_downloader_with_cache(
    spec: &DepSpec,
    output_dir: &Path,
    cache_root: Option<&Path>,
) -> Result<()> {
    ensure_image()?;

    let mount_arg = format!("{}:/workspace/out", to_docker_mount_path(output_dir));

    let kind_str = spec.kind.as_str();

    // 构建 docker run 命令
    let mut cmd = Command::new("docker");
    cmd.args(["run", "--rm", "-v", &mount_arg]);

    let cache_mounts = if let Some(cache_root) = cache_root {
        let cache_dir = cache_namespace_dir(cache_root, spec.kind);
        std::fs::create_dir_all(&cache_dir)
            .with_context(|| format!("创建下载缓存目录{}失败", cache_dir.display()))?;
        let cache_source = to_docker_mount_path(&cache_dir);
        let mounts = cache_container_paths(spec.kind)
            .iter()
            .map(|container_path| format!("{}:{}", cache_source, container_path))
            .collect::<Vec<_>>();
        for mount in &mounts {
            cmd.args(["-v", mount]);
        }
        info!("下载缓存: {}", cache_dir.display());
        Some(mounts)
    } else {
        info!("下载缓存: 已关闭");
        None
    };

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

    if let Some(cache_mounts) = cache_mounts {
        info!(
            "正在运行: docker run --rm -v {} -v {} {} dep-download {} {} {} /workspace/out",
            mount_arg,
            cache_mounts.join(" -v "),
            DOWNLOADER_IMAGE,
            kind_str,
            spec.name,
            spec.version
        );
    } else {
        info!(
            "正在运行: docker run --rm -v {} {} dep-download {} {} {} /workspace/out",
            mount_arg, DOWNLOADER_IMAGE, kind_str, spec.name, spec.version
        );
    }

    let status = cmd.status().context("执行docker run失败")?;

    if !status.success() {
        return Err(
            DepError::DockerCommandFailed(format!("docker run退出状态: {}", status)).into(),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DepKind;

    #[test]
    fn cache_is_shared_across_dependency_versions() {
        let first = DepSpec::new(DepKind::Npm, "@scope/pkg".to_string(), "1.2.3".to_string());
        let second = DepSpec::new(DepKind::Npm, "@scope/pkg".to_string(), "1.2.4".to_string());
        let first_dir = cache_namespace_dir(Path::new("cache"), first.kind);
        let second_dir = cache_namespace_dir(Path::new("cache"), second.kind);

        assert_eq!(first_dir, Path::new("cache/v3/npm"));
        assert_eq!(first_dir, second_dir);
    }

    #[test]
    fn cache_is_shared_across_dependency_names_and_images() {
        let first = DepSpec::new(DepKind::Conan, "zlib".to_string(), "1.2.13".to_string());
        let second = DepSpec::new(DepKind::Conan, "openssl".to_string(), "3.4.1".to_string());

        assert_eq!(
            cache_namespace_dir(Path::new("cache"), first.kind),
            cache_namespace_dir(Path::new("cache"), second.kind)
        );
    }

    #[test]
    fn package_managers_use_their_native_cache_paths() {
        assert_eq!(
            cache_container_paths(DepKind::Maven),
            &["/workspace/dep-cache", "/root/.m2/repository"]
        );
        assert_eq!(cache_container_paths(DepKind::Npm), &["/root/.npm"]);
        assert_eq!(cache_container_paths(DepKind::Pypi), &["/root/.cache/pip"]);
        assert_eq!(
            cache_container_paths(DepKind::Cargo),
            &["/root/.cargo/registry"]
        );
        assert_eq!(cache_container_paths(DepKind::Conan), &["/root/.conan2"]);
    }
}
