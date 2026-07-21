use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

/// Maven 依赖信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenDependency {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    pub scope: Option<String>,
}

impl MavenDependency {
    /// 转换为 Maven 坐标格式（groupId:artifactId）
    pub fn to_coordinate(&self) -> String {
        format!("{}:{}", self.group_id, self.artifact_id)
    }
}

/// 从 pom.xml 文件解析所有依赖
pub fn parse_pom_dependencies(pom_path: &Path) -> Result<Vec<MavenDependency>> {
    let file = File::open(pom_path)
        .with_context(|| format!("无法打开 pom.xml 文件: {}", pom_path.display()))?;
    let reader = BufReader::new(file);
    let mut xml_reader = Reader::from_reader(reader);
    xml_reader.trim_text(true);

    let mut dependencies = Vec::new();
    let mut buf = Vec::new();

    // 状态机变量
    let mut in_dependencies = false;
    let mut in_dependency = false;
    let mut in_group_id = false;
    let mut in_artifact_id = false;
    let mut in_version = false;
    let mut in_scope = false;

    // 当前依赖的临时数据
    let mut current_group_id = String::new();
    let mut current_artifact_id = String::new();
    let mut current_version = String::new();
    let mut current_scope: Option<String> = None;

    loop {
        match xml_reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"dependencies" => in_dependencies = true,
                    b"dependency" if in_dependencies => {
                        in_dependency = true;
                        // 重置当前依赖数据
                        current_group_id.clear();
                        current_artifact_id.clear();
                        current_version.clear();
                        current_scope = None;
                    }
                    b"groupId" if in_dependency => in_group_id = true,
                    b"artifactId" if in_dependency => in_artifact_id = true,
                    b"version" if in_dependency => in_version = true,
                    b"scope" if in_dependency => in_scope = true,
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default().to_string();
                if in_group_id {
                    current_group_id = text.trim().to_string();
                } else if in_artifact_id {
                    current_artifact_id = text.trim().to_string();
                } else if in_version {
                    current_version = text.trim().to_string();
                } else if in_scope {
                    current_scope = Some(text.trim().to_string());
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                match name.as_ref() {
                    b"dependencies" => in_dependencies = false,
                    b"dependency" => {
                        // 依赖标签结束，保存当前依赖
                        if in_dependency
                            && !current_group_id.is_empty()
                            && !current_artifact_id.is_empty()
                            && !current_version.is_empty()
                        {
                            // 过滤掉测试依赖
                            let is_test_scope = current_scope
                                .as_ref()
                                .map(|s| s.to_lowercase() == "test")
                                .unwrap_or(false);

                            if !is_test_scope {
                                dependencies.push(MavenDependency {
                                    group_id: current_group_id.clone(),
                                    artifact_id: current_artifact_id.clone(),
                                    version: current_version.clone(),
                                    scope: current_scope.clone(),
                                });
                            }
                        }
                        in_dependency = false;
                    }
                    b"groupId" => in_group_id = false,
                    b"artifactId" => in_artifact_id = false,
                    b"version" => in_version = false,
                    b"scope" => in_scope = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "解析 pom.xml 时出错 (位置 {}): {}",
                    xml_reader.buffer_position(),
                    e
                ))
            }
            _ => {}
        }
        buf.clear();
    }

    if dependencies.is_empty() {
        log::warn!("pom.xml 中未找到任何依赖（test scope 的依赖已被过滤）");
    }

    Ok(dependencies)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_simple_pom() {
        let pom_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>my-app</artifactId>
  <version>1.0.0</version>
  
  <dependencies>
    <dependency>
      <groupId>junit</groupId>
      <artifactId>junit</artifactId>
      <version>4.13.2</version>
    </dependency>
    <dependency>
      <groupId>org.apache.commons</groupId>
      <artifactId>commons-lang3</artifactId>
      <version>3.14.0</version>
    </dependency>
  </dependencies>
</project>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(pom_content.as_bytes()).unwrap();

        let deps = parse_pom_dependencies(temp_file.path()).unwrap();

        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].group_id, "junit");
        assert_eq!(deps[0].artifact_id, "junit");
        assert_eq!(deps[0].version, "4.13.2");
        assert_eq!(deps[1].group_id, "org.apache.commons");
        assert_eq!(deps[1].artifact_id, "commons-lang3");
        assert_eq!(deps[1].version, "3.14.0");
    }

    #[test]
    fn test_parse_pom_with_test_scope() {
        let pom_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <dependencies>
    <dependency>
      <groupId>org.springframework</groupId>
      <artifactId>spring-core</artifactId>
      <version>5.3.0</version>
    </dependency>
    <dependency>
      <groupId>junit</groupId>
      <artifactId>junit</artifactId>
      <version>4.13.2</version>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(pom_content.as_bytes()).unwrap();

        let deps = parse_pom_dependencies(temp_file.path()).unwrap();

        // 只应该有 spring-core，test scope 的 junit 应该被过滤掉
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].group_id, "org.springframework");
        assert_eq!(deps[0].artifact_id, "spring-core");
    }

    #[test]
    fn test_parse_empty_dependencies() {
        let pom_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <dependencies>
  </dependencies>
</project>"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(pom_content.as_bytes()).unwrap();

        let deps = parse_pom_dependencies(temp_file.path()).unwrap();
        assert_eq!(deps.len(), 0);
    }

    #[test]
    fn test_to_coordinate() {
        let dep = MavenDependency {
            group_id: "org.apache.commons".to_string(),
            artifact_id: "commons-lang3".to_string(),
            version: "3.14.0".to_string(),
            scope: None,
        };
        assert_eq!(dep.to_coordinate(), "org.apache.commons:commons-lang3");
    }
}
