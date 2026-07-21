# 变更日志 - pom.xml 批量下载支持

## 版本 1.5.0（计划中）

### 新增功能

#### 🎉 支持从 pom.xml 批量下载 Maven 依赖

- 添加 `--from-pom` 选项，允许从 Maven 项目的 pom.xml 文件中读取所有依赖并批量下载
- 自动过滤 `<scope>test</scope>` 的测试依赖
- 支持所有其他 scope（compile, provided, runtime）的依赖
- 批量下载时保留所有安全检查和许可证检查功能
- 智能跳过已存在的依赖目录，避免重复下载
- 错误容错机制：单个依赖失败不影响其他依赖的下载
- 提供详细的进度显示和结果汇总

### 技术变更

#### 新增模块

- **src/pom.rs**：pom.xml 解析模块
  - `parse_pom_dependencies()`: 使用 quick-xml 解析 pom.xml 文件
  - `MavenDependency`: 表示 Maven 依赖的数据结构
  - 完整的单元测试覆盖

#### 依赖更新

- 添加 `quick-xml = "0.36"` 用于 XML 解析

#### API 变更

- **src/cli.rs**：
  - `DownloadArgs.name`: `String` → `Option<String>`
  - `DownloadArgs.version`: `String` → `Option<String>`
  - 新增 `DownloadArgs.from_pom: Option<String>`
  - 参数验证逻辑更新：`--name` 和 `--version` 在使用 `--from-pom` 时变为可选

- **src/main.rs**：
  - 新增 `cmd_download_from_pom()` 函数处理批量下载
  - 更新 `main()` 函数的参数验证和路由逻辑
  - 原有的 `cmd_download()` 函数保持不变，确保向后兼容

### 文档更新

- **README.md**：
  - 添加从 pom.xml 批量下载的使用说明
  - 添加功能特性列表
  - 添加 pom.xml 示例
  
- **新增文档**：
  - `POM_SUPPORT.md`：pom.xml 功能的完整使用指南
  - `pom.example.xml`：示例 pom.xml 文件
  - `CHANGELOG_POM_SUPPORT.md`：本变更日志

### 测试

- 为 pom.rs 添加完整的单元测试：
  - 简单 pom.xml 解析
  - test scope 依赖过滤
  - 空依赖列表处理
  - Maven 坐标转换
  
- 更新 cli.rs 的测试：
  - 验证 `--from-pom` 参数解析
  - 验证参数互斥逻辑
  - 验证必需参数检查

### 使用示例

```bash
# 基本用法
dep-porter download --kind maven --from-pom pom.xml

# 自定义输出和缓存目录
dep-porter download --kind maven --from-pom pom.xml \
  --output ./downloads \
  --cache-dir ./maven-cache

# 关闭所有检查
dep-porter download --kind maven --from-pom pom.xml \
  --no-check-security \
  --no-check-license
```

### 向后兼容性

✅ 完全向后兼容，原有的单个依赖下载功能不受影响：

```bash
# 原有功能继续正常工作
dep-porter download --kind maven --name junit:junit --version 4.13.2
dep-porter download --kind npm --name lodash --version 4.17.21
```

### 限制和注意事项

1. **只支持 Maven**：`--from-pom` 仅适用于 `--kind maven`
2. **不支持属性解析**：版本号必须是具体值，不支持 `${property}` 形式
3. **不支持父 POM 继承**：版本号必须在当前 pom.xml 中明确指定
4. **不支持多模块递归**：只解析指定的单个 pom.xml 文件
5. **顺序执行**：批量下载是顺序执行的，不是并行的

### 未来改进方向

- [ ] 支持 Maven 属性解析（如 `${spring.version}`）
- [ ] 支持从父 POM 继承版本
- [ ] 支持多模块项目的递归解析
- [ ] 并行下载以提升性能
- [ ] 支持从其他项目文件格式读取依赖（如 Maven settings.xml）
- [ ] 添加 `--dry-run` 选项，只显示要下载的依赖列表

### 开发者说明

#### 如何编译

```bash
cargo build --release
```

#### 如何测试

```bash
# 运行所有测试
cargo test

# 运行特定模块的测试
cargo test --lib pom
cargo test --lib cli

# 测试示例文件
./target/release/dep-porter download --kind maven --from-pom pom.example.xml
```

#### 代码审查要点

1. XML 解析使用了 `quick-xml`，这是一个高性能的 Rust XML 解析库
2. 采用状态机方式解析 XML，避免了 DOM 树的内存开销
3. 错误处理完善，包含详细的上下文信息
4. 测试覆盖了主要的边界情况
5. CLI 参数使用 `clap` 的 derive API，保持了类型安全

#### 相关 Issue

- [待创建] 支持从 pom.xml 批量下载 Maven 依赖

### 贡献者

- [您的名字/GitHub 用户名]

---

## 迁移指南

如果您正在使用旧版本的 dep-porter，升级到 1.5.0 不需要任何代码修改。新功能是完全可选的。

### 从手动下载迁移到批量下载

**之前：**
```bash
dep-porter download --kind maven --name org.springframework.boot:spring-boot-starter-web --version 2.7.0
dep-porter download --kind maven --name org.apache.commons:commons-lang3 --version 3.14.0
dep-porter download --kind maven --name com.google.code.gson:gson --version 2.10.1
```

**现在：**

创建 pom.xml：
```xml
<project>
  <dependencies>
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter-web</artifactId>
      <version>2.7.0</version>
    </dependency>
    <dependency>
      <groupId>org.apache.commons</groupId>
      <artifactId>commons-lang3</artifactId>
      <version>3.14.0</version>
    </dependency>
    <dependency>
      <groupId>com.google.code.gson</groupId>
      <artifactId>gson</artifactId>
      <version>2.10.1</version>
    </dependency>
  </dependencies>
</project>
```

然后执行：
```bash
dep-porter download --kind maven --from-pom pom.xml
```

这样可以：
- 减少命令行输入
- 避免手动管理依赖列表
- 直接使用项目的 pom.xml，保持依赖版本同步
