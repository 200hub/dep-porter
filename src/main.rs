use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use log::{info, warn};

use dep_porter::cli::{Cli, Command};
use dep_porter::model::DepSpec;
use dep_porter::util::build_dir_name;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Download(args) => cmd_download(args),
        Command::Import(args) => cmd_import(args),
        Command::BatchDownload(args) => cmd_batch_download(args),
        Command::BatchImport(args) => cmd_batch_import(args),
    }
}

fn cmd_download(args: dep_porter::cli::DownloadArgs) -> Result<()> {
    let spec = DepSpec::new(args.kind, args.name.clone(), args.version.clone());
    let dir_name = build_dir_name(args.kind, &args.name, &args.version);
    let output_base = PathBuf::from(&args.output);
    let output_dir = output_base.join(&dir_name);

    // 安全检查（默认开启，--no-check-security 可关闭）
    if args.check_security && !args.no_check_security {
        match dep_porter::security::check_vulnerabilities(args.kind, &args.name, &args.version) {
            Ok(Some(findings)) if !findings.is_empty() => {
                dep_porter::security::print_findings(
                    args.kind,
                    &args.name,
                    &args.version,
                    &findings,
                );
                if !dep_porter::security::prompt_continue() {
                    info!("用户中止下载。");
                    return Ok(());
                }
            }
            Ok(Some(_)) => {
                info!("未发现已知漏洞。");
            }
            Ok(None) => {
                info!("安全检查不适用于{}。", args.kind);
            }
            Err(e) => {
                warn!("安全检查失败（{}），继续执行。", e);
            }
        }
    }

    // 许可证商用风险检查（默认开启，--no-check-license 可关闭）
    if args.check_license && !args.no_check_license {
        match dep_porter::license::check_license(args.kind, &args.name, &args.version) {
            Ok(Some(finding)) => {
                let needs_confirmation = dep_porter::license::print_finding(
                    args.kind,
                    &args.name,
                    &args.version,
                    &finding,
                );
                if needs_confirmation && !dep_porter::security::prompt_continue() {
                    info!("用户因许可证风险中止下载。");
                    return Ok(());
                }
            }
            Ok(None) => {
                info!("许可证检查不适用于{}。", args.kind);
            }
            Err(e) => {
                warn!("许可证检查失败（{}），继续执行。", e);
            }
        }
    }

    info!("正在下载 {} {}:{} ...", args.kind, args.name, args.version);
    info!("输出: {}", output_dir.display());

    std::fs::create_dir_all(&output_dir)?;

    let cache_dir = if args.no_cache {
        None
    } else {
        Some(
            args.cache_dir
                .unwrap_or_else(dep_porter::docker::default_cache_dir),
        )
    };
    dep_porter::docker::run_downloader_with_cache(&spec, &output_dir, cache_dir.as_deref())?;

    info!("下载完成: {}", output_dir.display());

    Ok(())
}

fn cmd_import(args: dep_porter::cli::ImportArgs) -> Result<()> {
    let config_path = PathBuf::from(&args.config);
    let cfg = dep_porter::config::AppConfig::from_file(&config_path)?;

    let spec = DepSpec::new(args.kind, args.name.clone(), args.version.clone());
    let dir_name = build_dir_name(args.kind, &args.name, &args.version);

    let download_dir = PathBuf::from(&dir_name);

    info!(
        "正在从 {} 导入 {} {}:{} ...",
        args.kind,
        args.name,
        args.version,
        download_dir.display()
    );
    info!("Nexus: {}", cfg.nexus.base_url);
    if args.overwrite {
        info!("模式: 覆盖");
    } else {
        info!("模式: 如果存在则跳过");
    }

    dep_porter::import::import_to_nexus(&spec, &download_dir, &cfg, args.overwrite)?;

    info!("导入完成。");

    Ok(())
}

/// 解析依赖文件得到直接依赖列表；若为空则给出提示。
fn parse_manifest_specs(file: &std::path::Path, include_dev: bool) -> Result<Vec<DepSpec>> {
    let options = dep_porter::manifest::ParseOptions { include_dev };
    let specs = dep_porter::manifest::parse_file(file, options)?;
    if specs.is_empty() {
        warn!(
            "未从 {} 解析到任何可下载的精确版本直接依赖。",
            file.display()
        );
    }
    Ok(specs)
}

fn cmd_batch_download(args: dep_porter::cli::BatchDownloadArgs) -> Result<()> {
    let specs = parse_manifest_specs(&args.file, args.include_dev)?;
    if specs.is_empty() {
        return Ok(());
    }

    info!(
        "从 {} 解析到 {} 个直接依赖：",
        args.file.display(),
        specs.len()
    );
    for spec in &specs {
        info!("  - {} {}@{}", spec.kind, spec.name, spec.version);
    }

    // 一次性安全 + 许可证风险检查，统一确认。
    if args.check_security || args.check_license {
        let check_security = args.check_security && !args.no_check_security;
        let check_license = args.check_license && !args.no_check_license;
        if check_security || check_license {
            info!("正在检查安全与许可证风险 ...");
            let reports = dep_porter::batch::collect_reports(&specs, check_security, check_license);
            if dep_porter::batch::any_requires_confirmation(&reports) {
                dep_porter::batch::print_reports(&reports);
                if !dep_porter::security::prompt_continue() {
                    info!("用户因风险中止批量下载。");
                    return Ok(());
                }
            } else {
                info!("未发现需要确认的安全或许可证风险。");
            }
        }
    }

    let cache_dir = if args.no_cache {
        None
    } else {
        Some(
            args.cache_dir
                .clone()
                .unwrap_or_else(dep_porter::docker::default_cache_dir),
        )
    };

    let output_base = PathBuf::from(&args.output);
    let total = specs.len();
    let mut failures = Vec::new();
    for (idx, spec) in specs.iter().enumerate() {
        let dir_name = build_dir_name(spec.kind, &spec.name, &spec.version);
        let output_dir = output_base.join(&dir_name);
        info!(
            "[{}/{}] 正在下载 {} {}@{} -> {}",
            idx + 1,
            total,
            spec.kind,
            spec.name,
            spec.version,
            output_dir.display()
        );
        if let Err(e) = std::fs::create_dir_all(&output_dir) {
            warn!("创建目录 {} 失败：{:#}", output_dir.display(), e);
            failures.push(format!("{} {}@{}", spec.kind, spec.name, spec.version));
            continue;
        }
        match dep_porter::docker::run_downloader_with_cache(spec, &output_dir, cache_dir.as_deref())
        {
            Ok(()) => info!("[{}/{}] 完成：{}", idx + 1, total, output_dir.display()),
            Err(e) => {
                warn!(
                    "[{}/{}] 下载 {} {}@{} 失败：{:#}",
                    idx + 1,
                    total,
                    spec.kind,
                    spec.name,
                    spec.version,
                    e
                );
                failures.push(format!("{} {}@{}", spec.kind, spec.name, spec.version));
            }
        }
    }

    if failures.is_empty() {
        info!("批量下载完成，共 {} 个依赖。", total);
        Ok(())
    } else {
        for f in &failures {
            warn!("下载失败：{}", f);
        }
        anyhow::bail!("批量下载有 {} / {} 个依赖失败", failures.len(), total);
    }
}

fn cmd_batch_import(args: dep_porter::cli::BatchImportArgs) -> Result<()> {
    let config_path = PathBuf::from(&args.config);
    let cfg = dep_porter::config::AppConfig::from_file(&config_path)?;

    let specs = parse_manifest_specs(&args.file, args.include_dev)?;
    if specs.is_empty() {
        return Ok(());
    }

    info!("Nexus: {}", cfg.nexus.base_url);
    if args.overwrite {
        info!("模式: 覆盖");
    } else {
        info!("模式: 如果存在则跳过");
    }

    let input_base = PathBuf::from(&args.input);
    let total = specs.len();
    let mut failures = Vec::new();
    for (idx, spec) in specs.iter().enumerate() {
        let dir_name = build_dir_name(spec.kind, &spec.name, &spec.version);
        let download_dir = input_base.join(&dir_name);
        info!(
            "[{}/{}] 正在导入 {} {}@{}（{}）",
            idx + 1,
            total,
            spec.kind,
            spec.name,
            spec.version,
            download_dir.display()
        );
        match dep_porter::import::import_to_nexus(spec, &download_dir, &cfg, args.overwrite) {
            Ok(()) => info!("[{}/{}] 导入完成：{}", idx + 1, total, dir_name),
            Err(e) => {
                warn!(
                    "[{}/{}] 导入 {} {}@{} 失败：{:#}",
                    idx + 1,
                    total,
                    spec.kind,
                    spec.name,
                    spec.version,
                    e
                );
                failures.push(format!("{} {}@{}", spec.kind, spec.name, spec.version));
            }
        }
    }

    if failures.is_empty() {
        info!("批量导入完成，共 {} 个依赖。", total);
        Ok(())
    } else {
        for f in &failures {
            warn!("导入失败：{}", f);
        }
        anyhow::bail!("批量导入有 {} / {} 个依赖失败", failures.len(), total);
    }
}
