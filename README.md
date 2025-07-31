# 🚀 Auto Proxy

一个支持多提供商的智能代理服务器，具有自动重试和故障转移功能。

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Build Status](https://github.com/mintya/auto-proxy/workflows/Release%20Build/badge.svg)](https://github.com/mintya/auto-proxy/actions)

## ✨ 主要特性

- 🔄 **多提供商支持**: 配置多个API提供商，自动轮换使用
- 🎯 **智能重试**: 请求失败时自动重试，支持故障转移
- 🔒 **隐私保护**: 日志中自动屏蔽敏感的Token信息
- 📊 **详细日志**: 彩色日志输出，清晰显示请求状态
- ⚡ **高性能**: 基于Rust和Tokio的异步架构
- 🎨 **美观界面**: 彩色终端输出，提升用户体验
- 🔧 **易于配置**: 简单的JSON配置文件

## 📦 安装

### 从Release下载

1. 访问 [Releases页面](https://github.com/mintya/auto-proxy/releases)
2. 下载适合您系统的版本：
   - **macOS**: `auto-proxy-x.x.x-macos-{x86_64|aarch64}.tar.gz`
   - **Linux**: `auto-proxy-x.x.x-linux-{x86_64|aarch64}.tar.gz`
3. 解压并运行：
   ```bash
   tar -xzf auto-proxy-*.tar.gz
   ./auto-proxy --help
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

首次运行时，程序会自动创建配置文件 `~/.claude-proxy-manager/providers.json`：

## 功能

- 监听HTTP请求并转发到目标服务器
- 自动替换请求中的Authorization头中的token
- 自动替换或添加Host头
- 支持从配置文件读取多个服务提供商的配置

## 配置文件

配置文件默认位于`~/.claude-proxy-manager/providers.json`，格式如下：

```json
[
  {
    "name": "your_name",
    "token": "sk-your_sk",
    "base_url": "https://your_base_url",
    "key_type": "AUTH_TOKEN"
  }
]
```

### 配置文件处理逻辑

- 当默认配置文件不存在时，程序会自动创建目录和配置文件，并提示用户修改配置后重新启动
- 当通过`--config`参数指定的配置文件不存在时，程序会提示错误并退出
- 当配置文件格式不正确或为空时，程序会提示错误并退出
- 程序会使用配置文件中的第一个提供商作为默认配置

## 使用方法

### 编译

```bash
cargo build --release
```

### 运行

使用默认配置文件：

```bash
cargo run -- --port 8080
```

指定配置文件：

```bash
cargo run -- --port 8080 --config /path/to/your/config.json
```

### 命令行参数

- `-p, --port <PORT>`: 指定监听端口，默认为8080
- `-c, --config <CONFIG>`: 指定配置文件路径，默认为`~/.claude-proxy-manager/providers.json`

## 日志输出

程序会输出以下日志信息：

- 配置文件读取情况
- 原始请求方法和路径
- 转发的地址
- 响应的状态码