# 🚀 Auto Proxy

一个支持多提供商的智能代理服务器，具有自动重试和故障转移功能。

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Build Status](https://github.com/mintya/auto-proxy/workflows/Release%20Build/badge.svg)](https://github.com/mintya/auto-proxy/actions)
[![Release](https://img.shields.io/github/v/release/mintya/auto-proxy)](https://github.com/mintya/auto-proxy/releases)

## ✨ 主要特性

- 🔄 **多提供商支持**: 配置多个API提供商，自动轮换使用
- 🎯 **智能重试**: 请求失败时自动重试，支持故障转移
- 🔒 **隐私保护**: 日志中自动屏蔽敏感的Token信息
- 📊 **详细日志**: 彩色日志输出，清晰显示请求状态
- ⚡ **高性能**: 基于Rust和Tokio的异步架构，使用rustls提供TLS支持
- 🎨 **美观界面**: 彩色终端输出，提升用户体验
- 🔧 **易于配置**: 简单的JSON配置文件
- 🌍 **跨平台**: 支持 macOS、Linux 和 Windows

---
#### ⚠️ 注意
- 主要适用于 [Claude Code](https://docs.anthropic.com/zh-CN/docs/claude-code/overview)
- 建议多申请几个不同的API Key自动轮训，推荐链接：
  - [Any Router](https://anyrouter.top/register?aff=o14E)
  - [wenwen-ai](https://code.wenwen-ai.com/register?aff=Qs7r)

## 📦 安装

### 快速安装（推荐）

#### Linux/macOS
```bash
# 自动检测系统架构并下载最新版本
curl -L -o auto-proxy.tar.gz "https://github.com/mintya/auto-proxy/releases/latest/download/auto-proxy-$(curl -s https://api.github.com/repos/mintya/auto-proxy/releases/latest | grep tag_name | cut -d '"' -f 4 | sed 's/v//')-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m).tar.gz"
tar -xzf auto-proxy.tar.gz
chmod +x auto-proxy
./auto-proxy --help
   ```

#### Windows
```shell
# 下载最新版本（需要手动替换版本号）
Invoke-WebRequest -Uri "https://github.com/mintya/auto-proxy/releases/latest/download/auto-proxy-{VERSION}-windows-x86_64.zip" -OutFile "auto-proxy.zip"
Expand-Archive -Path "auto-proxy.zip" -DestinationPath "."
.\auto-proxy.exe --help
```

### 从 Release 下载
- 访问 [Releases页面](https://github.com/mintya/auto-proxy/releases)
- 根据您的系统下载对应版本：
  - **macOS**
    - Intel (x86_64): `auto-proxy-{VERSION}-macos-x86_64.tar.gz`
    - Apple Silicon (aarch64): `auto-proxy-{VERSION}-macos-aarch64.tar.gz`
  - **Linux**
    - x86_64: `auto-proxy-{VERSION}-linux-x86_64.tar.gz`
    - aarch64: `auto-proxy-{VERSION}-linux-aarch64.tar.gz`
  - **Windows**
    - x86_64: `auto-proxy-{VERSION}-windows-x86_64.zip`
- 解压并运行：
```bash
# Linux/macOS
tar -xzf auto-proxy-*.tar.gz
chmod +x auto-proxy
./auto-proxy --help

# Windows
# 解压 zip 文件后运行
auto-proxy.exe --help
```

### 验证下载
每个 release 都包含 `SHA256SUMS` 文件用于验证下载完整性:
```bash
# 下载校验和文件
curl -L -O "https://github.com/mintya/auto-proxy/releases/latest/download/SHA256SUMS"

# 验证文件完整性
sha256sum -c SHA256SUMS
```

### 从源码编译

```bash
# 克隆仓库
git clone https://github.com/mintya/auto-proxy.git
cd auto-proxy

# 构建
cargo build --release

# 运行
./target/release/auto-proxy --help
```

### 使用Cargo安装

```bash
cargo install --git https://github.com/mintya/auto-proxy.git
```

## 🔧 配置

### 配置文件位置
配置文件默认位于：
- **macOS/Linux**: `~/.claude-proxy-manager/providers.json`
- **Windows**: `%USERPROFILE%\.claude-proxy-manager\providers.json`

### 配置文件格式
首次运行时，程序会自动创建配置文件模板：
```json
[
  {
    "name": "provider_1",
    "token": "sk-your-token-here",
    "base_url": "https://api.example.com",
    "key_type": "AUTH_TOKEN"
  },
  {
    "name": "provider_2", 
    "token": "sk-another-token",
    "base_url": "https://api.another.com",
    "key_type": "AUTH_TOKEN"
  }
]

```

### 配置字段说明
- `name`: 提供商名称，用于标识不同的配置
- `token`: API token，用于认证请求
- `base_url`: API 基础 URL，用于构建完整的请求地址
- `key_type`: 认证方式，当前支持 `AUTH_TOKEN`

### 配置文件处理逻辑

- ✅ 默认配置文件不存在时，自动创建目录和模板文件
- ❌ 通过 --config 指定的文件不存在时，提示错误并退出
- ❌ 配置文件格式错误或为空时，提示错误并退出
- 🔄 程序会轮换使用配置文件中的所有提供商

### 功能

- 监听HTTP请求并转发到目标服务器
- 自动替换请求中的Authorization头中的token
- 自动替换或添加Host头
- 支持从配置文件读取多个服务提供商的配置

## 🚀 使用方法

### 基本用法

```bash
# 使用默认端口 8080 和默认配置文件
auto-proxy

# 指定端口
auto-proxy --port 3000

# 指定配置文件
auto-proxy --config /path/to/config.json

# 同时指定端口和配置文件
auto-proxy --port 3000 --config /path/to/config.json
```

### 命令行参数

```bash
USAGE:
    auto-proxy [OPTIONS]

OPTIONS:
    -p, --port <PORT>        监听端口 [default: 8080]
    -c, --config <CONFIG>    配置文件路径 [default: ~/.claude-proxy-manager/providers.json]
    -h, --help              显示帮助信息
    -V, --version           显示版本信息
```

### 使用示例
启动代理服务器后，您可以通过以下方式使用：
```bash
# 启动代理服务器
auto-proxy --port 8080

# 在另一个终端中测试
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer your-token" \
  -d '{"model": "gpt-3.5-turbo", "messages": [{"role": "user", "content": "Hello!"}]}'
```

## 🔒 隐私和安全

- **Token 保护**: 日志中自动屏蔽 API Token 的敏感部分
- **TLS 支持**: 使用 rustls 提供现代化的 TLS 实现
- **无数据存储**: 代理服务器不存储任何请求数据
- **本地运行**: 所有处理都在本地进行

## 🛠️ 开发

### 构建要求
- Rust 1.70 或更高版本
- 支持的目标平台：
  - x86_64-apple-darwin (macOS Intel)
  - aarch64-apple-darwin (macOS Apple Silicon)
  - x86_64-unknown-linux-gnu (Linux x86_64)
  - aarch64-unknown-linux-gnu (Linux aarch64)
  - x86_64-pc-windows-msvc (Windows x86_64)

### 本地开发
```bash
# 克隆项目
git clone https://github.com/mintya/auto-proxy.git
cd auto-proxy

# 运行测试
cargo test

# 开发模式运行
cargo run -- --port 8080

# 发布模式构建
cargo build --release
```

### 依赖说明
主要依赖：
- **tokio**: 异步运行时
- **hyper**: HTTP 客户端和服务器
- **hyper-rustls**: TLS 支持（纯 Rust 实现）
- **serde**: JSON 序列化/反序列化
- **clap**: 命令行参数解析
- **colored**: 彩色终端输出

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

- Fork 本仓库
- 创建特性分支 (git checkout -b feature/amazing-feature)
- 提交更改 (git commit -m 'Add some amazing feature')
- 推送到分支 (git push origin feature/amazing-feature)
- 开启 Pull Request

## 📄 许可证
本项目采用 MIT 许可证 - 查看 [LICENSE](https://opensource.org/licenses/MIT) 文件了解详情。

## 🔗 相关链接
- [GitHub 仓库](https://github.com/mintya/auto-proxy)
- [问题反馈](https://github.com/mintya/auto-proxy/issues)
- [最新版本](https://github.com/mintya/auto-proxy/releases/latest)  

--- 

如果这个项目对您有帮助，请考虑给它一个 ⭐️！

[![Stargazers over time](https://starchart.cc/mintya/auto-proxy.svg?variant=adaptive)](https://starchart.cc/mintya/auto-proxy)
