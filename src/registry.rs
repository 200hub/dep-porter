//! 原生Nexus格式的注册表发布负载构建器。
//!
//! 本模块包含*纯*（非网络）逻辑，用于将下载的工件转换为Nexus原生npm和Cargo
//! 托管仓库期望的确切线路格式。将其与`import.rs`分离使得棘手的元数据转换
//! 可以在没有活动Nexus的情况下进行单元测试。
//!
//! ## Cargo
//! Nexus Cargo托管仓库只接受通过Cargo注册表API（`PUT /api/v1/crates/new`）的发布——
//! 直接路径上传的`.crate`文件*不会*被索引，对`cargo`客户端不可用。发布正文如下：
//!
//! ```text
//! [u32-LE json长度][json元数据][u32-LE crate长度][.crate字节]
//! ```
//!
//! JSON元数据（依赖项 + 特性）必须与crate完全匹配，因此我们从下载时捕获的
//! crates.io稀疏索引行中提取。
//!
//! ## npm
//! Nexus npm托管仓库接受标准的`npm publish`文档：
//! 一个JSON包描述符，其中tarball作为base64编码的`_attachment`嵌入。

use std::io::Read;

use anyhow::{Context, Result};
use base64::Engine;
use serde_json::{json, Map, Value};
use sha1::{Digest, Sha1};
use sha2::Sha512;

// ── Cargo ───────────────────────────────────────────────────────────────────

/// 计算crate名称的稀疏索引路径前缀，匹配crates.io和Nexus使用的布局
/// （例如`se/rd/serde`、`3/s/syn`、`1/a`）。
pub fn cargo_sparse_index_path(name: &str) -> String {
    let n = name.to_lowercase();
    match n.len() {
        0 => n,
        1 => format!("1/{}", n),
        2 => format!("2/{}", n),
        3 => format!("3/{}/{}", &n[0..1], n),
        _ => format!("{}/{}/{}", &n[0..2], &n[2..4], n),
    }
}

/// 将crates.io稀疏索引行转换为Cargo发布API（`api/v1/crates/new`）期望的JSON元数据。
///
/// 索引格式使用`req`表示版本要求，并可能将特性分散在`features`和`features2`中；
/// 发布格式使用`version_req`和单个合并的`features`映射。重命名的依赖项表示方式不同：
/// 索引将重命名存储在`name`中，将真实crate存储在`package`中，而发布期望真实crate在
/// `name`中，重命名在`explicit_name_in_toml`中。
pub fn cargo_index_to_publish_meta(index_line: &str) -> Result<Value> {
    let idx: Value = serde_json::from_str(index_line).context("解析crates.io索引行失败")?;

    let name = idx
        .get("name")
        .and_then(|v| v.as_str())
        .context("index entry missing 'name'")?;
    let vers = idx
        .get("vers")
        .and_then(|v| v.as_str())
        .context("index entry missing 'vers'")?;

    let mut deps = Vec::new();
    if let Some(arr) = idx.get("deps").and_then(|v| v.as_array()) {
        for d in arr {
            let idx_name = d.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            // 重命名的依赖项：crates.io索引将重命名存储在`name`中，将真实crate存储在`package`中。
            // Nexus将发布的`name`直接复制到它提供的索引中，因此我们必须将`name`保留为
            // 重命名（crate的特性/代码引用的），并在`explicit_name_in_toml`中携带真实crate名称。
            // 反向操作会产生一个索引，其特性表引用了不再存在的依赖项名称，
            // `cargo`会将其视为"无效"而拒绝。
            let explicit = d
                .get("package")
                .and_then(|p| p.as_str())
                .filter(|p| !p.is_empty());

            let mut dep = json!({
                "name": idx_name,
                "version_req": d.get("req").and_then(|v| v.as_str()).unwrap_or("*"),
                "features": d.get("features").cloned().unwrap_or_else(|| json!([])),
                "optional": d.get("optional").and_then(|v| v.as_bool()).unwrap_or(false),
                "default_features": d.get("default_features").and_then(|v| v.as_bool()).unwrap_or(true),
                "target": d.get("target").cloned().unwrap_or(Value::Null),
                "kind": d.get("kind").and_then(|v| v.as_str()).unwrap_or("normal"),
                "registry": d.get("registry").cloned().unwrap_or(Value::Null),
            });
            if let Some(e) = explicit {
                dep["explicit_name_in_toml"] = json!(e);
            }
            deps.push(dep);
        }
    }

    // 将features + features2合并为单个映射（发布格式）。
    let mut features = Map::new();
    if let Some(f) = idx.get("features").and_then(|v| v.as_object()) {
        for (k, v) in f {
            features.insert(k.clone(), v.clone());
        }
    }
    if let Some(f) = idx.get("features2").and_then(|v| v.as_object()) {
        for (k, v) in f {
            features.insert(k.clone(), v.clone());
        }
    }

    Ok(json!({
        "name": name,
        "vers": vers,
        "deps": deps,
        "features": features,
        "authors": [],
        "description": null,
        "documentation": null,
        "homepage": null,
        "readme": null,
        "readme_file": null,
        "keywords": [],
        "categories": [],
        "license": null,
        "license_file": null,
        "repository": null,
        "badges": {},
        "links": idx.get("links").cloned().unwrap_or(Value::Null),
    }))
}

/// 组装Cargo发布API的二进制正文：
/// `[u32-LE json长度][json][u32-LE crate长度][crate]`。
pub fn build_cargo_publish_body(meta: &Value, crate_bytes: &[u8]) -> Result<Vec<u8>> {
    let json_bytes = serde_json::to_vec(meta).context("serialize cargo publish metadata")?;
    let mut body = Vec::with_capacity(8 + json_bytes.len() + crate_bytes.len());
    body.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    body.extend_from_slice(&json_bytes);
    body.extend_from_slice(&(crate_bytes.len() as u32).to_le_bytes());
    body.extend_from_slice(crate_bytes);
    Ok(body)
}

// ── npm ───────────────────────────────────────────────────────────────────

/// 从npm tarball（gzip压缩的tar）中提取并解析`package/package.json`。
pub fn read_npm_package_json(tgz: &[u8]) -> Result<Value> {
    let gz = flate2::read::GzDecoder::new(tgz);
    let mut archive = tar::Archive::new(gz);
    let mut best: Option<Value> = None;
    for entry in archive.entries().context("read npm tarball entries")? {
        let mut entry = entry.context("read npm tarball entry")?;
        let path = entry.path().context("entry path")?.to_path_buf();
        let path_str = path.to_string_lossy().replace('\\', "/");
        if path_str.ends_with("/package.json") || path_str == "package.json" {
            let mut buf = String::new();
            entry
                .read_to_string(&mut buf)
                .context("read package.json")?;
            let value: Value = serde_json::from_str(&buf).context("parse package.json")?;
            // 优先选择规范的顶级`package/package.json`。
            if path_str == "package/package.json" {
                return Ok(value);
            }
            if best.is_none() {
                best = Some(value);
            }
        }
    }
    best.context("npm tarball does not contain a package.json")
}

/// 对npm包名称进行URL编码以用作路径段。作用域名称保留前导`@`，但将`/`编码为`%2f`（npm的约定）。
pub fn npm_encode_name(name: &str) -> String {
    name.replace('/', "%2f")
}

/// 包的无作用域tarball文件名（`@scope/pkg` -> `pkg`）。
pub fn npm_unscoped_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// 为单个tarball构建npm发布文档（packument）。
///
/// 返回`(name, version, document)`。tarball作为base64编码的`_attachment`嵌入，
/// 完整性/shasum在原始tarball字节上计算。
pub fn build_npm_publish_doc(
    package_json: &Value,
    tgz: &[u8],
    nexus_base: &str,
    repo: &str,
) -> Result<(String, String, Value)> {
    let name = package_json
        .get("name")
        .and_then(|v| v.as_str())
        .context("package.json missing 'name'")?
        .to_string();
    let version = package_json
        .get("version")
        .and_then(|v| v.as_str())
        .context("package.json missing 'version'")?
        .to_string();

    let shasum = {
        let mut h = Sha1::new();
        h.update(tgz);
        hex_encode(&h.finalize())
    };
    let integrity = {
        let mut h = Sha512::new();
        h.update(tgz);
        format!(
            "sha512-{}",
            base64::engine::general_purpose::STANDARD.encode(h.finalize())
        )
    };

    let unscoped = npm_unscoped_name(&name);
    let tarball_file = format!("{}-{}.tgz", unscoped, version);
    let base = nexus_base.trim_end_matches('/');
    let tarball_url = format!("{}/repository/{}/{}/-/{}", base, repo, name, tarball_file);

    // 版本元数据 = package.json 加上注册表分发字段。
    let mut version_meta = package_json
        .as_object()
        .cloned()
        .context("package.json is not an object")?;
    version_meta.insert("_id".into(), json!(format!("{}@{}", name, version)));
    version_meta.insert(
        "dist".into(),
        json!({
            "tarball": tarball_url,
            "shasum": shasum,
            "integrity": integrity,
        }),
    );

    let attachment_data = base64::engine::general_purpose::STANDARD.encode(tgz);
    let doc = json!({
        "_id": name,
        "name": name,
        "description": package_json.get("description").cloned().unwrap_or(Value::Null),
        "dist-tags": { "latest": version },
        "versions": { version.clone(): Value::Object(version_meta) },
        "_attachments": {
            tarball_file: {
                "content_type": "application/octet-stream",
                "data": attachment_data,
                "length": tgz.len(),
            }
        }
    });

    Ok((name, version, doc))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_index_path_lengths() {
        assert_eq!(cargo_sparse_index_path("a"), "1/a");
        assert_eq!(cargo_sparse_index_path("ab"), "2/ab");
        assert_eq!(cargo_sparse_index_path("abc"), "3/a/abc");
        assert_eq!(cargo_sparse_index_path("serde"), "se/rd/serde");
        assert_eq!(cargo_sparse_index_path("Serde"), "se/rd/serde");
        assert_eq!(cargo_sparse_index_path("aead"), "ae/ad/aead");
    }

    #[test]
    fn cargo_meta_basic() {
        let line = r#"{"name":"aead","vers":"0.5.2","deps":[{"name":"crypto-common","req":"^0.1.4","features":[],"optional":false,"default_features":true,"target":null,"kind":"normal"}],"cksum":"abc","features":{"std":["alloc"]},"yanked":false}"#;
        let meta = cargo_index_to_publish_meta(line).unwrap();
        assert_eq!(meta["name"], "aead");
        assert_eq!(meta["vers"], "0.5.2");
        let dep = &meta["deps"][0];
        assert_eq!(dep["name"], "crypto-common");
        assert_eq!(dep["version_req"], "^0.1.4");
        assert!(dep.get("req").is_none());
        assert_eq!(meta["features"]["std"][0], "alloc");
        assert!(meta["license"].is_null());
    }

    #[test]
    fn cargo_meta_renamed_dep() {
        // 重命名的依赖项：索引name=rand2 package=rand。Nexus复制
        // publish `name` into the served index, so `name` must stay the rename
        // and the real crate goes to `explicit_name_in_toml`.
        let line = r#"{"name":"x","vers":"1.0.0","deps":[{"name":"rand2","req":"^0.8","features":[],"optional":false,"default_features":true,"target":null,"kind":"normal","package":"rand"}],"cksum":"c","features":{}}"#;
        let meta = cargo_index_to_publish_meta(line).unwrap();
        let dep = &meta["deps"][0];
        assert_eq!(dep["name"], "rand2");
        assert_eq!(dep["explicit_name_in_toml"], "rand");
    }

    #[test]
    fn cargo_meta_merges_features2() {
        let line = r#"{"name":"x","vers":"1.0.0","deps":[],"cksum":"c","features":{"a":[]},"features2":{"b":["dep:serde"]}}"#;
        let meta = cargo_index_to_publish_meta(line).unwrap();
        assert!(meta["features"].get("a").is_some());
        assert_eq!(meta["features"]["b"][0], "dep:serde");
    }

    #[test]
    fn cargo_body_framing() {
        let meta = json!({"name":"x"});
        let crate_bytes = b"CRATEDATA";
        let body = build_cargo_publish_body(&meta, crate_bytes).unwrap();
        let json_len = u32::from_le_bytes(body[0..4].try_into().unwrap()) as usize;
        let json_part = &body[4..4 + json_len];
        assert_eq!(json_part, br#"{"name":"x"}"#);
        let crate_len =
            u32::from_le_bytes(body[4 + json_len..8 + json_len].try_into().unwrap()) as usize;
        assert_eq!(crate_len, crate_bytes.len());
        assert_eq!(&body[8 + json_len..], crate_bytes);
    }

    #[test]
    fn npm_name_encoding() {
        assert_eq!(npm_encode_name("lodash"), "lodash");
        assert_eq!(npm_encode_name("@scope/pkg"), "@scope%2fpkg");
        assert_eq!(npm_unscoped_name("@scope/pkg"), "pkg");
        assert_eq!(npm_unscoped_name("lodash"), "lodash");
    }

    #[test]
    fn npm_publish_doc_shape() {
        let pkg = json!({
            "name": "@scope/pkg",
            "version": "1.2.3",
            "description": "hi",
            "main": "index.js"
        });
        let tgz = b"fake-tarball-bytes";
        let (name, version, doc) =
            build_npm_publish_doc(&pkg, tgz, "http://nexus:8081/", "npm").unwrap();
        assert_eq!(name, "@scope/pkg");
        assert_eq!(version, "1.2.3");
        assert_eq!(doc["dist-tags"]["latest"], "1.2.3");
        let vmeta = &doc["versions"]["1.2.3"];
        assert_eq!(vmeta["_id"], "@scope/pkg@1.2.3");
        assert_eq!(
            vmeta["dist"]["tarball"],
            "http://nexus:8081/repository/npm/@scope/pkg/-/pkg-1.2.3.tgz"
        );
        assert!(vmeta["dist"]["integrity"]
            .as_str()
            .unwrap()
            .starts_with("sha512-"));
        let att = &doc["_attachments"]["pkg-1.2.3.tgz"];
        assert_eq!(att["length"], tgz.len());
    }
}
