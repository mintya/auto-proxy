//! Auto Proxy - 智能代理服务器主程序
//! 
//! 这是一个支持多提供商的智能代理服务器，具有自动重试和故障转移功能。

use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::convert::Infallible;
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;
use colored::*;
use auto_proxy::{read_providers_config, handle_request, ProxyState};

/// 命令行参数
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 监听的端口号
    #[arg(short, long, default_value_t = 8080)]
    port: u16,
    
    /// 配置文件路径
    #[arg(short, long)]
    config: Option<PathBuf>,
    
    /// 每个供应商每分钟最大请求数
    #[arg(short = 'r', long, default_value_t = 5)]
    rate_limit: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行参数
    let args = Args::parse();
    
    println!("{}", "🚀 Auto Proxy 启动中...".bright_blue().bold());
    println!();
    
    // 读取配置文件
    let (providers, _actual_config_path) = match read_providers_config(args.config) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{} {}", "❌ 配置加载失败:".red().bold(), e);
            std::process::exit(1);
        }
    };
    
    // 打印提供商信息
    println!("{}", "📋 已加载的提供商:".bright_green().bold());
    for (i, provider) in providers.iter().enumerate() {
        let masked_token = provider.masked_token();
        println!("  {}. {} - {} (Token: {})", 
            (i + 1).to_string().bright_white(),
            provider.name.bright_cyan(), 
            provider.base_url.bright_blue(),
            masked_token.bright_yellow()
        );
    }
    println!();
    println!("{}", format!("⚡ 负载均衡模式: 轮询 + 健康度权重").bright_magenta());
    println!("{}", format!("🎯 速率限制: 每个供应商每分钟最多 {} 次请求", args.rate_limit).bright_magenta());
    println!("{}",         "💚 健康度系统: 自动故障恢复和快速失败".bright_magenta());
    println!();
    
    // 构建监听地址
    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    
    // 将providers包装在Arc中以便在多个请求间共享
    let providers = Arc::new(providers);
    
    // 创建代理状态管理
    let state = Arc::new(ProxyState::new_with_rate_limit(args.rate_limit));
    
    // 创建服务
    let make_svc = make_service_fn(move |_conn| {
        let providers = providers.clone();
        let state = state.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let providers = providers.clone();
                let state = state.clone();
                async move { handle_request(req, providers, state).await }
            }))
        }
    });
    
    // 启动服务器
    let server = Server::bind(&addr).serve(make_svc);
    
    println!("{}", "🌐 HTTP代理服务器已启动!".green().bold());
    println!("{}", format!("📡 监听地址: {}", addr).cyan());
    println!();
    println!("{}", "📝 使用说明:".yellow().bold());
    println!("{}", format!("   export ANTHROPIC_BASE_URL=\"http://localhost:{}\"", args.port).bright_blue());
    println!("{}", "   export ANTHROPIC_AUTH_TOKEN=\"sk-your-token-here\"".bright_blue());
    println!();
    println!("{}", "🔄 负载均衡特性:".green().bold());
    println!("{}",         "   • 智能轮询算法，自动分散负载".white());
    println!("{}",         "   • 健康度追踪，失败供应商自动降权".white());
    println!("{}",         "   • 快速失败机制，避免无效重试".white());
    println!("{}",         "   • 紧急恢复模式，确保服务可用性".white());
    println!();
    println!("{}", "✅ 代理服务已就绪，等待请求...".green());
    
    // 等待服务器关闭
    if let Err(e) = server.await {
        eprintln!("{}", format!("服务器错误: {}", e).red());
    }
    
    Ok(())
}
