# pom.xml 批量下载功能说明

## 功能概述

dep-porter 现在支持从 Maven 项目的 `pom.xml` 文件中读取所有依赖并批量下载。这个功能可以大大简化内网 Maven 项目的依赖搬运工作。

## 使用方法

### 基本用法

```bash
dep-porter download --kind maven --from-pom pom.xml
```

### 高级用法

```bash
# 自定义输出目录
dep-porter download --kind maven --from-pom pom.xml --output ./downloads

# 使用自定义缓存目录
dep-porter download --kind maven --from-pom pom.xml --cache-dir ./maven-cache

# 关闭安全漏洞检查
dep-porter download --kind maven --from-pom pom.xml --no-check-security

# 关闭许可证检查
dep-porter download --kind maven --from-pom pom.xml --no-check-license

# 关闭所有检查
dep-porter download --kind maven --from-pom pom.xml --no-check-security --no-check-license

# 不使用缓存
dep-porter download --kind maven --from-pom pom.xml --no-cache
```

## 功能特性

1. **自动解析依赖**：自动从 `pom.xml` 中提取所有 `<dependency>` 标签
2. **智能过滤**：自动过滤 `<scope>test</scope>` 的测试依赖
3. **批量下载**：依次下载每个依赖及其传递依赖
4. **安全检查**：对每个依赖执行安全漏洞检查（可选）
5. **许可证检查**：对每个依赖执行许可证商用风险检查（可选）
6. **避免重复**：如果依赖目录已存在，自动跳过
7. **错误容错**：单个依赖下载失败不影响其他依赖的下载
8. **进度显示**：显示当前处理进度（例如：[3/10]）
9. **结果汇总**：下载完成后显示成功和失败的统计信息

## pom.xml 示例

```xml
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  
  <groupId>com.example</groupId>
  <artifactId>my-app</artifactId>
  <version>1.0.0</version>
  
  <dependencies>
    <!-- 生产依赖：会被下载 -->
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter-web</artifactId>
      <version>2.7.0</version>
    </dependency>
    
    <!-- 生产依赖：会被下载 -->
    <dependency>
      <groupId>org.apache.commons</groupId>
      <artifactId>commons-lang3</artifactId>
      <version>3.14.0</version>
    </dependency>
    
    <!-- provided scope：会被下载 -->
    <dependency>
      <groupId>org.projectlombok</groupId>
      <artifactId>lombok</artifactId>
      <version>1.18.30</version>
      <scope>provided</scope>
    </dependency>
    
    <!-- test scope：会被自动过滤，不下载 -->
    <dependency>
      <groupId>junit</groupId>
      <artifactId>junit</artifactId>
      <version>4.13.2</version>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>
```

在这个例子中：
- `spring-boot-starter-web`、`commons-lang3` 和 `lombok` 会被下载
- `junit`（test scope）会被自动过滤，不会下载

## 输出示例

```
正在解析 pom.xml: pom.xml
找到 3 个依赖项（已过滤 test scope）

===== [1/3] 下载: org.springframework.boot:spring-boot-starter-web:2.7.0 =====
未发现已知漏洞。
正在下载 maven org.springframework.boot:spring-boot-starter-web:2.7.0 ...
输出: maven_org.springframework.boot_spring-boot-starter-web_2.7.0
✓ 下载成功: maven_org.springframework.boot_spring-boot-starter-web_2.7.0

===== [2/3] 下载: org.apache.commons:commons-lang3:3.14.0 =====
未发现已知漏洞。
正在下载 maven org.apache.commons:commons-lang3:3.14.0 ...
输出: maven_org.apache.commons_commons-lang3_3.14.0
✓ 下载成功: maven_org.apache.commons_commons-lang3_3.14.0

===== [3/3] 下载: org.projectlombok:lombok:1.18.30 =====
未发现已知漏洞。
正在下载 maven org.projectlombok:lombok:1.18.30 ...
输出: maven_org.projectlombok_lombok_1.18.30
✓ 下载成功: maven_org.projectlombok_lombok_1.18.30

========================================
批量下载完成:
  总计: 3
  成功: 3
  失败: 0
```

## 错误处理

如果某个依赖下载失败，程序会：
1. 记录错误信息
2. 继续处理下一个依赖
3. 在最后显示所有失败的依赖列表
4. 以非零退出码退出

示例输出：
```
========================================
批量下载完成:
  总计: 5
  成功: 3
  失败: 2

失败的依赖:
  - com.example:missing-artifact:1.0.0（Docker命令失败: ...）
  - com.example:another-fail:2.0.0（用户跳过）
```

## 注意事项

1. **只支持 Maven**：`--from-pom` 选项只能与 `--kind maven` 一起使用
2. **test scope 过滤**：scope 为 `test` 的依赖会被自动过滤
3. **其他 scope 保留**：`compile`、`provided`、`runtime` 等其他 scope 的依赖都会被下载
4. **版本必须明确**：pom.xml 中的每个依赖都必须指定明确的版本号，不支持版本范围或从父 POM 继承
5. **属性不解析**：不支持 Maven 属性（如 `${spring.version}`），版本号必须是具体值
6. **互斥选项**：不能同时使用 `--from-pom` 和 `--name`/`--version`

## 技术实现

### 新增文件

1. **src/pom.rs**：pom.xml 解析模块
   - `parse_pom_dependencies()`: 解析 pom.xml 文件
   - `MavenDependency`: 依赖信息结构体
   - 使用 `quick-xml` 库进行 XML 解析

### 修改文件

1. **Cargo.toml**：添加 `quick-xml = "0.36"` 依赖
2. **src/lib.rs**：添加 `pub mod pom;`
3. **src/cli.rs**：
   - `DownloadArgs.name` 和 `version` 改为 `Option<String>`
   - 添加 `from_pom: Option<String>` 参数
   - 添加参数验证逻辑
4. **src/main.rs**：
   - 添加 `cmd_download_from_pom()` 函数
   - 修改 `main()` 函数的参数验证逻辑

### 依赖库

- `quick-xml 0.36`：快速的 XML 解析库，用于解析 pom.xml

## 测试

项目包含了完整的单元测试：

```bash
# 运行所有测试
cargo test

# 运行 pom 解析测试
cargo test --lib pom

# 运行 CLI 测试
cargo test --lib cli
```

测试覆盖：
- 简单 pom.xml 解析
- 带 test scope 的依赖过滤
- 空依赖列表处理
- CLI 参数验证
- 互斥选项检查

## 示例文件

项目根目录包含 `pom.example.xml`，可以用来测试此功能：

```bash
dep-porter download --kind maven --from-pom pom.example.xml --no-check-security --no-check-license
```

## 常见问题

**Q: 可以解析多模块项目的 pom.xml 吗？**
A: 当前版本只解析指定的单个 pom.xml 文件，不会递归解析子模块。如果需要，需要对每个模块的 pom.xml 分别执行下载命令。

**Q: 支持从父 POM 继承的版本吗？**
A: 不支持。每个依赖必须在 pom.xml 中明确指定版本号。

**Q: 支持 Maven 属性（如 ${spring.version}）吗？**
A: 不支持。版本号必须是具体的字符串值。

**Q: 下载的依赖包含传递依赖吗？**
A: 是的。每个依赖都会通过 Maven 的 `dependency:get` 命令下载完整的传递依赖树。

**Q: 可以只下载生产依赖，不下载 provided 依赖吗？**
A: 当前版本会下载所有非 test scope 的依赖。如果需要更细粒度的控制，可以手动编辑 pom.xml 或使用单个依赖下载模式。

**Q: 性能如何？**
A: 批量下载是顺序执行的，不是并行的。如果启用了缓存（默认开启），重复的传递依赖只会下载一次，可以显著提升性能。

## 向后兼容性

此功能完全向后兼容，不影响原有的单个依赖下载功能：

```bash
# 原有功能依然正常工作
dep-porter download --kind maven --name junit:junit --version 4.13.2
dep-porter download --kind npm --name lodash --version 4.17.21
```
