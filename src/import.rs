use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use log::{info, warn};

use crate::config::AppConfig;
use crate::model::{DepError, DepKind, DepSpec, MavenCoordinate};
use crate::registry;
use crate::util::{collect_files_sorted, relative_path};

/// 将下载的依赖项导入到Nexus仓库。
pub fn import_to_nexus(
    spec: &DepSpec,
    download_dir: &Path,
    config: &AppConfig,
    overwrite: bool,
) -> Result<()> {
    if !download_dir.exists() {
        return Err(DepError::DownloadDirNotFound(download_dir.display().to_string()).into());
    }

    match spec.kind {
        DepKind::Maven => import_maven(spec, download_dir, config, overwrite),
        DepKind::Npm => import_npm(spec, download_dir, config, overwrite),
        DepKind::Pypi => import_pypi(spec, download_dir, config),
        DepKind::Cargo => import_cargo(spec, download_dir, config, overwrite),
        DepKind::Conan => import_raw(spec, download_dir, config, "conan", overwrite),
    }
}

/// 通过将本地仓库中的每个文件上传到Nexus Maven仓库中的相应路径来导入Maven工件。
fn import_maven(
    spec: &DepSpec,
    download_dir: &Path,
    config: &AppConfig,
    overwrite: bool,
) -> Result<()> {
    let coord = MavenCoordinate::parse(&spec.name)?;

    // 兼容两种目录结构：
    // 1. download_dir/repository/... (旧方式)
    // 2. download_dir/... (新方式，直接是Maven仓库结构)
    let repo_dir = if download_dir.join("repository").exists() {
        download_dir.join("repository")
    } else {
        download_dir.to_path_buf()
    };

    // 上传仓库中的所有文件（包括传递依赖项）
    let files = collect_files_sorted(&repo_dir)?;
    if files.is_empty() {
        return Err(DepError::DownloadDirEmpty(repo_dir.display().to_string()).into());
    }

    // 过滤掉Maven本地仓库的元数据文件（不是工件，Nexus会拒绝）
    let files: Vec<_> = files
        .into_iter()
        .filter(|f| {
            let name = f.file_name().unwrap_or_default().to_string_lossy();
            // 过滤Maven本地仓库生成的元数据文件
            name != "_remote.repositories"
                && name != "resolver-status.properties"
                && !name.starts_with("maven-metadata-")
                && name != "maven-metadata.xml"
        })
        .collect();

    info!(
        "正在上传{}个文件（包括传递依赖项）",
        files.len()
    );

    let client = reqwest::blocking::Client::new();
    let nexus_base = config.nexus.base_url.trim_end_matches('/');

    // SNAPSHOT版本的传递依赖项（非SNAPSHOT）应该上传到maven仓库，而不是maven-snapshots
    let snapshot_repo = config
        .repositories
        .maven_snapshots
        .as_deref()
        .unwrap_or(&config.repositories.maven);
    let release_repo = &config.repositories.maven;

    for file in &files {
        let full_rel = file
            .strip_prefix(&repo_dir)
            .context("计算相对路径失败")?
            .to_string_lossy()
            .replace('\\', "/");

        // 根据文件路径判断是否是SNAPSHOT版本，选择对应的仓库
        let repo_name = if full_rel.to_uppercase().contains("-SNAPSHOT") {
            snapshot_repo
        } else {
            release_repo
        };
        let url = format!("{}/repository/{}/{}", nexus_base, repo_name, full_rel);

        // 如果不覆盖，检查工件是否已存在（HEAD）
        if !overwrite {
            let head = client
                .head(&url)
                .basic_auth(&config.nexus.username, Some(&config.nexus.password))
                .send();
            if let Ok(resp) = head {
                if resp.status().is_success() {
                    info!("跳过（已存在）: {}", url);
                    continue;
                }
            }
        }

        info!("正在上传: {}", url);
        let content =
            fs::read(file).with_context(|| format!("读取{}失败", file.display()))?;

        let resp = client
            .put(&url)
            .basic_auth(&config.nexus.username, Some(&config.nexus.password))
            .body(content)
            .send()
            .with_context(|| format!("PUT {}失败", url))?;

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
        "Maven导入完成: 为{}:{}:{}上传了{}个文件",
        coord.group_id,
        coord.artifact_id,
        spec.version,
        files.len()
    );
    Ok(())
}

/// 通过使用标准npm发布协议将npm包发布到Nexus npm托管仓库来导入它们。
///
/// 原生npm仓库*不会*索引上传到任意路径的`.tgz`文件——该包对`npm install`不可见。
/// 相反，我们为每个tarball（请求的包和所有传递依赖项）发送一个npm发布文档（packument），
/// 其中tarball作为base64编码的`_attachment`嵌入，就像`npm publish`所做的那样。
fn import_npm(
    spec: &DepSpec,
    download_dir: &Path,
    config: &AppConfig,
    overwrite: bool,
) -> Result<()> {
    // 收集所有tarball（请求的包 + 传递依赖项）。
    let files = collect_files_sorted(download_dir)?;
    let tgz_files: Vec<_> = files
        .iter()
        .filter(|f| {
            f.file_name()
                .map(|n| n.to_string_lossy().ends_with(".tgz"))
                .unwrap_or(false)
        })
        .collect();

    if tgz_files.is_empty() {
        warn!(
            "在{}中未找到.tgz文件。npm导入需要包tarball。",
            download_dir.display()
        );
        return Err(DepError::DownloadDirEmpty(format!(
            "{}中没有.tgz文件",
            download_dir.display()
        ))
        .into());
    }

    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let repo_name = &config.repositories.npm;
    let client = reqwest::blocking::Client::new();
    let mut published = 0u32;
    let mut skipped = 0u32;

    for file in &tgz_files {
        let tgz = fs::read(file).with_context(|| format!("读取{}失败", file.display()))?;

        let package_json = registry::read_npm_package_json(&tgz)
            .with_context(|| format!("从{}读取package.json失败", file.display()))?;

        let (name, version, doc) =
            registry::build_npm_publish_doc(&package_json, &tgz, nexus_base, repo_name)?;

        // 如果已存在则跳过：HEAD版本化元数据端点。
        let exists_url = format!(
            "{}/repository/{}/{}/{}",
            nexus_base,
            repo_name,
            registry::npm_encode_name(&name),
            version
        );
        if !overwrite {
            if let Ok(resp) = client
                .get(&exists_url)
                .basic_auth(&config.nexus.username, Some(&config.nexus.password))
                .send()
            {
                if resp.status().is_success() {
                    info!("跳过（已存在）: {}@{}", name, version);
                    skipped += 1;
                    continue;
                }
            }
        }

        let url = format!(
            "{}/repository/{}/{}",
            nexus_base,
            repo_name,
            registry::npm_encode_name(&name)
        );
        info!("正在发布npm包: {}@{}", name, version);

        let resp = client
            .put(&url)
            .basic_auth(&config.nexus.username, Some(&config.nexus.password))
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&doc)?)
            .send()
            .with_context(|| format!("PUT {}失败", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(DepError::NexusUploadFailed {
                url: format!("{} ({}@{}): {}", url, name, version, body.trim()),
                status: status.as_u16(),
            }
            .into());
        }
        info!("  -> {} {}@{}", status, name, version);
        published += 1;
    }

    info!(
        "npm导入完成 {}@{}: 发布了{}个，跳过了{}个（共{}个tarball）",
        spec.name,
        spec.version,
        published,
        skipped,
        tgz_files.len()
    );
    Ok(())
}

/// 使用`twine upload`导入PyPI包。
fn import_pypi(spec: &DepSpec, download_dir: &Path, config: &AppConfig) -> Result<()> {
    // 先检查twine是否已安装
    if which::which("twine").is_err() {
        return Err(anyhow::anyhow!(
            "twine未安装。请先安装twine:\n\
             \n\
             方式1 (推荐): pip install twine\n\
             方式2: pipx install twine\n\
             \n\
             安装后请重新运行此命令。"
        ));
    }

    let packages_dir = download_dir.join("packages");
    if !packages_dir.exists() {
        return Err(DepError::DownloadDirNotFound(format!(
            "{}（预期'packages/'子目录）",
            packages_dir.display()
        ))
        .into());
    }

    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let repo_name = &config.repositories.pypi;
    // Nexus要求URL末尾带斜杠，否则返回400错误
    let repo_url = format!("{}/repository/{}/", nexus_base, repo_name);

    info!(
        "正在从{}上传PyPI包到{}",
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
        .context("执行twine失败")?;

    if !status.success() {
        return Err(DepError::DockerCommandFailed(format!(
            "twine上传退出状态: {}",
            status
        ))
        .into());
    }

    info!("PyPI导入完成 {}=={}", spec.name, spec.version);
    Ok(())
}

/// 将Cargo crate导入到原生Nexus Cargo托管仓库。
///
/// Nexus Cargo仓库只接受通过Cargo注册表API（`PUT /api/v1/crates/new`）的发布；
/// 上传到原始路径的`.crate`文件不会被索引，也无法被`cargo`解析。
/// 因此，我们使用该API发布每个真实的`.crate`文件（请求的crate和所有传递依赖项），
/// 从下载时捕获的crates.io稀疏索引行（`index/{name}-{version}.json`）中提取每个crate所需的元数据。
///
/// 如果未配置cargo仓库，我们将回退到尽力而为的原始上传（虽然`cargo`无法使用，但会保留工件）。
fn import_cargo(
    spec: &DepSpec,
    download_dir: &Path,
    config: &AppConfig,
    overwrite: bool,
) -> Result<()> {
    let crates_dir = download_dir.join("crates");
    let index_dir = download_dir.join("index");

    if crates_dir.exists() {
        if let Some(cargo_repo) = &config.repositories.cargo {
            info!(
                "正在将crate发布到原生cargo仓库'{}'",
                cargo_repo
            );
            return publish_crates(&crates_dir, &index_dir, config, cargo_repo, overwrite)
                .with_context(|| {
                    format!(
                        "将crate发布到cargo仓库'{}'失败",
                        cargo_repo
                    )
                });
        }
        warn!(
            "未配置[repositories] cargo；回退到原始上传（cargo无法使用）。"
        );
    } else {
        warn!("未找到crates/目录；回退到原始上传。");
    }

    // 回退：将所有内容上传到原始仓库（旧版，cargo无法使用）。
    import_raw(spec, download_dir, config, "cargo", overwrite)
}

/// 通过Cargo注册表发布API将每个`.crate`文件发布到Nexus Cargo托管仓库。
fn publish_crates(
    crates_dir: &Path,
    index_dir: &Path,
    config: &AppConfig,
    repo_name: &str,
    overwrite: bool,
) -> Result<()> {
    let files = collect_files_sorted(crates_dir)?;
    let crate_files: Vec<_> = files
        .iter()
        .filter(|f| {
            f.file_name()
                .map(|n| n.to_string_lossy().ends_with(".crate"))
                .unwrap_or(false)
        })
        .collect();

    if crate_files.is_empty() {
        return Err(DepError::DownloadDirEmpty(crates_dir.display().to_string()).into());
    }

    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let publish_url = format!("{}/repository/{}/api/v1/crates/new", nexus_base, repo_name);
    let client = reqwest::blocking::Client::new();
    let mut published = 0u32;
    let mut skipped = 0u32;

    for file in &crate_files {
        let filename = file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let stem = match filename.strip_suffix(".crate") {
            Some(s) => s,
            None => continue,
        };

        // 伴随索引文件包含权威的名称/版本/元数据。
        let index_file = index_dir.join(format!("{}.json", stem));
        let index_line = fs::read_to_string(&index_file).with_context(|| {
            format!(
                "缺少'{}'的crates.io索引元数据（预期{}）。 \
                 请使用更新的dep-porter重新下载以捕获索引元数据。",
                stem,
                index_file.display()
            )
        })?;

        let meta = registry::cargo_index_to_publish_meta(index_line.trim())?;
        let name = meta["name"].as_str().unwrap_or("").to_string();
        let version = meta["vers"].as_str().unwrap_or("").to_string();

        // 如果已存在则跳过：查询此crate/版本的稀疏索引。
        if !overwrite
            && crate_version_exists(&client, config, nexus_base, repo_name, &name, &version)
        {
            info!("跳过（已存在）: {} {}", name, version);
            skipped += 1;
            continue;
        }

        let crate_bytes =
            fs::read(file).with_context(|| format!("读取{}失败", file.display()))?;
        let body = registry::build_cargo_publish_body(&meta, &crate_bytes)?;

        info!("正在发布crate: {} {}", name, version);
        let resp = client
            .put(&publish_url)
            .basic_auth(&config.nexus.username, Some(&config.nexus.password))
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
            .with_context(|| format!("PUT {}失败", publish_url))?;

        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        // Cargo API在逻辑失败时返回200状态码和一个`errors`数组。
        if !status.is_success() || text.contains("\"errors\"") {
            return Err(DepError::NexusUploadFailed {
                url: format!("{} ({} {}): {}", publish_url, name, version, text.trim()),
                status: status.as_u16(),
            }
            .into());
        }
        info!("  -> {} {} {}", status, name, version);
        published += 1;
    }

    info!(
        "通过'{}'完成Cargo导入: 发布了{}个，跳过了{}个（共{}个crate）",
        repo_name,
        published,
        skipped,
        crate_files.len()
    );
    Ok(())
}

/// 通过请求crate的下载资源来检查其版本是否已存在于Nexus cargo仓库中
/// （比稀疏索引更可靠，因为Nexus可能从过时的缓存中提供服务）。
fn crate_version_exists(
    client: &reqwest::blocking::Client,
    config: &AppConfig,
    nexus_base: &str,
    repo_name: &str,
    name: &str,
    version: &str,
) -> bool {
    let url = format!(
        "{}/repository/{}/crates/{}/{}/download",
        nexus_base, repo_name, name, version
    );
    if let Ok(resp) = client
        .get(&url)
        .basic_auth(&config.nexus.username, Some(&config.nexus.password))
        .send()
    {
        return resp.status().is_success();
    }
    false
}

/// 将`download_dir`下的所有文件上传到指定的Nexus仓库。
///
/// `kind_prefix`用于URL路径中（例如`cargo/serde/1.0.0/...`）。
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
        return Err(DepError::DownloadDirEmpty(download_dir.display().to_string()).into());
    }

    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let client = reqwest::blocking::Client::new();

    for file in &files {
        let rel = relative_path(download_dir, file)
            .context("计算相对路径失败")?
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
                    info!("跳过（已存在）: {}", url);
                    continue;
                }
            }
        }

        info!("正在上传: {}", url);
        let content =
            fs::read(file).with_context(|| format!("读取{}失败", file.display()))?;

        let resp = client
            .put(&url)
            .basic_auth(&config.nexus.username, Some(&config.nexus.password))
            .header("Content-Type", "application/octet-stream")
            .body(content)
            .send()
            .with_context(|| format!("PUT {}失败", url))?;

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

/// 将文件导入到Nexus原始仓库，作为不支持类型的回退方案。
fn import_raw(
    spec: &DepSpec,
    download_dir: &Path,
    config: &AppConfig,
    kind_prefix: &str,
    overwrite: bool,
) -> Result<()> {
    let repo_name = &config.repositories.raw;
    upload_files_to_repo(
        spec,
        download_dir,
        config,
        repo_name,
        kind_prefix,
        overwrite,
    )?;

    info!(
        "{}导入完成 {}@{}（通过原始仓库'{}'）",
        kind_prefix, spec.name, spec.version, repo_name
    );
    Ok(())
}
