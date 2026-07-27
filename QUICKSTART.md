# 快速开始指南

本指南帮助你快速上手 dep-porter，实现从外网下载依赖到内网 Nexus 的完整流程。

## 🎯 使用场景

你有一个内网 Nexus 仓库，需要从外网下载依赖并导入到内网。传统方式是手动下载每个依赖及其传递依赖，非常繁琐。

**dep-porter 的解决方案：**
1. 在外网机器上使用项目的依赖文件（pom.xml、package.json 等）批量下载
2. 将下载的依赖拷贝到内网
3. 在内网机器上批量导入到 Nexus

## ⚡ 5 分钟快速体验

### 准备工作

1. 下载 dep-porter：
   ```bash
   # 访问 https://github.com/200hub/dep-porter/releases
   # 下载对应平台的版本并解压
   ```

2. 确保外网机器安装了 Docker（用于下载）

### 外网机器操作

```bash
# 1. 进入你的项目目录
cd /path/to/your/project

# 2. 批量下载所有依赖（根据项目类型选择命令）

# Maven 项目
dep-porter batch-download --file pom.xml

# Node.js 项目（推荐使用 lock 文件）
dep-porter batch-download --file package-lock.json

# Python 项目
dep-porter batch-download --file requirements.txt

# Rust 项目（推荐使用 lock 文件）
dep-porter batch-download --file Cargo.lock

# C++ 项目
dep-porter batch-download --file conanfile.txt

# 3. 下载完成后，当前目录会生成多个 {类型}_{名称}_{版本} 目录
ls -la
# maven_org.springframework.boot_spring-boot-starter-web_2.7.0/
# maven_org.apache.commons_commons-lang3_3.14.0/
# ...
```

### 拷贝到内网

将以下内容拷贝到内网机器：
- dep-porter 二进制文件
- 所有下载的依赖目录
- 依赖文件（pom.xml、package.json 等）
- config.toml 配置文件（下一步创建）

### 内网机器操作

```bash
# 1. 创建 Nexus 配置文件
cat > config.toml <<EOF
[nexus]
base_url = "http://your-nexus.internal.com"
username = "admin"
password = "admin123"

[repositories]
maven = "maven-releases"
npm = "npm-hosted"
pypi = "pypi-hosted"
raw = "raw-hosted"
EOF

# 2. 批量导入到 Nexus（使用与下载时相同的依赖文件）

# Maven 项目
dep-porter batch-import --file pom.xml

# Node.js 项目
dep-porter batch-import --file package-lock.json

# Python 项目
dep-porter batch-import --file requirements.txt

# Rust 项目
dep-porter batch-import --file Cargo.lock

# C++ 项目
dep-porter batch-import --file conanfile.txt

# 3. 导入完成！
# 现在内网项目可以从 Nexus 拉取依赖了
```

## 📋 完整流程示例

### 示例：Maven 项目

**外网机器：**
```bash
# 1. 准备
cd ~/projects/my-spring-app
ls pom.xml  # 确认有 pom.xml

# 2. 批量下载（关闭检查以加速，可选）
dep-porter batch-download --file pom.xml \
  --no-check-security \
  --no-check-license

# 3. 打包要拷贝的文件
mkdir ~/transfer
cp dep-porter ~/transfer/
cp pom.xml ~/transfer/
cp -r maven_* ~/transfer/
cd ~/transfer && tar -czf transfer.tar.gz *

# 4. 拷贝 transfer.tar.gz 到内网
```

**内网机器：**
```bash
# 1. 解压
tar -xzf transfer.tar.gz

# 2. 创建配置
cat > config.toml <<'EOF'
[nexus]
base_url = "http://nexus.internal.com"
username = "admin"
password = "admin123"

[repositories]
maven = "maven-releases"
maven_snapshots = "maven-snapshots"
npm = "npm-hosted"
pypi = "pypi-hosted"
raw = "raw-hosted"
EOF

# 3. 批量导入
./dep-porter batch-import --file pom.xml

# 4. 验证
# 在浏览器打开 http://nexus.internal.com
# 查看 maven-releases 仓库是否有新导入的依赖
```

## 🎓 进阶使用

### 包含开发依赖

```bash
# 下载时包含 devDependencies、dev-dependencies 等
dep-porter batch-download --file package.json --include-dev

# 导入时也要加上 --include-dev
dep-porter batch-import --file package.json --include-dev
```

### 自定义输出目录

```bash
# 下载到指定目录
dep-porter batch-download --file pom.xml --output ./my-deps

# 导入时指定输入目录（必须与下载时的 --output 一致）
dep-porter batch-import --file pom.xml --input ./my-deps
```

### 使用缓存加速

```bash
# 首次下载
dep-porter batch-download --file pom.xml --cache-dir ~/.dep-cache

# 后续下载会复用缓存中的依赖
dep-porter batch-download --file pom2.xml --cache-dir ~/.dep-cache
```

### 覆盖已存在的依赖

```bash
# 导入时覆盖 Nexus 中已存在的制品
dep-porter batch-import --file pom.xml --overwrite
```

## 🔍 常见问题

### Q: 下载很慢怎么办？

**A:** 使用国内镜像源（默认已启用），或自定义镜像：

```bash
# 使用阿里云镜像（默认）
dep-porter batch-download --file pom.xml

# 自定义 Maven 镜像
MAVEN_MIRROR=https://maven.aliyun.com/repository/central \
dep-porter batch-download --file pom.xml

# 自定义 npm 镜像
NPM_MIRROR=https://registry.npmmirror.com \
dep-porter batch-download --file package.json
```

### Q: 下载失败怎么办？

**A:** 查看详细日志并重试失败的依赖：

```bash
# 1. 启用详细日志
RUST_LOG=debug dep-porter batch-download --file pom.xml

# 2. 如果只有少数依赖失败，可以手动下载单个依赖
dep-porter download --kind maven --name junit:junit --version 4.13.2

# 3. 使用缓存避免重复下载已成功的依赖
dep-porter batch-download --file pom.xml --cache-dir ./cache
```

### Q: 导入时提示目录不存在？

**A:** 确保导入时的参数与下载时一致：

```bash
# 错误示例：目录不匹配
dep-porter batch-download --file pom.xml --output ./downloads
dep-porter batch-import --file pom.xml  # ❌ 默认在当前目录查找

# 正确示例：目录匹配
dep-porter batch-download --file pom.xml --output ./downloads
dep-porter batch-import --file pom.xml --input ./downloads  # ✅
```

### Q: Maven 依赖版本包含 ${property} 怎么办？

**A:** dep-porter 支持 Maven 属性替换：

```xml
<properties>
  <spring.version>2.7.0</spring.version>
</properties>
<dependencies>
  <dependency>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-web</artifactId>
    <version>${spring.version}</version>  <!-- ✅ 会自动替换 -->
  </dependency>
</dependencies>
```

如果属性无法解析，会跳过该依赖并给出警告。

### Q: npm 的版本范围（^, ~）怎么处理？

**A:** 推荐使用 `package-lock.json` 而不是 `package.json`：

```bash
# 方式 1：使用 package-lock.json（推荐）
dep-porter batch-download --file package-lock.json

# 方式 2：dep-porter 会尝试清理版本前缀
# ^4.17.21 → 4.17.21
# ~4.18.0 → 4.18.0
dep-porter batch-download --file package.json
```

### Q: 如何验证导入成功？

**A:** 有三种方式验证：

1. **浏览器查看**：打开 Nexus web 界面，在对应仓库中搜索依赖名称

2. **命令行测试**（Maven 示例）：
   ```bash
   # 配置项目使用内网 Nexus
   # 编辑 ~/.m2/settings.xml 添加 mirror
   
   # 清理本地缓存
   rm -rf ~/.m2/repository/junit/junit/4.13.2
   
   # 重新下载（应该从 Nexus 拉取）
   mvn dependency:get -Dartifact=junit:junit:4.13.2
   ```

3. **API 验证**（Maven 示例）：
   ```bash
   curl -u admin:admin123 \
     "http://nexus.internal.com/repository/maven-releases/junit/junit/4.13.2/junit-4.13.2.jar" \
     --head
   ```

## 📚 下一步

- 阅读[完整 README](README.md) 了解所有功能
- 查看 [examples/](examples/) 目录的示例配置文件
- 参考[常见问题](README.md#常见问题)了解更多故障排查方法
- 访问 [GitHub Issues](https://github.com/200hub/dep-porter/issues) 报告问题或提建议

## 💡 小贴士

1. **优先使用锁定文件**：`package-lock.json`、`Cargo.lock` 比配置文件更可靠
2. **启用缓存**：使用 `--cache-dir` 可以大幅提升重复下载的速度
3. **关闭检查加速**：在可信环境使用 `--no-check-security --no-check-license`
4. **批量操作省时间**：使用 `batch-download`/`batch-import` 比逐个依赖快得多
5. **先小后大**：在小项目上先测试流程，再应用到大项目

Happy coding! 🚀
