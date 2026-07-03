---
name: dep-porter
description: "Use when the user needs to download dependencies (Maven/npm/PyPI/Cargo/Conan) from the internet and import them into an air-gapped Nexus repository. Triggers: 'download dependencies for Nexus', 'import to Nexus', 'offline dependency transfer', 'air-gapped dependency sync', '搬运依赖到内网'."
version: 1.0.0
requires:
  - Docker (download phase only)
---

# dep-porter

Transfer dependencies from public registries to an air-gapped Nexus.

## When to use

User has an internal Nexus that cannot access the internet and needs to transfer packages (with transitive dependencies) across the network boundary.

**Do not use** when Nexus can reach the internet directly — configure Nexus proxy repositories instead.

## Installation

Download pre-built binaries from [GitHub Releases](https://github.com/gudaoxuri/dep-porter/releases):

| Platform | File |
|----------|------|
| Linux x86_64 | `dep-porter-linux-amd64` |
| Linux ARM64 | `dep-porter-linux-arm64` |
| Windows x86_64 | `dep-porter-windows-amd64.exe` |
| macOS x86_64 | `dep-porter-macos-amd64` |
| macOS ARM64 | `dep-porter-macos-arm64` |

## Quick reference

```bash
# Download (internet-connected machine, requires Docker)
dep-porter download --kind <kind> --name <name> --version <ver> [--check-security]

# Import (air-gapped machine, reads config.toml from cwd by default)
dep-porter import --kind <kind> --name <name> --version <ver> [--overwrite]
```

## Dependency name format

| Kind  | `--name` format          | Example                      |
|-------|--------------------------|------------------------------|
| Maven | `groupId:artifactId`     | `org.apache.commons:commons-lang3` |
| npm   | package name             | `lodash`                     |
| PyPI  | package name             | `requests`                   |
| Cargo | crate name               | `serde`                      |
| Conan | package name             | `zlib`                       |

## Import routing

| Kind  | Target repo                     | Fallback         |
|-------|--------------------------------|------------------|
| Maven | `maven` (or `maven_snapshots` if version contains `-SNAPSHOT`) | —                |
| npm   | `npm`                          | —                |
| PyPI  | `pypi` (via twine)             | —                |
| Cargo | `cargo`                        | `raw`            |
| Conan | `raw`                          | —                |

All transitive dependencies are uploaded. `_remote.repositories`, `resolver-status.properties`, and `maven-metadata-*.xml` files are automatically filtered out.

## Overwrite behavior

- Default: **skip-if-exists** — HEAD check before each file, skip if 200
- `--overwrite`: PUT directly. Fails if Nexus repo policy is `ALLOW_ONCE`

## Security check

`--check-security` queries [OSV.dev](https://osv.dev) for known CVEs before downloading. Supported for Maven/npm/PyPI/Cargo. Not supported for Conan.

When vulnerabilities are found, displays CVE IDs + CVSS scores and prompts `[y/N]` to continue.

## Config file

`config.toml` (in cwd, or specify with `--config`):

```toml
[nexus]
base_url = "http://nexus.internal.example.com"
username = "admin"
password = "admin123"

[repositories]
maven = "maven-releases"
# maven_snapshots = "maven-snapshots"  # optional
npm = "npm-hosted"
pypi = "pypi-hosted"
# cargo = "cargo-hosted"               # optional, falls back to raw
raw = "raw-hosted"
```

## Workflow: download → transfer → import

```
- [ ] Step 1: Download on internet machine
      dep-porter download --kind maven --name org.apache.commons:commons-lang3 --version 3.14.0
      → creates maven_org.apache.commons_commons-lang3_3.14.0/

- [ ] Step 2: Copy to air-gapped machine
      - dep-porter binary (from GitHub Releases)
      - Download directory
      - config.toml

- [ ] Step 3: Import on air-gapped machine
      dep-porter import --kind maven --name org.apache.commons:commons-lang3 --version 3.14.0
```

## Gotchas

- Docker image `gudaoxuri/dep-downloader:latest` is automatically pulled from Docker Hub on first run.
- Maven `dependency:get` is used (not `dependency:go-offline`) to avoid pulling Maven plugin dependencies.
- Maven SNAPSHOT versions: files containing `-SNAPSHOT` in their path are uploaded to `maven_snapshots` repo, other files (transitive deps) are uploaded to `maven` repo.
- SNAPSHOT detection is case-insensitive: `-SNAPSHOT`, `-snapshot`, `-Snapshot` all route to `maven_snapshots`.
- Cargo imports try `cargo` repo first; on any failure (404, 500, connection error), automatically fall back to `raw`.
- Conan security check is not available — OSV.dev does not index Conan packages.
- Windows paths are auto-converted (`C:\foo` → `/c/foo`) for Docker mounts.
- `--config` defaults to `config.toml` in the current directory. The file must exist or the command fails.
