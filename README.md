# dep-porter

A CLI tool to download dependencies from the internet and import them into an air-gapped (intranet) Nexus repository.

## Use Cases

- Your organization uses an internal Nexus repository that cannot access the public internet.
- You need to transfer dependencies (Maven, npm, PyPI, Cargo, Conan) from a connected machine to a disconnected environment.
- You want a repeatable, automated process for syncing dependencies across network boundaries.

## Not Suitable For

- Environments where Nexus is directly connected to the internet (use Nexus proxy repositories instead).
- Real-time dependency resolution during builds (this is an offline batch tool).
- Dependencies that require platform-specific native compilation during download.

## Supported Dependency Types

| Kind  | Download | Import to Nexus | Notes |
|-------|----------|-----------------|-------|
| Maven | Yes      | Yes (Maven repo) | Full transitive dependency resolution via `mvn dependency:go-offline` |
| npm   | Yes      | Yes (npm repo)   | Downloads tarballs; uploads to Nexus npm-hosted |
| PyPI  | Yes      | Yes (PyPI repo)  | Uses `pip download` + `twine upload` |
| Cargo | Yes      | Yes (raw repo)   | Uses `cargo vendor`; uploads to raw repository |
| Conan | Yes      | Yes (raw repo)   | Uses `conan install`; uploads to raw repository |

## Prerequisites

- **Rust** 1.70+ (for building the CLI)
- **Docker** (for running the downloader container)
- **twine** (optional, for PyPI import on the target machine: `pip install twine`)

## Building

### Build the Rust CLI

```bash
cd dep-porter
cargo build --release
```

The binary will be at `target/release/dep-porter` (or `target\release\dep-porter.exe` on Windows).

### Build the Docker Downloader Image

```bash
docker build -f Dockerfile.downloader -t dep-downloader:latest .
```

This creates an Ubuntu 24.04 image with all required tools pre-installed:

- OpenJDK 17 + Maven
- Node.js 20 + npm
- Python 3 + pip + twine
- Rust + Cargo
- Conan

## Usage

### Phase 1: Download (on an internet-connected machine)

```bash
# Maven
dep-porter download --kind maven --name org.apache.commons:commons-lang3 --version 3.14.0

# npm
dep-porter download --kind npm --name lodash --version 4.17.21

# PyPI
dep-porter download --kind pypi --name requests --version 2.32.3

# Cargo
dep-porter download --kind cargo --name serde --version 1.0.203

# Conan
dep-porter download --kind conan --name zlib --version 1.2.13
```

Each command creates a directory named `{kind}_{safe_name}_{version}` containing all downloaded artifacts and their transitive dependencies.

### Phase 2: Transfer

Copy the following to your air-gapped machine:

1. The `dep-porter` binary (or source + `cargo build --release`)
2. The download directories (e.g. `maven_org.apache.commons_commons-lang3_3.14.0/`)
3. A `config.toml` file with your Nexus settings

### Phase 3: Import (on the air-gapped machine)

Create a `config.toml`:

```toml
[nexus]
base_url = "http://nexus.internal.example.com"
username = "admin"
password = "admin123"

[repositories]
maven = "maven-releases"
npm = "npm-hosted"
pypi = "pypi-hosted"
raw = "raw-hosted"
```

Then run:

```bash
# Maven
dep-porter import --kind maven --name org.apache.commons:commons-lang3 --version 3.14.0 --config config.toml

# npm
dep-porter import --kind npm --name lodash --version 4.17.21 --config config.toml

# PyPI
dep-porter import --kind pypi --name requests --version 2.32.3 --config config.toml

# Cargo
dep-porter import --kind cargo --name serde --version 1.0.203 --config config.toml

# Conan
dep-porter import --kind conan --name zlib --version 1.2.13 --config config.toml
```

## Nexus Repository Configuration

You need to create the following repositories in Nexus before importing:

| Repository Type | Suggested Name   | Format  | Type   |
|-----------------|------------------|---------|--------|
| Maven           | maven-releases   | maven2  | hosted |
| npm             | npm-hosted       | npm     | hosted |
| PyPI            | pypi-hosted      | pypi    | hosted |
| Raw (fallback)  | raw-hosted       | raw     | hosted |

The raw repository is used as a fallback for Cargo and Conan, which Nexus does not natively support in most versions.

## Testing

### Local tests (no external dependencies)

```bash
cargo test
```

All local logic tests (directory naming, Maven coordinate parsing, config parsing, etc.) run without Docker or Nexus.

### Docker E2E tests

Requires Docker and the downloader image built:

```bash
docker build -f Dockerfile.downloader -t dep-downloader:latest .
RUN_DOCKER_E2E=1 cargo test
```

### Nexus E2E tests

Requires a running Nexus instance and these environment variables:

```bash
export NEXUS_BASE_URL=http://nexus.example.com
export NEXUS_USERNAME=admin
export NEXUS_PASSWORD=admin123
export NEXUS_MAVEN_REPO=maven-releases
export NEXUS_RAW_REPO=raw-hosted
RUN_NEXUS_E2E=1 cargo test
```

## Directory Naming Convention

Download directories follow the pattern:

```
{kind}_{safe_name}_{version}
```

Where `safe_name` replaces `/`, `:`, `@`, `\` with `_`.

Examples:

```
maven_org.apache.commons_commons-lang3_3.14.0
npm_lodash_4.17.21
pypi_requests_2.32.3
cargo_serde_1.0.203
conan_zlib_1.2.13
```

## Frequently Asked Questions

**Q: Can I use this without Docker?**
A: The download phase requires Docker. The import phase does not.

**Q: What if Docker is not available on the air-gapped machine?**
A: You only need Docker for the download phase (on the connected machine). The import phase uses direct HTTP uploads to Nexus.

**Q: How are transitive dependencies handled?**
A: Each package manager's native resolution is used:
- Maven: `mvn dependency:go-offline`
- npm: `npm install`
- PyPI: `pip download`
- Cargo: `cargo fetch` + `cargo vendor`
- Conan: `conan install --build=missing`

**Q: What about Cargo and Conan support in Nexus?**
A: Nexus Repository OSS does not natively support Cargo or Conan formats. These are uploaded to a raw repository as a fallback. If your Nexus instance has Cargo/Conan support (Pro or plugins), you can adjust the repository names in `config.toml`.

**Q: Can I download multiple versions of the same package?**
A: Run separate download commands for each version. Each creates its own directory.

**Q: Does this work on Windows?**
A: Yes. The CLI is a Rust binary that works on Windows, Linux, and macOS. Docker path mounting handles Windows paths automatically.
