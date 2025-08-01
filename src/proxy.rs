//! 代理请求处理功能

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::time::Duration;
use std::convert::Infallible;
use std::path::PathBuf;
use hyper::{Body, Client, Request, Response};
use hyper_rustls::HttpsConnectorBuilder;
use http::header::{HeaderValue, AUTHORIZATION, HOST};
use tokio::time::sleep;
use colored::*;
use crate::provider::Provider;

/// 代理状态管理
pub struct ProxyState {
    /// 上次成功的服务商索引
    pub last_successful_provider: AtomicUsize,
    /// 配置文件路径
    pub config_path: std::sync::Mutex<Option<PathBuf>>,
    /// 是否为首次请求
    pub is_first_request: AtomicBool,
}

impl ProxyState {
    pub fn new() -> Self {
        Self {
            last_successful_provider: AtomicUsize::new(0),
            config_path: std::sync::Mutex::new(None),
            is_first_request: AtomicBool::new(true),
        }
    }
    
    /// 设置配置文件路径
    pub fn set_config_path(&self, path: Option<PathBuf>) {
        *self.config_path.lock().unwrap() = path;
    }
    
    /// 初始化优先服务商索引
    pub fn initialize_preferred_provider(&self, providers: &[Provider]) {
        // 查找配置中标记为优先的服务商
        if let Some(index) = providers.iter().position(|p| p.is_preferred()) {
            self.last_successful_provider.store(index, Ordering::Relaxed);
            println!("{} 从配置文件读取到优先服务商: {}", 
                "⭐".bright_yellow(), 
                providers[index].name.bright_green()
            );
        }
    }
    
    /// 更新配置文件中的优先服务商
    pub async fn update_preferred_provider_in_config(&self, providers: &mut [Provider], new_preferred_index: usize) {
        // 重置所有服务商的优先标记
        for provider in providers.iter_mut() {
            provider.set_preferred(false);
        }
        
        // 设置新的优先服务商
        if new_preferred_index < providers.len() {
            providers[new_preferred_index].set_preferred(true);
            
            // 获取配置文件路径的拷贝，避免跨异步边界持有 MutexGuard
            let config_path = {
                self.config_path.lock().unwrap().clone()
            };
            
            // 保存到配置文件
            if let Some(config_path) = config_path {
                match self.save_config_file(&config_path, providers).await {
                    Ok(_) => {
                        println!("{} 已更新配置文件中的优先服务商: {}", 
                            "💾".cyan(), 
                            providers[new_preferred_index].name.bright_green()
                        );
                    }
                    Err(e) => {
                        println!("{} 更新配置文件失败: {}", "❌".red(), e);
                    }
                }
            }
        }
    }
    
    /// 保存配置文件
    async fn save_config_file(&self, config_path: &PathBuf, providers: &[Provider]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let json_content = serde_json::to_string_pretty(providers)?;
        tokio::fs::write(config_path, json_content).await?;
        Ok(())
    }
}

/// 处理代理请求
/// 
/// # 参数
/// * `req` - 原始HTTP请求
/// * `providers` - 提供商列表
/// * `state` - 代理状态管理
/// 
/// # 返回
/// * `Ok(Response<Body>)` - 成功的响应
/// * `Err(Infallible)` - 不可能的错误（用于满足hyper的类型要求）
pub async fn handle_request(req: Request<Body>, providers: Arc<Vec<Provider>>, state: Arc<ProxyState>) -> Result<Response<Body>, Infallible> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    
    println!("{} {} {}", "🔄".cyan(), method.to_string().bright_blue(), uri.to_string().bright_white());
    
    // 获取请求体
    let body_bytes = match hyper::body::to_bytes(req.into_body()).await {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("{} {}", "❌ 读取请求体失败:".red(), e);
            return Ok(Response::builder()
                .status(400)
                .body(Body::from("Bad Request"))
                .unwrap());
        }
    };
    
    let is_first_request = state.is_first_request.swap(false, Ordering::Relaxed);
    
    if is_first_request {
        // 首次请求：先尝试优先服务商，失败后并行尝试所有服务商
        handle_first_request(&providers, &state, &method, &uri, &headers, &body_bytes).await
    } else {
        // 后续请求：优先尝试上次成功的服务商
        handle_subsequent_request(&providers, &state, &method, &uri, &headers, &body_bytes).await
    }
}

/// 处理首次请求
async fn handle_first_request(
    providers: &Arc<Vec<Provider>>, 
    state: &Arc<ProxyState>,
    method: &hyper::Method,
    uri: &hyper::Uri,
    headers: &hyper::HeaderMap,
    body_bytes: &hyper::body::Bytes,
) -> Result<Response<Body>, Infallible> {
    let preferred_index = state.last_successful_provider.load(Ordering::Relaxed);
    
    // 如果有优先服务商，先尝试它
    if preferred_index < providers.len() && providers[preferred_index].is_preferred() {
        let provider = &providers[preferred_index];
        println!("{} 首次请求 - 优先尝试配置的首选服务商: {} ({})", 
            "⭐".bright_yellow(), 
            provider.name.bright_green(), 
            provider.base_url.bright_blue()
        );
        
        // 对优先服务商重试3次
        for retry in 0..3 {
            if retry > 0 {
                println!("{} 优先服务商第 {} 次重试...", "🔄".yellow(), retry + 1);
                sleep(Duration::from_millis(500)).await;
            }
            
            match try_provider(provider, method, uri, headers, body_bytes).await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        println!("{} 优先服务商请求成功: {}", 
                            "✅".green(), 
                            status.to_string().bright_green()
                        );
                        return Ok(response);
                    } else {
                        println!("{} 优先服务商请求失败: {}", 
                            "❌".red(), 
                            status.to_string().bright_red()
                        );
                    }
                }
                Err(e) => {
                    println!("{} 优先服务商网络错误: {}", 
                        "❌".red(), 
                        e.to_string().bright_red()
                    );
                }
            }
        }
        
        println!("{} 优先服务商失败，开始并行尝试所有服务商...", "🚀".bright_blue());
    } else {
        println!("{} 首次请求 - 并行尝试所有服务商...", "🚀".bright_blue());
    }
    
    // 并行尝试所有服务商
    let mut tasks = Vec::new();
    
    for (index, provider) in providers.iter().enumerate() {
        let provider = provider.clone();
        let method = method.clone();
        let uri = uri.clone();
        let headers = headers.clone();
        let body_bytes = body_bytes.clone();
        
        let task = tokio::spawn(async move {
            match try_provider(&provider, &method, &uri, &headers, &body_bytes).await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        Some((index, provider.name.clone(), response))
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        });
        
        tasks.push(task);
    }
    
    // 等待第一个成功的响应
    for task in tasks {
        if let Ok(Some((successful_index, provider_name, response))) = task.await {
            state.last_successful_provider.store(successful_index, Ordering::Relaxed);
            println!("{} 并行请求成功 - 服务商: {}，已设为下次优先选择", 
                "🎯".bright_green(), 
                provider_name.bright_green()
            );
            
            // 异步更新配置文件（不阻塞响应）
            let providers_clone = providers.clone();
            let state_clone = state.clone();
            tokio::spawn(async move {
                let mut providers_mut: Vec<Provider> = (*providers_clone).clone();
                state_clone.update_preferred_provider_in_config(&mut providers_mut, successful_index).await;
            });
            
            return Ok(response);
        }
    }
    
    // 所有服务商都失败了
    println!("{} 所有服务商都失败了", "💥".red());
    Ok(Response::builder()
        .status(500)
        .body(Body::from("All providers failed"))
        .unwrap())
}

/// 处理后续请求
async fn handle_subsequent_request(
    providers: &Arc<Vec<Provider>>, 
    state: &Arc<ProxyState>,
    method: &hyper::Method,
    uri: &hyper::Uri,
    headers: &hyper::HeaderMap,
    body_bytes: &hyper::body::Bytes,
) -> Result<Response<Body>, Infallible> {
    let provider_count = providers.len();
    let last_successful_index = state.last_successful_provider.load(Ordering::Relaxed);
    
    // 创建一个重新排序的提供商索引列表，将上次成功的放在首位
    let mut provider_indices: Vec<usize> = (0..provider_count).collect();
    if last_successful_index < provider_count {
        provider_indices.remove(last_successful_index);
        provider_indices.insert(0, last_successful_index);
    }
    
    for (try_count, &provider_index) in provider_indices.iter().enumerate() {
        let provider = &providers[provider_index];
        
        if try_count == 0 && provider_index == last_successful_index {
            println!("{} 优先尝试上次成功的提供商: {} ({})", 
                "⭐".yellow(), 
                provider.name.bright_green(), 
                provider.base_url.bright_blue()
            );
        } else {
            println!("{} 尝试提供商: {} ({})", 
                "🎯".yellow(), 
                provider.name.bright_green(), 
                provider.base_url.bright_blue()
            );
        }
        
        // 对每个提供商重试3次
        for retry in 0..3 {
            if retry > 0 {
                println!("{} 第 {} 次重试...", "🔄".yellow(), retry + 1);
                sleep(Duration::from_millis(500)).await;
            }
            
            match try_provider(&provider, &method, &uri, &headers, &body_bytes).await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        // 如果成功的服务商不是当前优先的，更新优先服务商
                        let current_preferred = state.last_successful_provider.load(Ordering::Relaxed);
                        if provider_index != current_preferred {
                            state.last_successful_provider.store(provider_index, Ordering::Relaxed);
                            println!("{} 请求成功: {}，已更新优先服务商为: {}", 
                                "✅".green(), 
                                status.to_string().bright_green(),
                                provider.name.bright_cyan()
                            );
                            
                            // 异步更新配置文件
                            let providers_clone = providers.clone();
                            let state_clone = state.clone();
                            tokio::spawn(async move {
                                let mut providers_mut: Vec<Provider> = (*providers_clone).clone();
                                state_clone.update_preferred_provider_in_config(&mut providers_mut, provider_index).await;
                            });
                        } else {
                            println!("{} 请求成功: {}", 
                                "✅".green(), 
                                status.to_string().bright_green()
                            );
                        }
                        return Ok(response);
                    } else {
                        println!("{} 请求失败: {}", "❌".red(), status.to_string().bright_red());
                    }
                }
                Err(e) => {
                    println!("{} 网络错误: {}", "❌".red(), e.to_string().bright_red());
                }
            }
        }
        
        // 如果不是最后一个要尝试的提供商，继续尝试下一个
        if try_count < provider_indices.len() - 1 {
            println!("{} 切换到下一个提供商...", "🔄".yellow());
        }
    }
    
    // 所有提供商都失败了
    println!("{} 所有提供商都失败了", "💥".red());
    Ok(Response::builder()
        .status(500)
        .body(Body::from("All providers failed"))
        .unwrap())
}

/// 尝试使用指定提供商发送请求
async fn try_provider(
    provider: &Provider,
    method: &hyper::Method,
    uri: &hyper::Uri,
    headers: &hyper::HeaderMap,
    body_bytes: &hyper::body::Bytes,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    let https = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();
    let client = Client::builder().build::<_, hyper::Body>(https);
    
    // 构建新的URI
    let target_uri = format!("{}{}", provider.base_url, uri.path_and_query().map(|x| x.as_str()).unwrap_or("/"));
    let target_uri: hyper::Uri = target_uri.parse()?;
    
    // 创建新请求
    let mut new_req = Request::builder()
        .method(method)
        .uri(&target_uri);
    
    // 复制原始请求头，但跳过某些头
    for (name, value) in headers {
        if name != "host" && name != "authorization" {
            new_req = new_req.header(name, value);
        }
    }
    
    // 设置新的Authorization头
    let masked_token = provider.masked_token();
    println!("{} 使用Token: {}", "🔑".cyan(), masked_token.bright_yellow());
    
    new_req = new_req.header(AUTHORIZATION, format!("Bearer {}", provider.token));
    
    // 设置新的Host头
    if let Some(host) = target_uri.host() {
        let target_host = if let Some(port) = target_uri.port_u16() {
            format!("{}:{}", host, port)
        } else {
            host.to_string()
        };
        let target_host_str = target_host.as_str();
        new_req = new_req.header(HOST, HeaderValue::from_str(target_host_str)?);
    }
    
    let new_req = new_req.body(Body::from(body_bytes.clone()))?;
    
    // 发送请求
    let response = client.request(new_req).await?;
    Ok(response)
}