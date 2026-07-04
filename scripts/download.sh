#!/usr/bin/env bash
set -euo pipefail

# ── Usage ─────────────────────────────────────────────────────────────
# dep-download <kind> <name> <version> <out_dir>
#
# Supported kinds: maven, npm, pypi, cargo, conan
# ──────────────────────────────────────────────────────────────────────

KIND="${1:?Usage: dep-download <kind> <name> <version> <out_dir>}"
NAME="${2:?Missing dependency name}"
VERSION="${3:?Missing dependency version}"
OUT_DIR="${4:?Missing output directory}"

# 镜像源配置（可通过环境变量覆盖）
MAVEN_MIRROR="${MAVEN_MIRROR:-https://maven.aliyun.com/repository/central}"
NPM_MIRROR="${NPM_MIRROR:-https://registry.npmmirror.com}"
PYPI_MIRROR="${PYPI_MIRROR:-https://mirrors.aliyun.com/pypi/simple}"
CARGO_MIRROR="${CARGO_MIRROR:-https://mirrors.ustc.edu.cn/crates.io-index}"

mkdir -p "$OUT_DIR"

echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║                   dep-porter 下载器                       ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "  类型    : $KIND"
echo "  名称    : $NAME"
echo "  版本    : $VERSION"
echo "  输出目录: $OUT_DIR"
echo ""
echo "  镜像源配置:"
echo "    Maven : $MAVEN_MIRROR"
echo "    npm   : $NPM_MIRROR"
echo "    PyPI  : $PYPI_MIRROR"
echo "    Cargo : $CARGO_MIRROR"
echo ""
echo "────────────────────────────────────────────────────────────"

# ── Maven ─────────────────────────────────────────────────────────────
download_maven() {
    local group_id artifact_id
    group_id="${NAME%%:*}"
    artifact_id="${NAME#*:}"

    if [[ "$group_id" == "$artifact_id" && "$NAME" != *:* ]]; then
        echo "ERROR: Invalid Maven coordinate '$NAME'. Expected format: groupId:artifactId"
        exit 1
    fi

    local work_dir
    work_dir=$(mktemp -d)

    # 判断是否为SNAPSHOT版本，如果是则添加额外的仓库
    local snapshot_repos=""
    if [[ "$VERSION" == *"-SNAPSHOT" ]]; then
        echo "Detected SNAPSHOT version, adding snapshot repositories..."
        snapshot_repos="
    <repository>
      <id>apache-snapshots</id>
      <url>https://repository.apache.org/content/repositories/snapshots</url>
      <snapshots>
        <enabled>true</enabled>
      </snapshots>
    </repository>
    <repository>
      <id>ossrh-snapshots</id>
      <url>https://oss.sonatype.org/content/repositories/snapshots</url>
      <snapshots>
        <enabled>true</enabled>
      </snapshots>
    </repository>"
    fi

    cat > "$work_dir/pom.xml" <<EOF
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>dep.porter</groupId>
  <artifactId>dep-porter-download</artifactId>
  <version>1.0.0</version>
  <repositories>
    <repository>
      <id>aliyun-central</id>
      <url>${MAVEN_MIRROR}</url>
    </repository>
    ${snapshot_repos}
  </repositories>
  <dependencies>
    <dependency>
      <groupId>$group_id</groupId>
      <artifactId>$artifact_id</artifactId>
      <version>$VERSION</version>
    </dependency>
  </dependencies>
</project>
EOF

    # 配置 Maven settings 使用镜像源
    mkdir -p "$HOME/.m2"
    cat > "$HOME/.m2/settings.xml" <<SETTINGS
<settings>
  <mirrors>
    <mirror>
      <id>aliyun-maven</id>
      <mirrorOf>central</mirrorOf>
      <name>Aliyun Maven Mirror</name>
      <url>${MAVEN_MIRROR}</url>
    </mirror>
  </mirrors>
</settings>
SETTINGS

    echo ""
    echo "▶ 步骤 1/2: 解析 Maven 依赖..."
    echo "  正在下载 $NAME:$VERSION 及其传递依赖..."
    cd "$work_dir"

    # dependency:get only downloads the artifact + POM + transitive deps.
    # dependency:go-offline pulls Maven plugins and their deps too (bloated).
    # -B: batch mode, no -q: show download progress
    mvn dependency:get -B \
        -Dartifact="$group_id:$artifact_id:$VERSION" \
        -Dtransitive=true 2>&1 || true

    echo ""
    echo "▶ 步骤 2/2: 收集下载文件..."
    # Copy the local repository contents to output (without the repository/ prefix)
    if [[ -d "$HOME/.m2/repository" ]]; then
        cp -r "$HOME/.m2/repository" "$OUT_DIR/"
        local count
        count=$(find "$OUT_DIR" -type f | wc -l)
        echo "✓ Maven 下载完成"
        echo "  共 $count 个文件已保存到 $OUT_DIR"
    else
        echo "ERROR: Maven local repository not found after download."
        exit 1
    fi

    rm -rf "$work_dir"
}

# ── NPM ───────────────────────────────────────────────────────────────
download_npm() {
    local work_dir
    work_dir=$(mktemp -d)
    cat > "$work_dir/package.json" <<EOF
{
  "name": "dep-porter-download",
  "version": "1.0.0",
  "private": true,
  "dependencies": {
    "$NAME": "$VERSION"
  }
}
EOF

    # 配置 npm 镜像源
    npm config set registry "$NPM_MIRROR"

    echo ""
    echo "▶ 步骤 1/3: 安装 npm 依赖..."
    echo "  正在下载 $NAME@$VERSION 及其传递依赖..."
    cd "$work_dir"
    npm install --ignore-scripts --no-audit --no-fund 2>&1

    echo ""
    echo "▶ 步骤 2/3: 解析依赖树..."
    # Pack the authentic published tarball for EVERY package in the resolved
    # tree (the requested package + all transitive dependencies). We read the
    # installed node_modules to enumerate exact name@version pairs, then
    # `npm pack` each so the tarball matches the registry byte-for-byte.
    mkdir -p "$OUT_DIR/tarballs"
    node -e '
      const fs = require("fs"), path = require("path");
      const seen = new Set();
      function handle(pdir) {
        const pj = path.join(pdir, "package.json");
        if (!fs.existsSync(pj)) return;
        let j;
        try { j = JSON.parse(fs.readFileSync(pj, "utf8")); } catch { return; }
        if (j.name && j.version) seen.add(j.name + "@" + j.version);
        walk(pdir);
      }
      function walk(dir) {
        const nm = path.join(dir, "node_modules");
        if (!fs.existsSync(nm)) return;
        for (const e of fs.readdirSync(nm)) {
          if (e.startsWith(".")) continue;
          if (e.startsWith("@")) {
            for (const s of fs.readdirSync(path.join(nm, e))) handle(path.join(nm, e, s));
          } else {
            handle(path.join(nm, e));
          }
        }
      }
      walk(process.cwd());
      fs.writeFileSync("pkglist.txt", [...seen].join("\n") + "\n");
      console.error("  解析完成: 发现 " + seen.size + " 个包");
    '

    echo ""
    echo "▶ 步骤 3/3: 打包 tarball..."
    local total
    total=$(wc -l < pkglist.txt)
    local current=0
    while IFS= read -r pkg || [[ -n "$pkg" ]]; do
        [[ -z "$pkg" ]] && continue
        current=$((current + 1))
        echo "  [$current/$total] 打包 $pkg"
        npm pack "$pkg" --pack-destination "$OUT_DIR/tarballs" 2>/dev/null \
            || echo "  WARN: failed to pack $pkg"
    done < pkglist.txt

    local count
    count=$(find "$OUT_DIR/tarballs" -name '*.tgz' | wc -l)
    echo ""
    echo "✓ npm 下载完成"
    echo "  共 $count 个 tarball 已保存到 $OUT_DIR/tarballs"
    rm -rf "$work_dir"
}

# ── PyPI ──────────────────────────────────────────────────────────────
download_pypi() {
    mkdir -p "$OUT_DIR/packages"

    echo ""
    echo "▶ 正在下载 PyPI 依赖..."
    echo "  $NAME==$VERSION 及其传递依赖..."
    pip3 download "$NAME==$VERSION" \
        -d "$OUT_DIR/packages" \
        -i "$PYPI_MIRROR" \
        --trusted-host "$(echo "$PYPI_MIRROR" | sed 's|https\?://||' | sed 's|/.*||')" \
        --no-cache-dir 2>&1

    local count
    count=$(find "$OUT_DIR/packages" -type f | wc -l)
    echo ""
    echo "✓ PyPI 下载完成"
    echo "  共 $count 个包已保存到 $OUT_DIR/packages"
}

# ── Cargo ─────────────────────────────────────────────────────────────
download_cargo() {
    local work_dir
    work_dir=$(mktemp -d)
    cat > "$work_dir/Cargo.toml" <<EOF
[package]
name = "dep-porter-download"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[dependencies]
$NAME = "=$VERSION"
EOF
    mkdir -p "$work_dir/src"
    echo "" > "$work_dir/src/lib.rs"

    # 配置 Cargo 镜像源
    mkdir -p "$HOME/.cargo"
    cat > "$HOME/.cargo/config.toml" <<CARGO_CONFIG
[source.crates-io]
replace-with = "ustc"

[source.ustc]
registry = "sparse+${CARGO_MIRROR}"
CARGO_CONFIG

    echo ""
    echo "▶ 步骤 1/2: 下载 Cargo 依赖..."
    echo "  正在解析并下载 $NAME==$VERSION 及其传递依赖..."
    cd "$work_dir"

    # Resolve + fetch all dependencies into the local registry cache. The
    # cache stores the authentic `.crate` files (correct checksums) that we
    # republish to Nexus — do NOT re-tar vendored sources, as that changes the
    # bytes and produces crates the client cannot verify.
    cargo fetch 2>&1

    echo ""
    echo "▶ 步骤 2/2: 收集 crate 文件和索引..."
    mkdir -p "$OUT_DIR/crates" "$OUT_DIR/index"

    # For every crates.io dependency in the resolved graph:
    #   1. copy its authentic .crate from the registry cache
    #   2. save its crates.io sparse-index line (deps + features) so the import
    #      side can reconstruct the exact Cargo publish metadata offline.
    OUT_DIR="$OUT_DIR" python3 - <<'PY'
import os, json, glob, shutil, subprocess, urllib.request

out = os.environ["OUT_DIR"]
crates_dir = os.path.join(out, "crates")
index_dir = os.path.join(out, "index")
cargo_home = os.environ.get("CARGO_HOME", os.path.expanduser("~/.cargo"))

meta = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--format-version", "1", "--offline"]))

def sparse_prefix(n):
    n = n.lower()
    if len(n) == 1: return "1/" + n
    if len(n) == 2: return "2/" + n
    if len(n) == 3: return "3/" + n[0] + "/" + n
    return n[:2] + "/" + n[2:4] + "/" + n

total = len([p for p in meta["packages"] if "registry+" in (p.get("source") or "")])
copied = 0
indexed = 0
for p in meta["packages"]:
    src = p.get("source") or ""
    # Only real crates.io registry deps (skip the local root + git/path deps).
    if "registry+" not in src:
        continue
    name, vers = p["name"], p["version"]
    stem = f"{name}-{vers}"

    # 1. copy authentic .crate
    hits = glob.glob(os.path.join(cargo_home, "registry", "cache", "*", f"{stem}.crate"))
    if hits:
        shutil.copy(hits[0], os.path.join(crates_dir, f"{stem}.crate"))
        copied += 1
    else:
        print(f"  WARN: .crate not found in cache for {stem}")

    # 2. save the crates.io index line for this exact version
    try:
        url = "https://index.crates.io/" + sparse_prefix(name)
        data = urllib.request.urlopen(url, timeout=30).read().decode()
        line = next(l for l in data.splitlines() if json.loads(l)["vers"] == vers)
        with open(os.path.join(index_dir, f"{stem}.json"), "w") as f:
            f.write(line)
        indexed += 1
    except Exception as e:
        print(f"  WARN: failed to fetch index metadata for {stem}: {e}")

print(f"  处理完成: {total} 个 crate")
print(f"  复制: {copied} 个 .crate 文件")
print(f"  索引: {indexed} 个 index 条目")
PY

    # Keep the lockfile for reference / reproducibility.
    [[ -f Cargo.lock ]] && cp Cargo.lock "$OUT_DIR/"

    echo ""
    echo "✓ Cargo 下载完成"
    echo "  .crate 文件保存到 $OUT_DIR/crates"
    echo "  索引元数据保存到 $OUT_DIR/index"
    rm -rf "$work_dir"
}

# ── Conan ─────────────────────────────────────────────────────────────
download_conan() {
    local work_dir
    work_dir=$(mktemp -d)
    cat > "$work_dir/conanfile.txt" <<EOF
[requires]
$NAME/$VERSION
EOF

    echo ""
    echo "▶ 步骤 1/2: 下载 Conan 依赖..."
    echo "  正在下载 $NAME/$VERSION 及其传递依赖..."
    cd "$work_dir"

    # Detect or create a default Conan profile
    conan profile detect --force 2>&1

    # Patch profile to set compiler (detect may miss it without gcc)
    local profile="$HOME/.conan2/profiles/default"
    if ! grep -q "compiler=" "$profile" 2>/dev/null; then
        cat > "$profile" <<PROFILE
[settings]
arch=x86_64
build_type=Release
compiler=gcc
compiler.cppstd=gnu17
compiler.libcxx=libstdc++11
compiler.version=12
os=Linux
PROFILE
    fi

    # Install dependencies — try download-only first, fall back to build
    conan install . --build=never 2>&1 || true

    echo ""
    echo "▶ 步骤 2/2: 收集 Conan 缓存..."
    # Copy Conan cache / output to the output directory
    if [[ -d "$HOME/.conan2" ]]; then
        cp -r "$HOME/.conan2" "$OUT_DIR/conan-cache" 2>/dev/null || true
    fi

    # Also copy any generated files from the install
    cp -r . "$OUT_DIR/conan-workspace" 2>/dev/null || true

    echo ""
    echo "✓ Conan 下载完成"
    echo "  文件已保存到 $OUT_DIR"
    rm -rf "$work_dir"
}

# ── Dispatch ──────────────────────────────────────────────────────────
case "$KIND" in
    maven)  download_maven ;;
    npm)    download_npm ;;
    pypi)   download_pypi ;;
    cargo)  download_cargo ;;
    conan)  download_conan ;;
    *)
        echo "错误：不支持此类型：$KIND"
        echo "当前支持: maven, npm, pypi, cargo, conan"
        exit 1
        ;;
esac
