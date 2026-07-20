use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
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
        DepKind::Pypi => import_pypi(spec, download_dir, config, overwrite),
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
    let discovered_files = files.len();
    let files: Vec<_> = files
        .into_iter()
        .filter(|f| is_maven_artifact_file(f))
        .collect();

    let ignored_files = discovered_files - files.len();
    if ignored_files > 0 {
        info!(
            "已忽略{}个校验、本地元数据或系统杂项文件",
            ignored_files
        );
    }

    if files.is_empty() {
        return Err(DepError::DownloadDirEmpty(format!(
            "{}（没有可上传的Maven工件）",
            repo_dir.display()
        ))
        .into());
    }

    let total_files = files.len();
    info!(
        "正在上传{}个文件（包括传递依赖项）",
        total_files
    );

    // 创建进度条
    let pb = ProgressBar::new(total_files as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );
    pb.set_message("上传中...");

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
                    pb.inc(1);
                    continue;
                }
            }
        }

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
            pb.finish_with_message("上传失败");
            let details = nexus_response_details(resp);
            return Err(DepError::NexusUploadFailed {
                url: url.clone(),
                status: status.as_u16(),
                details,
            }
            .into());
        }
        pb.inc(1);
    }

    pb.finish_with_message("完成");

    info!(
        "Maven导入完成: 为{}:{}:{}上传了{}个文件",
        coord.group_id,
        coord.artifact_id,
        spec.version,
        total_files
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

    let total_files = tgz_files.len();
    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let repo_name = &config.repositories.npm;
    let client = reqwest::blocking::Client::new();
    let mut published = 0u32;
    let mut skipped = 0u32;

    // 创建进度条
    let pb = ProgressBar::new(total_files as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );
    pb.set_message("发布npm包...");

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
                    skipped += 1;
                    pb.inc(1);
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

        let resp = client
            .put(&url)
            .basic_auth(&config.nexus.username, Some(&config.nexus.password))
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&doc)?)
            .send()
            .with_context(|| format!("PUT {}失败", url))?;

        let status = resp.status();
        if !status.is_success() {
            pb.finish_with_message("发布失败");
            let body = resp.text().unwrap_or_default();
            return Err(DepError::NexusUploadFailed {
                url: format!("{} ({}@{})", url, name, version),
                status: status.as_u16(),
                details: response_body_details(body),
            }
            .into());
        }
        published += 1;
        pb.inc(1);
    }

    pb.finish_with_message("完成");

    info!(
        "npm导入完成 {}@{}: 发布了{}个，跳过了{}个（共{}个tarball）",
        spec.name,
        spec.version,
        published,
        skipped,
        total_files
    );
    Ok(())
}

/// 使用`twine upload`导入PyPI包。
fn import_pypi(spec: &DepSpec, download_dir: &Path, config: &AppConfig, overwrite: bool) -> Result<()> {
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

    // 收集所有包文件（.whl 和 .tar.gz）
    let package_files: Vec<_> = fs::read_dir(&packages_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name.ends_with(".whl") || name.ends_with(".tar.gz")
        })
        .collect();

    if package_files.is_empty() {
        return Err(DepError::DownloadDirEmpty(packages_dir.display().to_string()).into());
    }

    let client = reqwest::blocking::Client::new();
    let mut uploaded = 0u32;
    let mut skipped = 0u32;

    for entry in &package_files {
        let filename = entry.file_name().to_string_lossy().to_string();

        // 从文件名解析包名和版本
        let (pkg_name, pkg_version) = parse_pypi_filename(&filename);

        // 如果不覆盖，检查包是否已存在
        if !overwrite {
            if pypi_package_exists(&client, &config, nexus_base, repo_name, &pkg_name, &pkg_version) {
                info!("跳过（已存在）: {}=={}", pkg_name, pkg_version);
                skipped += 1;
                continue;
            }
        }

        // 上传单个包
        info!("正在上传: {}", filename);
        let status = std::process::Command::new("twine")
            .args([
                "upload",
                "--repository-url",
                &repo_url,
                "-u",
                &config.nexus.username,
                "-p",
                &config.nexus.password,
                &entry.path().to_string_lossy(),
            ])
            .env("PYTHONIOENCODING", "utf-8")
            .status()
            .context("执行twine失败")?;

        if !status.success() {
            return Err(DepError::DockerCommandFailed(format!(
                "上传{}失败: {}",
                filename, status
            ))
            .into());
        }
        uploaded += 1;
    }

    info!(
        "PyPI导入完成 {}: 上传了{}个，跳过了{}个（共{}个包）",
        spec.name, uploaded, skipped, package_files.len()
    );
    Ok(())
}

/// 从 PyPI 文件名解析包名和版本
/// wheel 格式: {name}-{version}(-{build})?-{python}-{abi}-{platform}.whl
/// sdist 格式: {name}-{version}.tar.gz
fn parse_pypi_filename(filename: &str) -> (String, String) {
    if filename.ends_with(".whl") {
        // wheel 格式: {name}-{version}-{python}-{abi}-{platform}.whl
        // 例如: certifi-2026.6.17-py3-none-any.whl
        //       charset_normalizer-3.4.7-cp310-cp310-manylinux2014_x86_64.manylinux_2_17_x86_64.manylinux_2_28_x86_64.whl
        let without_ext = filename.trim_end_matches(".whl");
        let parts: Vec<&str> = without_ext.split('-').collect();
        
        // python 标签列表
        let python_tags = ["py2", "py3", "cp2", "cp3", "pp2", "pp3", "jy", "IronPython"];
        
        // 找到 python 标签的位置
        let mut python_tag_idx = None;
        for (i, part) in parts.iter().enumerate() {
            // python 标签通常以 py、cp、pp 等开头，后面是数字
            let lower = part.to_lowercase();
            for tag in &python_tags {
                if lower.starts_with(tag) {
                    python_tag_idx = Some(i);
                    break;
                }
            }
            if python_tag_idx.is_some() {
                break;
            }
        }
        
        if let Some(idx) = python_tag_idx {
            // python 标签之前的部分是包名和版本
            // 包名可能包含 '-'，所以需要从 python 标签往前推
            // 版本号是 python 标签前的第一个部分
            if idx >= 2 {
                // 至少有 name 和 version
                let version = parts[idx - 1];
                let name = parts[..idx - 1].join("-");
                return (name, version.to_string());
            }
        }
        
        // 如果找不到 python 标签，尝试通用方法
        // 假设最后三个部分是 python-abi-platform
        if parts.len() >= 4 {
            let version = parts[parts.len() - 4];
            let name = parts[..parts.len() - 4].join("-");
            return (name, version.to_string());
        }
    }

    // sdist 格式: {name}-{version}.tar.gz
    // 例如: requests-2.32.3.tar.gz
    let without_ext = filename.trim_end_matches(".tar.gz");
    if let Some(pos) = without_ext.rfind('-') {
        let version = &without_ext[pos + 1..];
        let pkg_name = &without_ext[..pos];
        return (pkg_name.to_string(), version.to_string());
    }

    // 无法解析，返回原始文件名
    (filename.to_string(), String::new())
}

/// 检查 PyPI 包是否已存在于 Nexus
/// 使用 PyPI simple API 检查: GET /repository/{repo}/simple/{package_name}/
fn pypi_package_exists(
    client: &reqwest::blocking::Client,
    config: &AppConfig,
    nexus_base: &str,
    repo_name: &str,
    pkg_name: &str,
    pkg_version: &str,
) -> bool {
    // 使用 PyPI simple API 检查包是否存在
    // 返回 HTML 格式，包含所有版本的链接
    let url = format!(
        "{}/repository/{}/simple/{}/",
        nexus_base, repo_name, pkg_name.to_lowercase()
    );

    if let Ok(resp) = client
        .get(&url)
        .basic_auth(&config.nexus.username, Some(&config.nexus.password))
        .send()
    {
        if resp.status().is_success() {
            if let Ok(body) = resp.text() {
                // simple API 返回的 HTML 中包含版本链接
                // 格式通常是: /repository/pypi/simple/{pkg_name}/{pkg_name}-{version}.tar.gz
                // 或: /repository/pypi/simple/{pkg_name}/{pkg_name}-{version}-py3-none-any.whl
                // 检查是否包含版本字符串
                return body.contains(pkg_version);
            }
        }
    }

    false
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

    let total_files = crate_files.len();
    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let publish_url = format!("{}/repository/{}/api/v1/crates/new", nexus_base, repo_name);
    let client = reqwest::blocking::Client::new();
    let mut published = 0u32;
    let mut skipped = 0u32;

    // 创建进度条
    let pb = ProgressBar::new(total_files as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );
    pb.set_message("发布Cargo crate...");

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
            skipped += 1;
            pb.inc(1);
            continue;
        }

        let crate_bytes =
            fs::read(file).with_context(|| format!("读取{}失败", file.display()))?;
        let body = registry::build_cargo_publish_body(&meta, &crate_bytes)?;

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
            pb.finish_with_message("发布失败");
            return Err(DepError::NexusUploadFailed {
                url: format!("{} ({} {})", publish_url, name, version),
                status: status.as_u16(),
                details: response_body_details(text),
            }
            .into());
        }
        published += 1;
        pb.inc(1);
    }

    pb.finish_with_message("完成");

    info!(
        "通过'{}'完成Cargo导入: 发布了{}个，跳过了{}个（共{}个crate）",
        repo_name,
        published,
        skipped,
        total_files
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

    let total_files = files.len();
    let nexus_base = config.nexus.base_url.trim_end_matches('/');
    let client = reqwest::blocking::Client::new();

    // 创建进度条
    let pb = ProgressBar::new(total_files as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );
    pb.set_message(format!("上传{}...", kind_prefix));

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
                    pb.inc(1);
                    continue;
                }
            }
        }

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
            pb.finish_with_message("上传失败");
            let details = nexus_response_details(resp);
            return Err(DepError::NexusUploadFailed {
                url: url.clone(),
                status: status.as_u16(),
                details,
            }
            .into());
        }
        pb.inc(1);
    }

    pb.finish_with_message("完成");

    Ok(())
}

/// Maven和Nexus都会生成标准校验文件；直接上传这些旁文件会与Nexus在上传主工件时
/// 自动生成的资产冲突，尤其会被禁止重复部署的仓库以HTTP 400拒绝。
fn is_maven_artifact_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    name != "_remote.repositories"
        && name != "resolver-status.properties"
        && name != "thumbs.db"
        && name != "desktop.ini"
        && name != "maven-metadata.xml"
        && !name.starts_with("maven-metadata-")
        // Finder会在浏览或复制目录时生成.DS_Store和AppleDouble(._*)文件；
        // 它们不是Maven工件，Maven格式仓库会拒绝这些路径。
        && !name.starts_with('.')
        && !name.ends_with(".lastupdated")
        && ![".md5", ".sha1", ".sha256", ".sha512"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
}

/// 保留Nexus返回的诊断正文。即使正文无法解码或为空，也要明确说明，避免只留下
/// 一个没有上下文的HTTP状态码。
fn nexus_response_details(resp: reqwest::blocking::Response) -> String {
    match resp.text() {
        Ok(body) => response_body_details(body),
        Err(err) => format!("无法读取响应正文: {}", err),
    }
}

fn response_body_details(body: String) -> String {
    let body = body.trim();
    if body.is_empty() {
        "<响应正文为空>".to_string()
    } else {
        body.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{is_maven_artifact_file, response_body_details};
    use std::path::Path;

    #[test]
    fn maven_upload_excludes_checksums_and_local_metadata() {
        for name in [
            "guava-parent-26.0-android.pom.sha1",
            "library.jar.md5",
            "library.jar.sha256",
            "library.jar.SHA512",
            "library.jar.lastUpdated",
            "_remote.repositories",
            "resolver-status.properties",
            "maven-metadata.xml",
            "maven-metadata-central.xml.sha1",
            ".DS_Store",
            "._guava-26.0-android.jar",
            "Thumbs.db",
            "desktop.ini",
        ] {
            assert!(!is_maven_artifact_file(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn maven_upload_keeps_real_artifacts_and_signatures() {
        for name in [
            "guava-parent-26.0-android.pom",
            "guava-26.0-android.jar",
            "guava-26.0-android-sources.jar",
            "guava-26.0-android.pom.asc",
        ] {
            assert!(is_maven_artifact_file(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn nexus_error_body_is_not_discarded() {
        assert_eq!(response_body_details(" real Nexus reason \n".into()), "real Nexus reason");
        assert_eq!(response_body_details(" \n".into()), "<响应正文为空>");
    }
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
