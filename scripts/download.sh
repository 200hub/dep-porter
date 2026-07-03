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

mkdir -p "$OUT_DIR"

echo "=== dep-download ==="
echo "  kind    : $KIND"
echo "  name    : $NAME"
echo "  version : $VERSION"
echo "===================="

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
    cat > "$work_dir/pom.xml" <<EOF
<project>
  <modelVersion>4.0.0</modelVersion>
  <groupId>dep.porter</groupId>
  <artifactId>dep-porter-download</artifactId>
  <version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>$group_id</groupId>
      <artifactId>$artifact_id</artifactId>
      <version>$VERSION</version>
    </dependency>
  </dependencies>
</project>
EOF

    echo "Downloading Maven dependency $NAME:$VERSION with transitive deps..."
    cd "$work_dir"

    # dependency:get only downloads the artifact + POM + transitive deps.
    # dependency:go-offline pulls Maven plugins and their deps too (bloated).
    mvn dependency:get -B -q \
        -Dartifact="$group_id:$artifact_id:$VERSION" \
        -Dtransitive=true 2>&1 || true

    # Copy the local repository to output
    if [[ -d "$HOME/.m2/repository" ]]; then
        cp -r "$HOME/.m2/repository" "$OUT_DIR/repository"
        echo "Maven download complete. Repository saved to $OUT_DIR/repository"
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

    echo "Downloading npm dependency $NAME@$VERSION with transitive deps..."
    cd "$work_dir"
    npm install --ignore-scripts 2>&1

    # Copy results to output
    [[ -d node_modules ]] && cp -r node_modules "$OUT_DIR/"
    [[ -f package-lock.json ]] && cp package-lock.json "$OUT_DIR/"

    # Copy npm cache
    local npm_cache
    npm_cache=$(npm config get cache 2>/dev/null || echo "$HOME/.npm")
    if [[ -d "$npm_cache" ]]; then
        cp -r "$npm_cache" "$OUT_DIR/npm-cache" 2>/dev/null || true
    fi

    # Also pack the specific package tarball
    npm pack "$NAME@$VERSION" --pack-destination "$OUT_DIR" 2>/dev/null || true

    echo "npm download complete. Files saved to $OUT_DIR"
    rm -rf "$work_dir"
}

# ── PyPI ──────────────────────────────────────────────────────────────
download_pypi() {
    mkdir -p "$OUT_DIR/packages"

    echo "Downloading PyPI dependency $NAME==$VERSION with transitive deps..."
    pip3 download "$NAME==$VERSION" \
        -d "$OUT_DIR/packages" \
        --no-cache-dir 2>&1

    echo "PyPI download complete. Packages saved to $OUT_DIR/packages"
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

    echo "Downloading Cargo dependency $NAME==$VERSION with transitive deps..."
    cd "$work_dir"

    # Fetch all dependencies
    cargo fetch 2>&1

    # Vendor dependencies
    cargo vendor vendor 2>&1

    # Create .crate files from vendored source (required for Nexus cargo repo upload)
    mkdir -p "$OUT_DIR/crates"
    for crate_dir in vendor/*/; do
        local crate_name
        crate_name=$(basename "$crate_dir")
        # Read version from Cargo.toml inside the vendored crate
        local crate_version
        crate_version=$(grep -m1 '^version' "$crate_dir/Cargo.toml" 2>/dev/null | sed 's/.*"\(.*\)".*/\1/')
        if [[ -n "$crate_version" ]]; then
            # .crate is a tar.gz with top-level dir named {name}-{version}
            local tar_dir="${crate_name}-${crate_version}"
            local crate_file="$OUT_DIR/crates/${crate_name}-${crate_version}.crate"
            # Create a temp dir with the correct top-level name
            local tmp_tar
            tmp_tar=$(mktemp -d)
            cp -r "$crate_dir" "$tmp_tar/$tar_dir"
            # Remove .cargo-checksum.json (not part of the crate)
            rm -f "$tmp_tar/$tar_dir/.cargo-checksum.json"
            # Create .crate file (tar.gz)
            tar -czf "$crate_file" -C "$tmp_tar" "$tar_dir"
            rm -rf "$tmp_tar"
        fi
    done

    # Copy results to output
    [[ -d vendor ]] && cp -r vendor "$OUT_DIR/"
    [[ -f Cargo.lock ]] && cp Cargo.lock "$OUT_DIR/"

    echo "Cargo download complete. Vendor saved to $OUT_DIR/vendor, .crate files in $OUT_DIR/crates"
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

    echo "Downloading Conan dependency $NAME/$VERSION with transitive deps..."
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

    # Copy Conan cache / output to the output directory
    if [[ -d "$HOME/.conan2" ]]; then
        cp -r "$HOME/.conan2" "$OUT_DIR/conan-cache" 2>/dev/null || true
    fi

    # Also copy any generated files from the install
    cp -r . "$OUT_DIR/conan-workspace" 2>/dev/null || true

    echo "Conan download complete. Files saved to $OUT_DIR"
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
        echo "ERROR: Unsupported dependency kind: $KIND"
        echo "Supported: maven, npm, pypi, cargo, conan"
        exit 1
        ;;
esac
