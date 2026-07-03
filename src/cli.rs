use clap::{Parser, Subcommand};

/// 一个从互联网下载依赖项并将其导入到气隙隔离的Nexus仓库中的CLI工具。
#[derive(Debug, Parser)]
#[command(name = "dep-porter", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// 下载依赖项及其所有传递依赖项。
    Download(DownloadArgs),
    /// 将先前下载的依赖项导入到Nexus。
    Import(ImportArgs),
}

/// `download`子命令的参数。
#[derive(Debug, Parser)]
pub struct DownloadArgs {
    /// 依赖类型：maven、npm、pypi、cargo、conan。
    #[arg(long, value_parser = clap::value_parser!(crate::model::DepKind))]
    pub kind: crate::model::DepKind,

    /// 依赖名称（例如Maven的`org.apache.commons:commons-lang3`，
    /// npm的`lodash`，PyPI的`requests`，Cargo的`serde`，Conan的`zlib`）。
    #[arg(long)]
    pub name: String,

    /// 依赖版本（例如`3.14.0`、`4.17.21`）。
    #[arg(long)]
    pub version: String,

    /// 输出目录。默认为当前工作目录。
    #[arg(long, default_value = ".")]
    pub output: String,

    /// 下载前通过OSV.dev检查已知漏洞。
    /// 如果发现漏洞，将提示您继续或中止。
    #[arg(long, default_value_t = false)]
    pub check_security: bool,
}

/// `import`子命令的参数。
#[derive(Debug, Parser)]
pub struct ImportArgs {
    /// 依赖类型：maven、npm、pypi、cargo、conan。
    #[arg(long, value_parser = clap::value_parser!(crate::model::DepKind))]
    pub kind: crate::model::DepKind,

    /// 依赖名称。
    #[arg(long)]
    pub name: String,

    /// 依赖版本。
    #[arg(long)]
    pub version: String,

    /// TOML配置文件路径。默认为当前目录下的config.toml。
    #[arg(long, default_value = "config.toml")]
    pub config: String,

    /// 覆盖Nexus中的现有工件。
    /// 如果为false（默认），则跳过现有工件。
    /// 如果为true，则覆盖现有工件（如果仓库策略禁止，可能会失败）。
    #[arg(long, default_value_t = false)]
    pub overwrite: bool,
}
