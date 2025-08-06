//! Auto Proxy - 智能代理服务器主程序
//! 
//! 这是一个支持多提供商的智能代理服务器，具有自动重试和故障转移功能。

use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::convert::Infallible;
use std::time::Duration;
use hyper::service::{make_service_fn, service_fn};
use hyper::Server;
use colored::*;
use tokio::time::interval;
use tokio::signal;
use auto_proxy::{read_providers_config, handle_request, ProxyState, TerminalUI, ServerInfo, NetworkStatus};

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

    /// 禁用终端UI，使用传统日志输出
    #[arg(long)]
    no_ui: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行参数
    let args = Args::parse();
    
    // 读取配置文件
    let (providers, _actual_config_path) = match read_providers_config(args.config) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{} {}", "❌ 配置加载失败:".red().bold(), e);
            return Err(e.into());
        }
    };

    let providers = Arc::new(providers);
    let state = Arc::new(ProxyState::new_with_rate_limit(args.rate_limit));
    let server_info = Arc::new(ServerInfo::new(args.port, args.rate_limit));

    if args.no_ui {
        // 传统日志模式
        run_traditional_mode(providers, state, server_info, args.port).await
    } else {
        // 终端UI模式
        run_ui_mode(providers, state, server_info, args.port).await
    }
}

/// 运行传统日志模式
async fn run_traditional_mode(
    providers: Arc<Vec<auto_proxy::Provider>>,
    state: Arc<ProxyState>,
    _server_info: Arc<ServerInfo>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "🚀 Auto Proxy 启动中...".bright_blue().bold());
    println!();
    
    // 打印提供商信息
    println!("{}", "📋 已加载的提供商:".bright_green().bold());
    for (index, provider) in providers.iter().enumerate() {
        println!("  {}. {} - {} (Token: {})", 
            index + 1,
            provider.name.bright_cyan(), 
            provider.base_url.bright_white(),
            provider.masked_token().bright_yellow()
        );
    }
    println!();
    
    println!("{}", "⚡ 负载均衡模式: 轮询 + 健康度权重".bright_green());
    println!("{} 速率限制: 每个供应商每分钟最多 {} 次请求", "🎯".cyan(), state.get_rate_limit());
    println!("{} 健康度系统: 自动故障恢复和快速失败", "💚".green());
    println!();

    // 启动HTTP服务器
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    
    let make_svc = make_service_fn(move |_conn| {
        let providers = Arc::clone(&providers);
        let state = Arc::clone(&state);
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                handle_request(req, Arc::clone(&providers), Arc::clone(&state))
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);
    
    println!("{} 服务器启动成功，监听端口: {}", 
        "🌟".bright_green(), 
        port.to_string().bright_yellow().bold()
    );
    println!("{} 访问地址: {}", 
        "🔗".cyan(), 
        format!("http://localhost:{}", port).bright_blue().underline()
    );
    println!();

    if let Err(e) = server.await {
        eprintln!("{} {}", "❌ 服务器错误:".red().bold(), e);
    }

    Ok(())
}

/// 运行终端UI模式
async fn run_ui_mode(
    providers: Arc<Vec<auto_proxy::Provider>>,
    state: Arc<ProxyState>,
    server_info: Arc<ServerInfo>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    // 初始化终端UI
    let mut terminal_ui = TerminalUI::new()?;
    terminal_ui.initialize()?;
    
    let logger = terminal_ui.logger();
    
    // 异步检测网络状态，不阻塞启动
    let server_info_clone = Arc::clone(&server_info);
    tokio::spawn(async move {
        let network_status = NetworkStatus::detect().await;
        server_info_clone.update_network_status(network_status.clone());
    });
    
    // 记录启动日志
    logger.info("🚀 Auto Proxy 启动中...".to_string());
    logger.info(format!("📋 已加载 {} 个提供商", providers.len()));
    
    for provider in providers.iter() {
        logger.info(format!("  - {} ({})", provider.name, provider.masked_token()));
    }
    
    logger.info("⚡ 负载均衡模式: 轮询 + 健康度权重".to_string());
    logger.info(format!("🎯 速率限制: 每个供应商每分钟最多 {} 次请求", server_info.rate_limit));
    logger.info("💚 健康度系统: 自动故障恢复和快速失败".to_string());

    // 为服务器和UI任务克隆引用
    let server_providers = Arc::clone(&providers);
    let server_state = Arc::clone(&state);
    let ui_providers = Arc::clone(&providers);
    let ui_state = Arc::clone(&state);
    let ui_server_info = Arc::clone(&server_info);

    // 创建全局日志记录器用于代理模块和退出处理
    let global_logger = Arc::new(logger.clone());
    let server_logger = Arc::clone(&global_logger);
    let exit_logger = Arc::clone(&global_logger);
    
    // 启动HTTP服务器
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    
    let make_svc = make_service_fn(move |_conn| {
        let providers = Arc::clone(&server_providers);
        let state = Arc::clone(&server_state);
        let logger = Arc::clone(&server_logger);
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                auto_proxy::handle_request_with_logger(req, Arc::clone(&providers), Arc::clone(&state), Some(Arc::clone(&logger)))
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);
    
    logger.success(format!("🌟 服务器启动成功，监听端口: {}", port));
    logger.info(format!("🔗 访问地址: http://localhost:{}", port));

    // 启动UI渲染和事件处理任务
    let ui_providers_clone = Arc::clone(&ui_providers);
    let ui_state_clone = Arc::clone(&ui_state);
    let ui_server_info_clone = Arc::clone(&ui_server_info);
    let ui_logger = Arc::clone(&global_logger);
    
    let ui_task = tokio::spawn(async move {
        let mut render_interval = interval(Duration::from_millis(100)); // 10 FPS渲染
        let mut event_interval = interval(Duration::from_millis(16)); // ~60 FPS事件检查
        
        // 添加信号处理以确保优雅关闭
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        
        loop {
            tokio::select! {
                _ = render_interval.tick() => {
                    // 渲染UI
                    if let Err(e) = terminal_ui.render(&ui_providers_clone, &ui_state_clone, &ui_server_info_clone) {
                        eprintln!("⚠️ UI渲染错误: {}", e);
                        ui_logger.error(format!("UI渲染失败: {}", e));
                        break;
                    }
                },
                _ = event_interval.tick() => {
                    // 检查事件（更频繁）
                    if let Ok(key_action) = terminal_ui.check_key_input() {
                        match key_action.as_str() {
                            "exit" => {
                                ui_logger.info("用户请求退出...".to_string());
                                break;
                            }
                            action if action.starts_with("toggle:") => {
                                // 处理服务商启用/禁用切换
                                let provider_name = &action[7..]; // 移除 "toggle:" 前缀
                                let was_disabled = ui_state_clone.interactive_manager.toggle_provider(provider_name);
                                let status = if was_disabled { "禁用" } else { "启用" };
                                ui_logger.info(format!("服务商 {} 已{}", provider_name, status));
                            }
                            _ => {}
                        }
                    }
                },
                _ = &mut ctrl_c => {
                    ui_logger.info("接收到中断信号，正在优雅关闭...".to_string());
                    break;
                }
            }
        }
        
        // 确保终端状态被正确清理
        if let Err(e) = terminal_ui.cleanup() {
            eprintln!("⚠️ 终端清理失败: {}", e);
        }
        ui_logger.success("UI任务已安全退出".to_string());
    });

    // 运行服务器
    let server_result = tokio::select! {
        result = server => {
            // 服务器正常结束或出错
            result
        },
        _ = ui_task => {
            // UI 任务结束（用户按了退出键）
            Ok(())
        }
        _ = signal::ctrl_c() => {
            // 接收到 Ctrl+C 信号
            exit_logger.info("接收到 Ctrl+C 信号，正在优雅退出...".to_string());
            Ok(())
        }
    };

    if let Err(e) = server_result {
        eprintln!("{} {}", "❌ 服务器错误:".red().bold(), e);
    }

    // 程序退出前的清理工作
    println!("🔧 正在清理终端状态...");
    
    Ok(())
}

