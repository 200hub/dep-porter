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
    }
}

fn cmd_download(args: dep_porter::cli::DownloadArgs) -> Result<()> {
    let spec = DepSpec::new(args.kind, args.name.clone(), args.version.clone());
    let dir_name = build_dir_name(args.kind, &args.name, &args.version);
    let output_base = PathBuf::from(&args.output);
    let output_dir = output_base.join(&dir_name);

    // 安全检查（可选）
    if args.check_security {
        match dep_porter::security::check_vulnerabilities(args.kind, &args.name, &args.version) {
            Ok(Some(findings)) if !findings.is_empty() => {
                dep_porter::security::print_findings(args.kind, &args.name, &args.version, &findings);
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

    info!(
        "正在下载 {} {}:{} ...",
        args.kind, args.name, args.version
    );
    info!("输出: {}", output_dir.display());

    std::fs::create_dir_all(&output_dir)?;

    dep_porter::docker::run_downloader(&spec, &output_dir)?;

    info!(
        "下载完成: {}",
        output_dir.display()
    );

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
        args.kind, args.name, args.version, download_dir.display()
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
