#!/bin/bash
# dep-porter 的 pom.xml 包装脚本
# 使用方式: ./dep-porter-pom.sh --kind maven --from-pom pom.xml

set -euo pipefail

# 解析参数
FROM_POM=""
KIND=""
OUTPUT="."
CACHE_DIR=""
NO_CHECK_SECURITY=0
NO_CHECK_LICENSE=0
NO_CACHE=0

while [[ $# -gt 0 ]]; do
    case $1 in
        --kind)
            KIND="$2"
            shift 2
            ;;
        --from-pom)
            FROM_POM="$2"
            shift 2
            ;;
        --output)
            OUTPUT="$2"
            shift 2
            ;;
        --cache-dir)
            CACHE_DIR="$2"
            shift 2
            ;;
        --no-check-security)
            NO_CHECK_SECURITY=1
            shift
            ;;
        --no-check-license)
            NO_CHECK_LICENSE=1
            shift
            ;;
        --no-cache)
            NO_CACHE=1
            shift
            ;;
        *)
            echo "未知参数: $1"
            exit 1
            ;;
    esac
done

# 检查必需参数
if [[ -z "$FROM_POM" ]] || [[ -z "$KIND" ]]; then
    echo "使用方法: $0 --kind maven --from-pom <pom.xml> [其他选项]"
    exit 1
fi

if [[ "$KIND" != "maven" ]]; then
    echo "错误: --from-pom 仅适用于 --kind maven"
    exit 1
fi

if [[ ! -f "$FROM_POM" ]]; then
    echo "错误: pom.xml 文件不存在: $FROM_POM"
    exit 1
fi

echo "正在解析 pom.xml: $FROM_POM"

# 简单解析 pom.xml 中的依赖
# 注意：这是简化版本，仅处理简单格式的 pom.xml
dependencies=$(grep -A 3 "<dependency>" "$FROM_POM" | \
    grep -E "groupId|artifactId|version" | \
    sed 's/.*>\(.*\)<.*/\1/' | \
    paste - - - | \
    tr '\t' ':' | \
    sed 's/:/ /g')

if [[ -z "$dependencies" ]]; then
    echo "警告: 未找到依赖项"
    exit 0
fi

# 计算总数量
total=$(echo "$dependencies" | wc -l)
echo "找到 $total 个依赖项"

count=0
success=0
failed=0

# 创建输出目录
mkdir -p "$OUTPUT"

# 构建基础命令
BASE_CMD="./dep-porter download --kind maven"
if [[ "$NO_CHECK_SECURITY" -eq 1 ]]; then
    BASE_CMD="$BASE_CMD --no-check-security"
fi
if [[ "$NO_CHECK_LICENSE" -eq 1 ]]; then
    BASE_CMD="$BASE_CMD --no-check-license"
fi
if [[ -n "$CACHE_DIR" ]]; then
    BASE_CMD="$BASE_CMD --cache-dir $CACHE_DIR"
fi
if [[ "$NO_CACHE" -eq 1 ]]; then
    BASE_CMD="$BASE_CMD --no-cache"
fi

echo ""

# 处理每个依赖
while IFS=' ' read -r groupId artifactId version; do
    count=$((count + 1))
    
    # 构建坐标
    coord="$groupId:$artifactId"
    dir_name="maven_${groupId//./_}_${artifactId}_$version"
    output_dir="$OUTPUT/$dir_name"
    
    echo "===== [$count/$total] 下载: $coord:$version ====="
    
    # 检查目录是否已存在
    if [[ -d "$output_dir" ]]; then
        echo "目录已存在，跳过: $output_dir"
        success=$((success + 1))
        continue
    fi
    
    # 构建完整命令
    cmd="$BASE_CMD --name \"$coord\" --version \"$version\""
    
    echo "执行: $cmd"
    
    # 执行下载
    if eval "$cmd"; then
        echo "✓ 下载成功"
        success=$((success + 1))
    else
        echo "✗ 下载失败"
        failed=$((failed + 1))
    fi
    
    echo ""
done <<< "$dependencies"

echo "========================================"
echo "批量下载完成:"
echo "  总计: $total"
echo "  成功: $success"
echo "  失败: $failed"

if [[ $failed -gt 0 ]]; then
    exit 1
fi