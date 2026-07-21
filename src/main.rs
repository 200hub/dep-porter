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
        Command::Download(args) => {
            // 验证参数
            if let Some(ref pom_file) = args.from_pom {
                // 使用 pom.xml 模式
                if args.kind != dep_porter::model::DepKind::Maven {
                    anyhow::bail!("--from-pom 选项仅适用于 Maven 依赖（--kind maven）");
                }
                cmd_download_from_pom(&args, pom_file)
            } else {
                // 传统单个依赖模式
                if args.name.is_none() || args.version.is_none() {
                    anyhow::bail!("必须指定 --name 和 --version，或者使用 --from-pom");
                }
                cmd_download(args)
            }
        }
        Command::Import(args) => cmd_import(args),
    }
}

fn cmd_download(args: dep_porter::cli::DownloadArgs) -> Result<()> {
    let name = args.name.as_ref().unwrap().clone();
    let version = args.version.as_ref().unwrap().clone();
    
    let spec = DepSpec::new(args.kind, name.clone(), version.clone());
    let dir_name = build_dir_name(args.kind, &name, &version);
    let output_base = PathBuf::from(&args.output);
    let output_dir = output_base.join(&dir_name);

    // 安全检查（默认开启，--no-check-security 可关闭）
    if args.check_security && !args.no_check_security {
        match dep_porter::security::check_vulnerabilities(args.kind, &name, &version) {
            Ok(Some(findings)) if !findings.is_empty() => {
                dep_porter::security::print_findings(
                    args.kind,
                    &name,
                    &version,
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
        match dep_porter::license::check_license(args.kind, &name, &version) {
            Ok(Some(finding)) => {
                let needs_confirmation = dep_porter::license::print_finding(
                    args.kind,
                    &name,
                    &version,
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

    info!("正在下载 {} {}:{} ...", args.kind, name, version);
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

fn cmd_download_from_pom(args: &dep_porter::cli::DownloadArgs, pom_file: &str) -> Result<()> {
    let pom_path = PathBuf::from(pom_file);
    
    if !pom_path.exists() {
        anyhow::bail!("pom.xml 文件不存在: {}", pom_path.display());
    }

    info!("正在解析 pom.xml: {}", pom_path.display());
    let dependencies = dep_porter::pom::parse_pom_dependencies(&pom_path)?;

    if dependencies.is_empty() {
        info!("pom.xml 中未找到任何依赖，无需下载。");
        return Ok(());
    }

    info!("找到 {} 个依赖项（已过滤 test scope）", dependencies.len());
    info!("");

    let total = dependencies.len();
    let mut success_count = 0;
    let mut failed = Vec::new();

    for (idx, dep) in dependencies.iter().enumerate() {
        info!(
            "===== [{}/{}] 下载: {}:{}:{} =====",
            idx + 1,
            total,
            dep.group_id,
            dep.artifact_id,
            dep.version
        );

        let spec = DepSpec::new(
            dep_porter::model::DepKind::Maven,
            dep.to_coordinate(),
            dep.version.clone(),
        );
        let dir_name = build_dir_name(
            dep_porter::model::DepKind::Maven,
            &dep.to_coordinate(),
            &dep.version,
        );
        let output_base = PathBuf::from(&args.output);
        let output_dir = output_base.join(&dir_name);

        // 如果目录已存在，询问是否跳过
        if output_dir.exists() {
            info!("目录已存在，跳过: {}", output_dir.display());
            success_count += 1;
            continue;
        }

        // 安全检查
        if args.check_security && !args.no_check_security {
            match dep_porter::security::check_vulnerabilities(
                dep_porter::model::DepKind::Maven,
                &dep.to_coordinate(),
                &dep.version,
            ) {
                Ok(Some(findings)) if !findings.is_empty() => {
                    dep_porter::security::print_findings(
                        dep_porter::model::DepKind::Maven,
                        &dep.to_coordinate(),
                        &dep.version,
                        &findings,
                    );
                    if !dep_porter::security::prompt_continue() {
                        warn!("用户跳过此依赖");
                        failed.push(format!(
                            "{}:{}:{}（用户跳过）",
                            dep.group_id, dep.artifact_id, dep.version
                        ));
                        continue;
                    }
                }
                Ok(Some(_)) => {
                    info!("未发现已知漏洞。");
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("安全检查失败（{}），继续执行。", e);
                }
            }
        }

        // 许可证检查
        if args.check_license && !args.no_check_license {
            match dep_porter::license::check_license(
                dep_porter::model::DepKind::Maven,
                &dep.to_coordinate(),
                &dep.version,
            ) {
                Ok(Some(finding)) => {
                    let needs_confirmation = dep_porter::license::print_finding(
                        dep_porter::model::DepKind::Maven,
                        &dep.to_coordinate(),
                        &dep.version,
                        &finding,
                    );
                    if needs_confirmation && !dep_porter::security::prompt_continue() {
                        warn!("用户因许可证风险跳过此依赖");
                        failed.push(format!(
                            "{}:{}:{}（许可证风险）",
                            dep.group_id, dep.artifact_id, dep.version
                        ));
                        continue;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("许可证检查失败（{}），继续执行。", e);
                }
            }
        }

        std::fs::create_dir_all(&output_dir)?;

        let cache_dir = if args.no_cache {
            None
        } else {
            Some(
                args.cache_dir
                    .clone()
                    .unwrap_or_else(dep_porter::docker::default_cache_dir),
            )
        };

        match dep_porter::docker::run_downloader_with_cache(&spec, &output_dir, cache_dir.as_deref())
        {
            Ok(_) => {
                info!("✓ 下载成功: {}", output_dir.display());
                success_count += 1;
            }
            Err(e) => {
                warn!("✗ 下载失败: {}", e);
                failed.push(format!(
                    "{}:{}:{}（{}）",
                    dep.group_id, dep.artifact_id, dep.version, e
                ));
            }
        }
        info!("");
    }

    info!("========================================");
    info!("批量下载完成:");
    info!("  总计: {}", total);
    info!("  成功: {}", success_count);
    info!("  失败: {}", failed.len());

    if !failed.is_empty() {
        info!("");
        info!("失败的依赖:");
        for f in &failed {
            info!("  - {}", f);
        }
    }

    if failed.len() > 0 {
        anyhow::bail!("{} 个依赖下载失败", failed.len());
    }

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
