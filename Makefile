# Auto Proxy Makefile
# 提供常用的开发和构建命令

.PHONY: help build run test clean release install dev check fmt clippy

# 默认目标
help:
	@echo "Auto Proxy - 可用的命令:"
	@echo ""
	@echo "开发命令:"
	@echo "  make dev      - 开发模式运行 (debug build)"
	@echo "  make run      - 运行程序"
	@echo "  make check    - 检查代码"
	@echo "  make test     - 运行测试"
	@echo "  make fmt      - 格式化代码"
	@echo "  make clippy   - 运行 clippy 检查"
	@echo ""
	@echo "构建命令:"
	@echo "  make build    - 构建 debug 版本"
	@echo "  make release  - 构建 release 版本"
	@echo "  make install  - 安装到系统"
	@echo "  make package  - 打包所有平台的 release 版本"
	@echo ""
	@echo "其他命令:"
	@echo "  make clean    - 清理构建文件"
	@echo "  make deps     - 安装依赖"

# 开发命令
dev:
	@echo "🚀 启动开发模式..."
	cargo run -- --port 8080

run:
	@echo "▶️  运行程序..."
	cargo run

check:
	@echo "🔍 检查代码..."
	cargo check

test:
	@echo "🧪 运行测试..."
	cargo test

fmt:
	@echo "🎨 格式化代码..."
	cargo fmt

clippy:
	@echo "📎 运行 clippy 检查..."
	cargo clippy -- -D warnings

# 构建命令
build:
	@echo "🔨 构建 debug 版本..."
	cargo build

release:
	@echo "🔨 构建 release 版本..."
	cargo build --release

install:
	@echo "📦 安装到系统..."
	cargo install --path .

package:
	@echo "📦 打包所有平台的 release 版本..."
	./build-release.sh

# 清理命令
clean:
	@echo "🧹 清理构建文件..."
	cargo clean
	rm -rf target/release-builds

# 依赖管理
deps:
	@echo "📋 检查并安装依赖..."
	@if ! command -v cargo >/dev/null 2>&1; then \
		echo "❌ Rust/Cargo 未安装，请先安装 Rust"; \
		exit 1; \
	fi
	@echo "✅ 依赖检查完成"

# 开发工具
watch:
	@echo "👀 监视文件变化并自动重新构建..."
	@if command -v cargo-watch >/dev/null 2>&1; then \
		cargo watch -x check -x test -x run; \
	else \
		echo "❌ cargo-watch 未安装，请运行: cargo install cargo-watch"; \
	fi

# 代码质量检查
quality: fmt clippy test
	@echo "✅ 代码质量检查完成"

# 完整的构建流程
all: clean quality build release
	@echo "✅ 完整构建流程完成"

# 快速开始
quick-start:
	@echo "🚀 快速开始 Auto Proxy..."
	@echo "1. 检查依赖..."
	@make deps
	@echo "2. 构建项目..."
	@make build
	@echo "3. 运行程序..."
	@echo "请运行: make dev"