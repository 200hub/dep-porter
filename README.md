# dep-porter

内网 Nexus 离线依赖搬运工具。

将 Maven、npm、PyPI、Cargo、Conan 依赖从外网下载，再导入内网 Nexus 仓库。

## 适用场景

- 内网 Nexus 无法访问公网，需要人工搬运依赖
- 需要批量下载依赖及其所有传递依赖
- 需要可重复、可自动化的跨网依赖同步流程

## 不适用场景

- Nexus 可直接联网（应使用 Nexus 代理仓库）
- 构建时实时解析依赖（本工具是离线批量工具）
- 下载过程中需要平台特定原生编译的依赖

## 支持的依赖类型

| 类型  | 下载 | 导入 Nexus | 说明                                                                   |
| ----- | ---- | ---------- | ---------------------------------------------------------------------- |
| Maven | ✅    | Maven 仓库 | `mvn dependency:get` 递归下载；按 Maven2 布局逐文件上传                |
| npm   | ✅    | npm 仓库   | 打包依赖树中全部包，按 npm publish 协议发布（含传递依赖）          |
| PyPI  | ✅    | PyPI 仓库  | `pip download` + `twine upload`                                        |
| Cargo | ✅    | Cargo 仓库 | 复制真实 `.crate`，按 Cargo registry publish 协议发布到原生 Cargo 仓库 |
| Conan | ✅    | raw 仓库   | `conan install` 缓存配方（Nexus 原生不支持，走 raw 兜底）              |

> **重要**：npm 与 Cargo 的导入使用各自的原生发布协议（npm publish 文档、Cargo `PUT /api/v1/crates/new`），
> 而不是把文件堆到任意路径。这样上传后的包才能被 `npm install` / `cargo build` 正常解析和下载。
> 导入端全部通过 HTTP 实现，**无需安装 npm / cargo**（仅 PyPI 仍依赖 `twine`）。

---

## 使用指南

### 环境要求

| 工具       | 用途           | 阶段             |
| ---------- | -------------- | ---------------- |
| Docker     | 运行下载器容器 | 外网下载         |
| twine      | PyPI 导入      | 内网导入（可选） |

### 1. 获取程序

从 [GitHub Releases](https://github.com/200hub/dep-porter/releases) 下载对应平台的预编译版本：

| 平台 | 文件 |
|------|------|
| Linux x86_64 | `dep-porter-linux-amd64.tar.gz` |
| Linux ARM64 | `dep-porter-linux-arm64.tar.gz` |
| Windows x86_64 | `dep-porter-windows-amd64.zip` |
| macOS x86_64 | `dep-porter-macos-amd64.tar.gz` |
| macOS ARM64 | `dep-porter-macos-arm64.tar.gz` |

解压后得到 `dep-porter`（Windows 为 `dep-porter.exe`）。

```bash
# Linux/macOS
tar -xzf dep-porter-linux-amd64.tar.gz
./dep-porter --help

# Windows (PowerShell)
Expand-Archive dep-porter-windows-amd64.zip
.\dep-porter.exe --help
```

### 2. 外网下载（联网机器）

下载时会自动拉取 Docker 镜像 `gudaoxuri/dep-downloader:latest`。

```bash
# Maven（正式版本）
dep-porter download --kind maven --name org.apache.commons:commons-lang3 --version 3.14.0

# Maven（SNAPSHOT 快照版本）
dep-porter download --kind maven --name org.apache.commons:commons-lang3 --version 3.15.1-SNAPSHOT

# npm
dep-porter download --kind npm --name @h-ai/serv --version 0.1.0-alpha.31

# PyPI
dep-porter download --kind pypi --name requests --version 2.32.3

# Cargo
dep-porter download --kind cargo --name tardis --version 0.1.0-rc.19

# Conan
dep-porter download --kind conan --name zlib --version 1.2.13

# 跳过安全漏洞检查（默认会检查）
dep-porter download --kind maven --name log4j:log4j --version 1.2.17 --no-check-security
```

每个命令生成一个目录：`{类型}_{安全名称}_{版本}/`，包含所有下载的依赖及其传递依赖。导入时整个目录的所有依赖（包括传递依赖）都会上传到 Nexus。

#### 镜像源配置

下载时默认使用国内镜像源加速，可通过环境变量自定义：

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `MAVEN_MIRROR` | `https://maven.aliyun.com/repository/central` | Maven 中央仓库镜像 |
| `NPM_MIRROR` | `https://registry.npmmirror.com` | npm 镜像 |
| `PYPI_MIRROR` | `https://mirrors.aliyun.com/pypi/simple` | PyPI 镜像 |
| `CARGO_MIRROR` | `https://mirrors.ustc.edu.cn/crates.io-index` | Cargo 镜像 |

```bash
# 使用默认镜像源（国内加速）
dep-porter download --kind maven --name junit:junit --version 4.13.2

# 自定义镜像源
MAVEN_MIRROR=https://maven-central.storage.googleapis.com \
dep-porter download --kind maven --name junit:junit --version 4.13.2

# 使用官方源（海外服务器）
MAVEN_MIRROR=https://repo1.maven.org/maven2 \
NPM_MIRROR=https://registry.npmjs.org \
PYPI_MIRROR=https://pypi.org/simple \
CARGO_MIRROR=https://index.crates.io \
dep-porter download --kind maven --name junit:junit --version 4.13.2
```

### 3. 拷贝到内网

将以下内容拷贝到内网机器：

1. `dep-porter` 二进制文件
2. 下载目录（如 `maven_org.apache.commons_commons-lang3_3.14.0/`）
3. `config.toml` 配置文件

### 4. 内网导入

创建 `config.toml`：

```toml
[nexus]
base_url = "http://nexus.internal.example.com"
username = "admin"
password = "admin123"

[repositories]
maven = "maven-releases"
# maven_snapshots = "maven-snapshots"   # 可选，不填则 SNAPSHOT 版本也发到 maven
npm = "npm-hosted"
pypi = "pypi-hosted"
# cargo = "cargo-hosted"                # 可选，不填则走 raw 兜底
raw = "raw-hosted"
```

执行导入：

```bash
# Maven 正式版本 → 发到 maven 仓库
dep-porter import --kind maven --name org.apache.commons:commons-lang3 --version 3.14.0

# Maven SNAPSHOT → 自动发到 maven_snapshots 仓库（如果配置了的话）
dep-porter import --kind maven --name org.apache.commons:commons-lang3 --version 3.14.0-SNAPSHOT

dep-porter import --kind npm --name lodash --version 4.17.21
dep-porter import --kind pypi --name requests --version 2.32.3

# Cargo 优先走 cargo 仓库，失败自动降级到 raw
dep-porter import --kind cargo --name serde --version 1.0.203

dep-porter import --kind conan --name zlib --version 1.2.13

# 覆盖模式：已存在则覆盖（受仓库写策略限制）
dep-porter import --kind maven --name junit:junit --version 4.13.2 --overwrite
```

> `--config` 默认读取当前目录的 `config.toml`，可省略。如需指定其他路径：`--config /path/to/config.toml`

### Nexus 仓库配置

导入前需要在 Nexus 创建以下仓库：

| 仓库类型        | 建议名称        | 格式      | 类型   | 说明                                                                    |
| --------------- | --------------- | --------- | ------ | ----------------------------------------------------------------------- |
| Maven releases  | maven-releases  | maven2    | hosted | 正式版本                                                                |
| Maven snapshots | maven-snapshots | maven2    | hosted | 快照版本（可选，不配则也发到 maven）                                    |
| npm             | npm-hosted      | npm       | hosted | 原生 npm 格式                                                           |
| PyPI            | pypi-hosted     | pypi      | hosted | 原生 pypi 格式                                                          |
| Cargo           | cargo-hosted    | cargo | hosted | 原生 Cargo 格式（需 Nexus 3.74+，仅支持 sparse 协议）；未配置时降级 raw |
| raw（兜底）     | raw-hosted      | raw       | hosted | Conan 以及无 Cargo 仓库时的 fallback                                    |

**Maven**：版本号包含 `-SNAPSHOT` 的自动发到 `maven_snapshots` 仓库，其余发到 `maven` 仓库。

**Cargo**：上传到原生 `cargo` 格式仓库（Nexus 3.74+），使用 Cargo registry publish 协议（`PUT /api/v1/crates/new`）逐个发布真实 `.crate`，发布后可通过 sparse 协议被 `cargo` 解析。若未配置 `cargo` 仓库，则降级把文件堆到 `raw`（**此时 `cargo` 客户端无法使用**，仅作留存）。

**npm**：使用 npm publish 协议发布依赖树中的每一个 tarball（目标包 + 全部传递依赖），发布后可被 `npm install` 正常解析。

**覆盖模式**：
- 默认模式（skip-if-exists）：上传前先 HEAD 检查，已存在则跳过
- `--overwrite` 模式：直接 PUT 覆盖，如果仓库写策略禁止覆盖（如 `ALLOW_ONCE`）会报错

### 内网消费已导入的依赖

导入完成后，内网开发机需要把包管理器指向 Nexus 才能使用这些依赖。

**Cargo**（在项目根或 `~/.cargo/config.toml`）：

```toml
[registries.nexus]
index = "sparse+http://nexus.internal.example.com/repository/cargo/"

# 让所有 crates.io 依赖都从 Nexus 解析
[source.crates-io]
replace-with = "nexus"

[source.nexus]
registry = "sparse+http://nexus.internal.example.com/repository/cargo/"
```

Nexus Cargo 仓库需要认证，凭据以 token 形式提供（`Basic base64(user:password)`）：

```bash
# 例如 admin:admin123 → Basic YWRtaW46YWRtaW4xMjM=
export CARGO_REGISTRIES_NEXUS_TOKEN="Basic YWRtaW46YWRtaW4xMjM="
cargo build
```

**npm**（`.npmrc`）：

```ini
registry=http://nexus.internal.example.com/repository/npm/
//nexus.internal.example.com/repository/npm/:_auth=BASE64(user:password)
```

```bash
npm install     # 依赖将从 Nexus 拉取
```

**Maven**（`~/.m2/settings.xml`）：把 `<mirror>` 指向 `maven-releases`/`maven-public`，即可解析已导入的构件。

**PyPI**：`pip install --index-url http://user:password@nexus.internal.example.com/repository/pypi/simple/ <包名>`。

### 目录命名规则

```
{kind}_{safe_name}_{version}
```

`safe_name` 将 `/`、`:`、`@`、`\` 替换为 `_`。

示例：

```
maven_org.apache.commons_commons-lang3_3.14.0
npm_lodash_4.17.21
pypi_requests_2.32.3
cargo_serde_1.0.203
conan_zlib_1.2.13
```

### 安全检查（SCA）

下载时**默认开启**漏洞扫描，通过 [OSV.dev](https://osv.dev) API 查询已知 CVE。如需跳过检查，使用 `--no-check-security`：

```bash
# 默认会检查安全漏洞
dep-porter download --kind maven --name log4j:log4j --version 2.14.1

# 跳过安全检查
dep-porter download --kind maven --name log4j:log4j --version 2.14.1 --no-check-security
```

如果发现漏洞，会显示详情并提示是否继续：

```
=== Security Advisory ===
  maven log4j:log4j@2.14.1
  Found 2 known vulnerability(ies):

  [1] GHSA-26fg-89j6-3q3j (CVSS: 9.8) — Apache Log4j2 Remote Code Execution
  [2] CVE-2021-44228 (CVSS: 10.0) — Log4j 2.x JNDI RCE
=========================

Continue download anyway? [y/N]
```

- 输入 `y` 继续下载，输入其他或直接回车取消
- 支持 Maven/npm/PyPI/Cargo，Conan 暂不支持（OSV.dev 无此生态）

### 常见问题

**Q: 没有 Docker 能用吗？**
下载阶段需要 Docker，导入阶段不需要。

**Q: 内网机器没有 Docker 怎么办？**
Docker 只在下载阶段使用（联网机器）。导入阶段通过 HTTP 直接上传到 Nexus。

**Q: 传递依赖怎么处理？**
各包管理器原生解析，下载时获取完整依赖树，导入时**逐个**上传（包含所有传递依赖）：
- Maven：`mvn dependency:get -Dtransitive=true`，拷贝本地仓库整体上传
- npm：`npm install` 解析依赖树，对树中每个包 `npm pack` 取真实 tarball
- PyPI：`pip download`（含传递依赖）
- Cargo：`cargo fetch` 拉取整个依赖图，从本地 registry 缓存拷贝真实 `.crate` + 保存 crates.io sparse-index 元数据
- Conan：`conan install`

**Q: Maven SNAPSHOT 版本怎么处理？**
版本号包含 `-SNAPSHOT` 的自动发到 `maven_snapshots` 仓库（如果配置了），否则也发到 `maven` 仓库。

**Q: Cargo/Conan 在 Nexus 里不支持怎么办？**
Nexus 3.74+ 原生支持 Cargo（sparse 协议），在 `config.toml` 配置 `cargo` 仓库名即可。若 Nexus 无 Cargo 支持，不配 `cargo`，将降级到 raw 仓库兜底（仅留存，`cargo` 客户端无法直接使用）。Conan 始终走 raw 兜底。

**Q: 能下载同一包的多个版本吗？**
可以，每个版本执行一次 `download`，各自生成独立目录。

**Q: 导入时已存在的制品怎么处理？**
默认跳过（skip-if-exists），加 `--overwrite` 则覆盖。覆盖受 Nexus 仓库写策略限制，`ALLOW_ONCE` 策略下覆盖会返回 4xx 错误。

**Q: 安全检查会拖慢下载速度吗？**
安全检查是单次 HTTP 请求（OSV.dev API），通常 < 1 秒。使用 `--no-check-security` 可跳过检查。

**Q: 支持 Windows 吗？**
支持。CLI 是 Rust 二进制，Windows/Linux 均可运行。Docker 路径挂载会自动处理 Windows 路径。

---

## 开发指南

### 技术栈

- **语言**：Rust 2021 edition
- **CLI**：clap 4（derive 宏）
- **配置**：serde + toml
- **HTTP**：reqwest（blocking）
- **Docker 调用**：std::process::Command
- **错误处理**：anyhow + thiserror

### 项目结构

```
dep-porter/
├── Cargo.toml               # Rust 项目配置
├── Dockerfile.downloader     # Docker 下载器镜像（多阶段构建）
├── SKILL.md                  # LLM 使用指南
├── config.example.toml       # Nexus 配置示例
├── .github/
│   └── workflows/
│       ├── docker-publish.yml  # GitHub Actions: Docker 镜像构建与发布
│       └── build-release.yml   # GitHub Actions: 跨平台程序构建
├── scripts/
│   └── download.sh           # 容器内下载脚本（Maven/npm/PyPI/Cargo/Conan）
├── src/
│   ├── main.rs               # 入口
│   ├── cli.rs                # clap 参数解析
│   ├── config.rs             # TOML 配置读取
│   ├── docker.rs             # Docker 调用 + 镜像自动拉取
│   ├── import.rs             # Nexus 上传逻辑（含 overwrite/skip）
│   ├── registry.rs           # 原生发布载荷构造（Cargo publish / npm publish）
│   ├── model.rs              # 数据类型定义
│   ├── security.rs           # SCA 安全检查（OSV.dev API）
│   └── util.rs               # 工具函数（目录命名、路径转换）
└── tests/
    ├── e2e.rs                # 本地逻辑测试 + 可选 Docker/Nexus E2E
    └── docker_e2e.rs         # Docker E2E 测试（覆盖全部 5 种依赖类型）
```

### 编译

```bash
cd dep-porter
cargo build --release
```

产物：`target/release/dep-porter`（Windows: `target\release\dep-porter.exe`）

### 添加新的依赖类型

1. `src/model.rs`：在 `DepKind` 枚举中添加新变体
2. `scripts/download.sh`：添加对应的 `download_xxx()` 函数和 case 分支
3. `src/import.rs`：添加对应的导入逻辑（如需原生发布协议，在 `src/registry.rs` 添加可单测的载荷构造函数）
4. `tests/docker_e2e.rs`：添加工具可用性测试和下载测试
5. `Dockerfile.downloader`：如需新工具，在 builder/runtime 阶段安装

### 测试

#### 本地测试（不需要 Docker）

```bash
cargo test
```

测试内容：目录命名、Maven 坐标解析、配置解析、错误处理等。

#### Docker E2E 测试

需要 Docker 环境：

```bash
# 运行 Docker E2E 测试（覆盖 Maven/npm/PyPI/Cargo/Conan 全部 5 种类型）
# 首次运行会自动拉取 gudaoxuri/dep-downloader:latest 镜像
$env:RUN_DOCKER_E2E="1"   # PowerShell
cargo test --test docker_e2e -- --test-threads=1
```

测试内容：镜像拉取、全部工具可用性（mvn/java/node/npm/python/pip/twine/cargo/rustc/conan）、每种依赖类型的下载功能、目录命名验证、错误处理。

#### Nexus E2E 测试

Nexus E2E 测试会向**真实运行的 Nexus** 发布各类型构件，并验证它们能以正确的原生格式被检索/使用（即真正可用）。测试默认读取项目根目录的 `config.toml`，也可用 `NEXUS_*` 环境变量覆盖。

覆盖的验证（`tests/e2e.rs`）：

| 测试                                 | 内容                                                                          |
| ------------------------------------ | ----------------------------------------------------------------------------- |
| `test_e2e_cargo_publish_and_resolve` | 发布带传递依赖的 crate，校验 sparse index 列出版本与依赖、download 端点可下载 |
| `test_e2e_npm_publish_and_resolve`   | 发布目标包与传递依赖，校验 packument 记录依赖、dist tarball 可下载            |
| `test_e2e_maven_publish_and_resolve` | 上传 jar+pom，校验构件可检索                                                  |
| `test_e2e_raw_upload`                | Conan/raw 兜底上传，校验可检索                                                |

测试会为每次运行生成唯一版本号，避免与已发布构件冲突。

**方式 A：使用项目 `config.toml`（推荐）**

确保根目录 `config.toml` 指向可用的 Nexus（默认 `http://localhost:8081`，仓库名 `maven-releases`/`npm`/`pypi`/`cargo`/`raw`）：

```bash
# Linux/macOS
export RUN_NEXUS_E2E=1
cargo test --test e2e -- --test-threads=1
```

```powershell
# PowerShell
$env:RUN_NEXUS_E2E="1"
cargo test --test e2e -- --test-threads=1
```

**方式 B：用环境变量覆盖仓库/凭据**

```bash
export RUN_NEXUS_E2E=1
export NEXUS_BASE_URL=http://localhost:8081
export NEXUS_USERNAME=admin
export NEXUS_PASSWORD=<密码>
export NEXUS_MAVEN_REPO=maven-releases
export NEXUS_NPM_REPO=npm
export NEXUS_PYPI_REPO=pypi
export NEXUS_CARGO_REPO=cargo
export NEXUS_RAW_REPO=raw
# 可选：export NEXUS_MAVEN_SNAPSHOTS_REPO=maven-snapshots
cargo test --test e2e -- --test-threads=1
```

> 若要自建测试 Nexus，需创建以下 hosted 仓库：maven2（maven-releases）、npm、pypi、**cargo（format=cargo，Nexus 3.74+）**、raw。

#### 端到端完整测试（下载 + 导入）

验证从下载到导入的完整流程：

```bash
# 1. 启动 Nexus
docker run -d --name nexus-test -p 8081:8081 sonatype/nexus3:latest
# 等待 1-2 分钟启动完成

# 2. 获取密码
NEXUS_PASS=$(docker exec nexus-test cat /nexus-data/admin.password)
echo "Nexus password: $NEXUS_PASS"

# 3. 创建仓库（通过 Nexus REST API）
curl -u "admin:$NEXUS_PASS" -X POST "http://localhost:8081/service/rest/v1/repositories/maven2/hosted" \
  -H "Content-Type: application/json" \
  -d '{"name":"maven-releases","online":true,"storage":{"blobStoreName":"default","strictContentTypeValidation":true,"writePolicy":"ALLOW_ONCE"}}'

curl -u "admin:$NEXUS_PASS" -X POST "http://localhost:8081/service/rest/v1/repositories/maven2/hosted" \
  -H "Content-Type: application/json" \
  -d '{"name":"maven-snapshots","online":true,"storage":{"blobStoreName":"default","strictContentTypeValidation":true,"writePolicy":"ALLOW_ONCE"},"maven":{"versionPolicy":"SNAPSHOT","layoutPolicy":"STRICT"}}'

curl -u "admin:$NEXUS_PASS" -X POST "http://localhost:8081/service/rest/v1/repositories/raw/hosted" \
  -H "Content-Type: application/json" \
  -d '{"name":"raw-hosted","online":true,"storage":{"blobStoreName":"default","strictContentTypeValidation":false,"writePolicy":"ALLOW_ONCE"}}'

# 4. 下载一个小型 Maven 依赖
cargo run -- download --kind maven --name junit:junit --version 4.13.2

# 5. 创建配置文件
cat > config.toml <<EOF
[nexus]
base_url = "http://localhost:8081"
username = "admin"
password = "$NEXUS_PASS"

[repositories]
maven = "maven-releases"
maven_snapshots = "maven-snapshots"
npm = "npm-hosted"
pypi = "pypi-hosted"
cargo = "cargo-hosted"
raw = "raw-hosted"
EOF

# 6. 导入到 Nexus
cargo run -- import --kind maven --name junit:junit --version 4.13.2 --config config.toml

# 7. 验证 SNAPSHOT 路由：下载一个 SNAPSHOT 版本并导入
cargo run -- download --kind maven --name org.example:my-snapshot --version 1.0.0-SNAPSHOT
cargo run -- import --kind maven --name org.example:my-snapshot --version 1.0.0-SNAPSHOT --config config.toml
# → 自动发到 maven-snapshots 仓库

# 8. 验证：在浏览器打开 http://localhost:8081
#    - maven-releases 仓库有 junit:junit:4.13.2
#    - maven-snapshots 仓库有 org.example:my-snapshot:1.0.0-SNAPSHOT
# 9. 清理
docker rm -f nexus-test
rm -rf maven_junit_junit_4.13.2 maven_org.example_my-snapshot_1.0.0-SNAPSHOT config.toml
```

### GitHub Actions

项目配置了 GitHub Actions 自动化构建：

#### Docker 镜像发布

当 `Dockerfile.downloader` 或 `scripts/download.sh` 有变更时，自动构建并发布到：
- Docker Hub: `gudaoxuri/dep-downloader`
- GitHub Container Registry: `ghcr.io/200hub/dep-porter/dep-downloader`

支持手动触发（workflow_dispatch）。

#### 跨平台程序构建与发布

当 `Cargo.toml` 中的 version 对应的 release 不存在时，自动构建以下平台的可执行文件：
- Linux AMD64 / ARM64
- Windows AMD64
- macOS AMD64 / ARM64

自动创建 GitHub Release，包含：
- 所有平台的二进制文件
- 自动生成的发布说明（提交记录）

**发布流程：**
```bash
# 1. 更新 Cargo.toml 中的 version
# version = "1.1.0"

# 2. 推送到 main/master
git add Cargo.toml
git commit -m "bump version to 1.1.0"
git push

# 3. GitHub Actions 自动：
#    - 检查 v1.1.0 release 是否存在
#    - 如果不存在，构建所有平台二进制文件
#    - 创建 v1.1.0 标签
#    - 发布 Release
```

支持手动触发（workflow_dispatch）。
