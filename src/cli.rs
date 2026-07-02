use clap::{Parser, Subcommand};

/// A CLI tool to download dependencies from the internet and import them
/// into an air-gapped Nexus repository.
#[derive(Debug, Parser)]
#[command(name = "dep-porter", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Download a dependency and all its transitive dependencies.
    Download(DownloadArgs),
    /// Import a previously downloaded dependency into Nexus.
    Import(ImportArgs),
}

/// Arguments for the `download` subcommand.
#[derive(Debug, Parser)]
pub struct DownloadArgs {
    /// Dependency kind: maven, npm, pypi, cargo, conan.
    #[arg(long, value_parser = clap::value_parser!(crate::model::DepKind))]
    pub kind: crate::model::DepKind,

    /// Dependency name (e.g. `org.apache.commons:commons-lang3` for Maven,
    /// `lodash` for npm, `requests` for PyPI, `serde` for Cargo, `zlib` for Conan).
    #[arg(long)]
    pub name: String,

    /// Dependency version (e.g. `3.14.0`, `4.17.21`).
    #[arg(long)]
    pub version: String,

    /// Output directory. Defaults to the current working directory.
    #[arg(long, default_value = ".")]
    pub output: String,
}

/// Arguments for the `import` subcommand.
#[derive(Debug, Parser)]
pub struct ImportArgs {
    /// Dependency kind: maven, npm, pypi, cargo, conan.
    #[arg(long, value_parser = clap::value_parser!(crate::model::DepKind))]
    pub kind: crate::model::DepKind,

    /// Dependency name.
    #[arg(long)]
    pub name: String,

    /// Dependency version.
    #[arg(long)]
    pub version: String,

    /// Path to the TOML configuration file.
    #[arg(long)]
    pub config: String,
}
