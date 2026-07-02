use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::model::{DepError, DepKind};

/// The default Docker image name for the downloader.
pub const DOWNLOADER_IMAGE: &str = "dep-downloader:latest";

/// Build a safe directory name from a dependency specification.
///
/// Format: `{kind}_{safe_name}_{version}`
///
/// `safe_name` replaces `/`, `:`, `@`, `\` with `_`.
pub fn build_dir_name(kind: DepKind, name: &str, version: &str) -> String {
    let safe_name = name
        .replace('/', "_")
        .replace(':', "_")
        .replace('@', "_")
        .replace('\\', "_");
    format!("{}_{}_{}", kind.as_str(), safe_name, version)
}

/// Convert a Windows host path to a Docker-compatible mount path.
///
/// On Windows, `C:\Users\foo` becomes `/c/Users/foo`.
/// On Linux/macOS, the path is returned as-is.
pub fn to_docker_mount_path(path: &Path) -> String {
    let s = path.to_string_lossy().to_string();
    if cfg!(target_os = "windows") {
        // C:\Users\foo -> /c/Users/foo
        if s.len() >= 2 && s.as_bytes()[1] == b':' {
            let drive = (s.as_bytes()[0] as char).to_lowercase().to_string();
            let rest = &s[2..].replace('\\', "/");
            return format!("/{}{}", drive, rest);
        }
    }
    s
}

/// Collect all files recursively under a directory.
pub fn collect_files(dir: &Path) -> Result<Vec<PathBuf>, DepError> {
    if !dir.exists() {
        return Err(DepError::DownloadDirNotFound(dir.display().to_string()));
    }
    let mut files = Vec::new();
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}

/// Collect all files recursively and return them sorted.
pub fn collect_files_sorted(dir: &Path) -> Result<Vec<PathBuf>, DepError> {
    let mut files = collect_files(dir)?;
    files.sort();
    Ok(files)
}

/// Compute the relative path of a file with respect to a base directory.
pub fn relative_path<'a>(base: &'a Path, file: &'a Path) -> Option<&'a Path> {
    file.strip_prefix(base).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_dir_name_maven() {
        let name = build_dir_name(DepKind::Maven, "org.apache.commons:commons-lang3", "3.14.0");
        assert_eq!(name, "maven_org.apache.commons_commons-lang3_3.14.0");
    }

    #[test]
    fn test_build_dir_name_npm() {
        let name = build_dir_name(DepKind::Npm, "lodash", "4.17.21");
        assert_eq!(name, "npm_lodash_4.17.21");
    }

    #[test]
    fn test_build_dir_name_pypi() {
        let name = build_dir_name(DepKind::Pypi, "requests", "2.32.3");
        assert_eq!(name, "pypi_requests_2.32.3");
    }

    #[test]
    fn test_build_dir_name_cargo() {
        let name = build_dir_name(DepKind::Cargo, "serde", "1.0.203");
        assert_eq!(name, "cargo_serde_1.0.203");
    }

    #[test]
    fn test_build_dir_name_conan() {
        let name = build_dir_name(DepKind::Conan, "zlib", "1.2.13");
        assert_eq!(name, "conan_zlib_1.2.13");
    }

    #[test]
    fn test_build_dir_name_special_chars() {
        let name = build_dir_name(DepKind::Npm, "@angular/core", "17.0.0");
        assert_eq!(name, "npm__angular_core_17.0.0");
    }

    #[test]
    fn test_maven_coordinate_parse() {
        use crate::model::MavenCoordinate;
        let coord = MavenCoordinate::parse("org.apache.commons:commons-lang3").unwrap();
        assert_eq!(coord.group_id, "org.apache.commons");
        assert_eq!(coord.artifact_id, "commons-lang3");
        assert_eq!(coord.group_path(), "org/apache/commons");
    }

    #[test]
    fn test_maven_coordinate_parse_invalid() {
        use crate::model::MavenCoordinate;
        assert!(MavenCoordinate::parse("no-colon").is_err());
        assert!(MavenCoordinate::parse(":artifact").is_err());
        assert!(MavenCoordinate::parse("group:").is_err());
        assert!(MavenCoordinate::parse("a:b:c").is_err());
    }

    #[test]
    fn test_dep_kind_from_str() {
        use std::str::FromStr;
        assert_eq!(DepKind::from_str("maven").unwrap(), DepKind::Maven);
        assert_eq!(DepKind::from_str("NPM").unwrap(), DepKind::Npm);
        assert_eq!(DepKind::from_str("pip").unwrap(), DepKind::Pypi);
        assert_eq!(DepKind::from_str("python").unwrap(), DepKind::Pypi);
        assert_eq!(DepKind::from_str("rust").unwrap(), DepKind::Cargo);
        assert_eq!(DepKind::from_str("cpp").unwrap(), DepKind::Conan);
        assert_eq!(DepKind::from_str("c++").unwrap(), DepKind::Conan);
        assert!(DepKind::from_str("unknown").is_err());
    }

    #[test]
    fn test_relative_path() {
        let base = Path::new("/foo/bar");
        let file = Path::new("/foo/bar/baz/qux.txt");
        let rel = relative_path(base, file).unwrap();
        assert_eq!(rel, Path::new("baz/qux.txt"));
    }
}
