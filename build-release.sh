#!/bin/bash

# Auto Proxy Release Build Script
# 支持 macOS 和 Linux 平台的交叉编译

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 项目信息
PROJECT_NAME="auto-proxy"
VERSION=$(grep '^version' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
BUILD_DIR="target/release-builds"

echo -e "${BLUE}🚀 Auto Proxy Release Builder${NC}"
echo -e "${BLUE}Version: ${VERSION}${NC}"
echo ""

# 检查是否安装了必要的工具
check_dependencies() {
    echo -e "${YELLOW}📋 检查依赖...${NC}"
    
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}❌ Cargo 未安装${NC}"
        exit 1
    fi
    
    if ! command -v rustc &> /dev/null; then
        echo -e "${RED}❌ Rust 编译器未安装${NC}"
        exit 1
    fi
    
    echo -e "${GREEN}✅ 依赖检查完成${NC}"
}

# 安装交叉编译目标
install_targets() {
    echo -e "${YELLOW}🎯 安装交叉编译目标...${NC}"
    
    # macOS targets
    rustup target add x86_64-apple-darwin
    rustup target add aarch64-apple-darwin
    
    # Linux targets
    rustup target add x86_64-unknown-linux-gnu
    rustup target add aarch64-unknown-linux-gnu
    
    echo -e "${GREEN}✅ 交叉编译目标安装完成${NC}"
}

# 构建函数
build_target() {
    local target=$1
    local platform=$2
    local arch=$3
    
    echo -e "${BLUE}🔨 构建 ${platform}-${arch}...${NC}"
    
    # 设置环境变量
    export CARGO_TARGET_DIR="target"
    
    # 构建
    if cargo build --release --target "$target"; then
        echo -e "${GREEN}✅ ${platform}-${arch} 构建成功${NC}"
        
        # 创建输出目录
        mkdir -p "${BUILD_DIR}/${platform}-${arch}"
        
        # 复制二进制文件
        local binary_name="${PROJECT_NAME}"
        if [[ "$target" == *"windows"* ]]; then
            binary_name="${PROJECT_NAME}.exe"
        fi
        
        cp "target/${target}/release/${binary_name}" "${BUILD_DIR}/${platform}-${arch}/"
        
        # 创建压缩包
        cd "${BUILD_DIR}/${platform}-${arch}"
        if [[ "$OSTYPE" == "darwin"* ]]; then
            # macOS 使用 tar
            tar -czf "../${PROJECT_NAME}-${VERSION}-${platform}-${arch}.tar.gz" "${binary_name}"
        else
            # Linux 使用 tar
            tar -czf "../${PROJECT_NAME}-${VERSION}-${platform}-${arch}.tar.gz" "${binary_name}"
        fi
        cd - > /dev/null
        
        echo -e "${GREEN}📦 ${platform}-${arch} 打包完成${NC}"
    else
        echo -e "${RED}❌ ${platform}-${arch} 构建失败${NC}"
        return 1
    fi
}

# 主构建流程
main() {
    echo -e "${YELLOW}🧹 清理旧的构建文件...${NC}"
    rm -rf "$BUILD_DIR"
    mkdir -p "$BUILD_DIR"
    
    check_dependencies
    install_targets
    
    echo -e "${YELLOW}🏗️  开始构建所有目标平台...${NC}"
    echo ""
    
    # 检测当前操作系统
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        echo -e "${BLUE}🍎 在 macOS 上构建${NC}"
        
        # macOS targets
        build_target "x86_64-apple-darwin" "macos" "x86_64"
        build_target "aarch64-apple-darwin" "macos" "aarch64"
        
        # Linux targets (需要额外配置)
        if command -v x86_64-linux-gnu-gcc &> /dev/null; then
            export CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc
            build_target "x86_64-unknown-linux-gnu" "linux" "x86_64"
        else
            echo -e "${YELLOW}⚠️  跳过 Linux x86_64 构建 (缺少交叉编译工具链)${NC}"
        fi
        
        if command -v aarch64-linux-gnu-gcc &> /dev/null; then
            export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
            build_target "aarch64-unknown-linux-gnu" "linux" "aarch64"
        else
            echo -e "${YELLOW}⚠️  跳过 Linux aarch64 构建 (缺少交叉编译工具链)${NC}"
        fi
        
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        # Linux
        echo -e "${BLUE}🐧 在 Linux 上构建${NC}"
        
        # Linux targets
        build_target "x86_64-unknown-linux-gnu" "linux" "x86_64"
        
        # 检查是否支持 aarch64
        if rustup target list --installed | grep -q "aarch64-unknown-linux-gnu"; then
            build_target "aarch64-unknown-linux-gnu" "linux" "aarch64"
        fi
        
        # macOS targets (通常不支持)
        echo -e "${YELLOW}⚠️  跳过 macOS 构建 (需要在 macOS 系统上构建)${NC}"
    else
        echo -e "${RED}❌ 不支持的操作系统: $OSTYPE${NC}"
        exit 1
    fi
    
    echo ""
    echo -e "${GREEN}🎉 构建完成！${NC}"
    echo -e "${BLUE}📁 构建文件位置: ${BUILD_DIR}${NC}"
    
    # 显示构建结果
    if [ -d "$BUILD_DIR" ]; then
        echo -e "${YELLOW}📦 生成的文件:${NC}"
        ls -la "$BUILD_DIR"/*.tar.gz 2>/dev/null || echo "没有找到压缩包文件"
    fi
}

# 显示帮助信息
show_help() {
    echo "Auto Proxy Release Builder"
    echo ""
    echo "用法: $0 [选项]"
    echo ""
    echo "选项:"
    echo "  -h, --help     显示此帮助信息"
    echo "  -c, --clean    仅清理构建文件"
    echo "  -t, --targets  仅安装交叉编译目标"
    echo ""
    echo "示例:"
    echo "  $0              # 构建所有支持的平台"
    echo "  $0 --clean     # 清理构建文件"
    echo "  $0 --targets   # 安装交叉编译目标"
}

# 解析命令行参数
case "${1:-}" in
    -h|--help)
        show_help
        exit 0
        ;;
    -c|--clean)
        echo -e "${YELLOW}🧹 清理构建文件...${NC}"
        rm -rf "$BUILD_DIR" target/
        echo -e "${GREEN}✅ 清理完成${NC}"
        exit 0
        ;;
    -t|--targets)
        install_targets
        exit 0
        ;;
    "")
        main
        ;;
    *)
        echo -e "${RED}❌ 未知选项: $1${NC}"
        show_help
        exit 1
        ;;
esac