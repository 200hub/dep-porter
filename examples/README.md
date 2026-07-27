# dep-porter 示例配置文件

本目录包含各种依赖配置文件的示例，用于演示 `dep-porter` 的批量下载和导入功能。

## 📁 示例文件列表

| 文件 | 生态系统 | 说明 |
|------|---------|------|
| `pom.xml` | Maven | Java 项目依赖，包含属性替换和 scope 过滤示例 |
| `package.json` | npm | Node.js 项目依赖，包含 dependencies 和 devDependencies |
| `requirements.txt` | PyPI | Python 项目依赖，仅包含固定版本 |
| `Cargo.toml` | Cargo | Rust 项目依赖，包含 features 和 dev-dependencies |
| `conanfile.txt` | Conan | C++ 项目依赖，包含 generators 和 options |

## 🚀 使用方法

### 1. 批量下载

在有网络的机器上执行：

```bash
# Maven
dep-porter batch-download --file examples/pom.xml

# npm
dep-porter batch-download --file examples/package.json

# PyPI
dep-porter batch-download --file examples/requirements.txt

# Cargo
dep-porter batch-download --file examples/Cargo.toml

# Conan
dep-porter batch-download --file examples/conanfile.txt

# 包含开发依赖
dep-porter batch-download --file examples/package.json --include-dev

# 自定义输出目录
dep-porter batch-download --file examples/pom.xml --output ./my-downloads

# 关闭检查加速下载
dep-porter batch-download --file examples/pom.xml --no-check-security --no-check-license
```

### 2. 批量导入

在内网机器上执行（需要先创建 `config.toml`）：

```bash
# Maven
dep-porter batch-import --file examples/pom.xml

# npm
dep-porter batch-import --file examples/package.json

# PyPI
dep-porter batch-import --file examples/requirements.txt

# Cargo
dep-porter batch-import --file examples/Cargo.toml

# Conan
dep-porter batch-import --file examples/conanfile.txt

# 自定义输入目录（需与下载时的 --output 一致）
dep-porter batch-import --file examples/pom.xml --input ./my-downloads

# 覆盖模式
dep-porter batch-import --file examples/pom.xml --overwrite
```

## 📝 各文件说明

### pom.xml (Maven)

**特性：**
- ✅ 支持 `${properties}` 属性替换
- ✅ 自动过滤 `test` scope 依赖
- ✅ 支持 `provided`、`runtime` 等其他 scope

**解析结果：**
- ✓ spring-boot-starter-web (compile)
- ✓ commons-lang3 (compile)
- ✓ gson (compile)
- ✓ lombok (provided)
- ✗ junit (test - 被过滤)
- ✗ mockito-core (test - 被过滤)

### package.json (npm)

**特性：**
- ✅ 解析 `dependencies`
- ✅ 可选解析 `devDependencies`（需要 `--include-dev`）
- ✅ 自动清理版本前缀（^, ~, >=）

**解析结果（默认）：**
- ✓ express
- ✓ lodash
- ✓ axios
- ✓ dotenv
- ✗ jest (devDependencies - 需要 --include-dev)
- ✗ eslint (devDependencies - 需要 --include-dev)
- ✗ nodemon (devDependencies - 需要 --include-dev)

### requirements.txt (PyPI)

**特性：**
- ✅ 支持注释和空行
- ✅ 仅支持 `name==version` 固定版本格式
- ⚠️ 不支持版本范围（>=, ~=, !=）
- ⚠️ 不支持无版本号

**解析结果：**
- ✓ flask==3.0.0
- ✓ django==5.0.1
- ✓ requests==2.32.3
- ✓ urllib3==2.1.0
- ✓ pandas==2.1.4
- ✓ numpy==1.26.2
- ✓ python-dotenv==1.0.0

### Cargo.toml (Rust)

**特性：**
- ✅ 解析 `[dependencies]`
- ✅ 支持完整格式（features、version）
- ✅ 可选解析 `[dev-dependencies]` 和 `[build-dependencies]`（需要 `--include-dev`）

**解析结果（默认）：**
- ✓ tokio
- ✓ axum
- ✓ serde
- ✓ serde_json
- ✓ anyhow
- ✓ thiserror
- ✓ log
- ✓ env_logger
- ✓ config
- ✓ dotenv
- ✗ tokio-test (dev-dependencies - 需要 --include-dev)
- ✗ criterion (dev-dependencies - 需要 --include-dev)
- ✗ cc (build-dependencies - 需要 --include-dev)

### conanfile.txt (Conan)

**特性：**
- ✅ 解析 `[requires]` 段
- ✅ 支持 `name/version` 格式
- ✅ 支持 `name/version@user/channel` 格式

**解析结果：**
- ✓ zlib/1.2.13
- ✓ boost/1.80.0
- ✓ openssl/3.2.0
- ✓ nlohmann_json/3.11.3

## 🎯 推荐使用锁定文件

对于支持锁定文件的生态系统，推荐使用锁定文件而不是配置文件：

| 配置文件 | 锁定文件 | 优势 |
|---------|---------|------|
| `package.json` | `package-lock.json` ⭐ | 精确版本、完整依赖树 |
| `Cargo.toml` | `Cargo.lock` ⭐ | 精确版本、完整依赖树 |
| `pom.xml` | - | Maven 没有标准锁定文件 |
| `requirements.txt` | - | 已经是固定版本 |
| `conanfile.txt` | - | Conan 没有标准锁定文件 |

**使用锁定文件的好处：**
- 🔒 精确版本，可重现构建
- 📦 包含完整依赖树（不仅仅是直接依赖）
- ✅ 避免版本范围解析问题
- 🚀 下载速度更快（不需要解析版本）

```bash
# 推荐：使用锁定文件
dep-porter batch-download --file package-lock.json
dep-porter batch-download --file Cargo.lock

# 次选：使用配置文件
dep-porter batch-download --file package.json
dep-porter batch-download --file Cargo.toml
```

## 💡 最佳实践

1. **使用锁定文件**：`package-lock.json` 和 `Cargo.lock` 比配置文件更可靠
2. **关闭检查加速**：在内网环境下载时使用 `--no-check-security --no-check-license`
3. **启用缓存**：使用 `--cache-dir` 复用已下载的依赖
4. **保持一致**：导入时的 `--input` 和 `--include-dev` 必须与下载时一致
5. **先测试**：在小项目上先测试流程，确保配置正确

## 🔧 故障排查

### 问题：依赖解析失败

**可能原因：**
- 版本号格式不正确（如使用了版本范围）
- 属性无法解析（Maven `${unknown.property}`）
- 文件格式错误

**解决方案：**
```bash
# 1. 使用锁定文件代替配置文件
dep-porter batch-download --file Cargo.lock  # 而不是 Cargo.toml

# 2. 查看详细日志
RUST_LOG=debug dep-porter batch-download --file pom.xml

# 3. 手动检查依赖文件格式
# - Maven: 确保所有依赖都有明确的 <version>
# - PyPI: 确保使用 name==version 格式
# - npm: 尝试使用 package-lock.json
```

### 问题：批量导入找不到目录

**可能原因：**
- 下载和导入使用了不同的输出/输入目录
- 下载和导入的 `--include-dev` 设置不一致

**解决方案：**
```bash
# 确保目录一致
dep-porter batch-download --file pom.xml --output ./downloads
dep-porter batch-import --file pom.xml --input ./downloads

# 确保 include-dev 一致
dep-porter batch-download --file package.json --include-dev
dep-porter batch-import --file package.json --include-dev
```

## 📚 相关文档

- [主 README](../README.md) - 完整使用指南
- [配置文件示例](../config.example.toml) - Nexus 配置示例
- [GitHub Releases](https://github.com/200hub/dep-porter/releases) - 下载最新版本
