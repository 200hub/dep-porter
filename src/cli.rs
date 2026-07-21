use std::path::PathBuf;

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
    /// 从依赖文件（pom.xml、package.json 等）批量下载所有直接依赖及其传递依赖。
    BatchDownload(BatchDownloadArgs),
    /// 从依赖文件批量导入先前下载的所有依赖到Nexus。
    BatchImport(BatchImportArgs),
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
    /// 默认开启，使用 --no-check-security 可关闭。
    #[arg(long, default_value_t = true, overrides_with = "no_check_security")]
    pub check_security: bool,

    /// 关闭安全漏洞检查。
    #[arg(long, hide = true)]
    pub no_check_security: bool,

    /// 下载前通过deps.dev检查许可证的商用风险。
    /// 默认开启，使用 --no-check-license 可关闭。
    #[arg(long, default_value_t = true, overrides_with = "no_check_license")]
    pub check_license: bool,

    /// 关闭许可证商用风险检查。
    #[arg(long, hide = true)]
    pub no_check_license: bool,

    /// 下载缓存根目录。默认使用DEP_PORTER_CACHE_DIR或当前目录下的.dep-porter-cache。
    #[arg(long, value_name = "DIR")]
    pub cache_dir: Option<PathBuf>,

    /// 关闭下载缓存。
    #[arg(long, conflicts_with = "cache_dir")]
    pub no_cache: bool,
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

/// `batch-download`子命令的参数。
#[derive(Debug, Parser)]
pub struct BatchDownloadArgs {
    /// 依赖文件路径：pom.xml、package.json、package-lock.json、
    /// requirements.txt、Cargo.toml、Cargo.lock、conanfile.txt。
    #[arg(long)]
    pub file: PathBuf,

    /// 输出目录。每个依赖会在其下生成独立的 `{类型}_{名称}_{版本}` 子目录。默认为当前工作目录。
    #[arg(long, default_value = ".")]
    pub output: String,

    /// 一并解析开发/构建期依赖（npm devDependencies、cargo dev/build-dependencies）。
    #[arg(long, default_value_t = false)]
    pub include_dev: bool,

    /// 下载前通过OSV.dev检查已知漏洞。
    /// 默认开启，使用 --no-check-security 可关闭。
    #[arg(long, default_value_t = true, overrides_with = "no_check_security")]
    pub check_security: bool,

    /// 关闭安全漏洞检查。
    #[arg(long, hide = true)]
    pub no_check_security: bool,

    /// 下载前通过deps.dev检查许可证的商用风险。
    /// 默认开启，使用 --no-check-license 可关闭。
    #[arg(long, default_value_t = true, overrides_with = "no_check_license")]
    pub check_license: bool,

    /// 关闭许可证商用风险检查。
    #[arg(long, hide = true)]
    pub no_check_license: bool,

    /// 下载缓存根目录。默认使用DEP_PORTER_CACHE_DIR或当前目录下的.dep-porter-cache。
    #[arg(long, value_name = "DIR")]
    pub cache_dir: Option<PathBuf>,

    /// 关闭下载缓存。
    #[arg(long, conflicts_with = "cache_dir")]
    pub no_cache: bool,
}

/// `batch-import`子命令的参数。
#[derive(Debug, Parser)]
pub struct BatchImportArgs {
    /// 依赖文件路径（应与 batch-download 时使用的文件一致）。
    #[arg(long)]
    pub file: PathBuf,

    /// 下载目录所在的基准目录（应与 batch-download 的 --output 一致）。默认为当前工作目录。
    #[arg(long, default_value = ".")]
    pub input: String,

    /// 一并导入开发/构建期依赖（应与 batch-download 的 --include-dev 一致）。
    #[arg(long, default_value_t = false)]
    pub include_dev: bool,

    /// TOML配置文件路径。默认为当前目录下的config.toml。
    #[arg(long, default_value = "config.toml")]
    pub config: String,

    /// 覆盖Nexus中的现有工件。
    #[arg(long, default_value_t = false)]
    pub overwrite: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_checks_security_and_license_by_default() {
        let cli = Cli::try_parse_from([
            "dep-porter",
            "download",
            "--kind",
            "npm",
            "--name",
            "lodash",
            "--version",
            "4.17.21",
        ])
        .unwrap();

        let Command::Download(args) = cli.command else {
            panic!("expected download command");
        };
        assert!(args.check_security);
        assert!(!args.no_check_security);
        assert!(args.check_license);
        assert!(!args.no_check_license);
        assert!(args.cache_dir.is_none());
        assert!(!args.no_cache);
    }

    #[test]
    fn download_license_check_can_be_disabled() {
        let cli = Cli::try_parse_from([
            "dep-porter",
            "download",
            "--kind",
            "npm",
            "--name",
            "lodash",
            "--version",
            "4.17.21",
            "--no-check-license",
        ])
        .unwrap();

        let Command::Download(args) = cli.command else {
            panic!("expected download command");
        };
        assert!(args.no_check_license);
        assert!(!(args.check_license && !args.no_check_license));
    }

    #[test]
    fn download_cache_can_be_configured() {
        let cli = Cli::try_parse_from([
            "dep-porter",
            "download",
            "--kind",
            "npm",
            "--name",
            "lodash",
            "--version",
            "4.17.21",
            "--cache-dir",
            "custom-cache",
        ])
        .unwrap();

        let Command::Download(args) = cli.command else {
            panic!("expected download command");
        };
        assert_eq!(args.cache_dir, Some(PathBuf::from("custom-cache")));
        assert!(!args.no_cache);
    }

    #[test]
    fn download_cache_can_be_disabled() {
        let cli = Cli::try_parse_from([
            "dep-porter",
            "download",
            "--kind",
            "npm",
            "--name",
            "lodash",
            "--version",
            "4.17.21",
            "--no-cache",
        ])
        .unwrap();

        let Command::Download(args) = cli.command else {
            panic!("expected download command");
        };
        assert!(args.cache_dir.is_none());
        assert!(args.no_cache);
    }

    #[test]
    fn cache_directory_and_no_cache_are_mutually_exclusive() {
        let result = Cli::try_parse_from([
            "dep-porter",
            "download",
            "--kind",
            "npm",
            "--name",
            "lodash",
            "--version",
            "4.17.21",
            "--cache-dir",
            "custom-cache",
            "--no-cache",
        ]);

        assert!(result.is_err());
    }

    #[test]
    fn batch_download_parses_file_and_defaults() {
        let cli =
            Cli::try_parse_from(["dep-porter", "batch-download", "--file", "pom.xml"]).unwrap();

        let Command::BatchDownload(args) = cli.command else {
            panic!("expected batch-download command");
        };
        assert_eq!(args.file, PathBuf::from("pom.xml"));
        assert_eq!(args.output, ".");
        assert!(!args.include_dev);
        assert!(args.check_security);
        assert!(args.check_license);
        assert!(!args.no_cache);
    }

    #[test]
    fn batch_download_flags_can_be_set() {
        let cli = Cli::try_parse_from([
            "dep-porter",
            "batch-download",
            "--file",
            "package.json",
            "--include-dev",
            "--no-check-security",
            "--no-cache",
        ])
        .unwrap();

        let Command::BatchDownload(args) = cli.command else {
            panic!("expected batch-download command");
        };
        assert!(args.include_dev);
        assert!(args.no_check_security);
        assert!(args.no_cache);
    }

    #[test]
    fn batch_import_parses_file_and_defaults() {
        let cli =
            Cli::try_parse_from(["dep-porter", "batch-import", "--file", "Cargo.lock"]).unwrap();

        let Command::BatchImport(args) = cli.command else {
            panic!("expected batch-import command");
        };
        assert_eq!(args.file, PathBuf::from("Cargo.lock"));
        assert_eq!(args.input, ".");
        assert_eq!(args.config, "config.toml");
        assert!(!args.overwrite);
        assert!(!args.include_dev);
    }
}
