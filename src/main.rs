use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use log::info;

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

    info!(
        "Downloading {} {}:{} -> {}",
        args.kind, args.name, args.version, output_dir.display()
    );

    std::fs::create_dir_all(&output_dir)?;

    dep_porter::docker::run_downloader(&spec, &output_dir)?;

    info!(
        "Download complete. Files saved to: {}",
        output_dir.display()
    );
    println!(
        "Download complete: {}",
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
        "Importing {} {}:{} from {}",
        args.kind, args.name, args.version, download_dir.display()
    );

    dep_porter::import::import_to_nexus(&spec, &download_dir, &cfg)?;

    info!("Import complete.");
    println!("Import complete.");

    Ok(())
}
