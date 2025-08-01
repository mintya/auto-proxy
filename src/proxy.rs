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
use crate::provider::{Provider, RateLimiter};
use std::collections::HashMap;

/// 代理状态管理
pub struct ProxyState {
    /// 上次成功的服务商索引
    pub last_successful_provider: AtomicUsize,
    /// 配置文件路径
    pub config_path: std::sync::Mutex<Option<PathBuf>>,
    /// 是否为首次请求
    pub is_first_request: AtomicBool,
    /// 每个提供商的速率限制器
    pub rate_limiters: std::sync::Mutex<HashMap<String, RateLimiter>>,
    /// 全局速率限制值
    pub rate_limit: usize,
}

impl ProxyState {
    pub fn new() -> Self {
        Self::new_with_rate_limit(1000)
    }
    
    pub fn new_with_rate_limit(rate_limit: usize) -> Self {
        Self {
            last_successful_provider: AtomicUsize::new(0),
            config_path: std::sync::Mutex::new(None),
            is_first_request: AtomicBool::new(true),
            rate_limiters: std::sync::Mutex::new(HashMap::new()),
            rate_limit,
        }
    }
    
    /// 设置配置文件路径
    pub fn set_config_path(&self, path: Option<PathBuf>) {
        *self.config_path.lock().unwrap() = path;
    }
    
    /// 初始化优先服务商索引
    pub fn initialize_preferred_provider(&self, providers: &[Provider]) {
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
        for provider in providers.iter_mut() {
            provider.set_preferred(false);
        }
        
        if new_preferred_index < providers.len() {
            providers[new_preferred_index].set_preferred(true);
            
            let config_path = {
                self.config_path.lock().unwrap().clone()
            };
            
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
    
    /// 检查提供商是否可以发起请求（速率限制）
    pub fn can_request(&self, provider_name: &str) -> bool {
        let mut limiters = self.rate_limiters.lock().unwrap();
        let limiter = limiters.entry(provider_name.to_string())
            .or_insert_with(|| RateLimiter::new(self.rate_limit));
        limiter.can_request()
    }
    
    /// 记录一次请求到指定提供商
    pub fn record_request(&self, provider_name: &str) {
        let mut limiters = self.rate_limiters.lock().unwrap();
        let limiter = limiters.entry(provider_name.to_string())
            .or_insert_with(|| RateLimiter::new(self.rate_limit));
        limiter.record_request();
    }
    
    /// 获取提供商当前请求数量
    pub fn get_current_requests(&self, provider_name: &str) -> usize {
        let mut limiters = self.rate_limiters.lock().unwrap();
        let limiter = limiters.entry(provider_name.to_string())
            .or_insert_with(|| RateLimiter::new(self.rate_limit));
        limiter.current_requests()
    }
    
    /// 获取速率限制值
    pub fn get_rate_limit(&self) -> usize {
        self.rate_limit
    }
}

/// 处理代理请求
pub async fn handle_request(req: Request<Body>, providers: Arc<Vec<Provider>>, state: Arc<ProxyState>) -> Result<Response<Body>, Infallible> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    
    println!("{} {} {}", "🔄".cyan(), method.to_string().bright_blue(), uri.to_string().bright_white());
    
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
        handle_first_request(&providers, &state, &method, &uri, &headers, &body_bytes).await
    } else {
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
            
            match try_provider(provider, method, uri, headers, body_bytes, state).await {
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
        let state_clone = state.clone();
        
        let task = tokio::spawn(async move {
            match try_provider(&provider, &method, &uri, &headers, &body_bytes, &state_clone).await {
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
            
            match try_provider(&provider, &method, &uri, &headers, &body_bytes, state).await {
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

async fn try_provider(
    provider: &Provider,
    method: &hyper::Method,
    uri: &hyper::Uri,
    headers: &hyper::HeaderMap,
    body_bytes: &hyper::body::Bytes,
    state: &Arc<ProxyState>,
) -> Result<Response<Body>, Box<dyn std::error::Error + Send + Sync>> {
    // 检查速率限制
    if !state.can_request(&provider.name) {
        let current_requests = state.get_current_requests(&provider.name);
        let rate_limit = state.get_rate_limit();
        println!("{} 服务商 {} 已达到速率限制 ({}/{}/分钟)", 
            "⚠️".yellow(), 
            provider.name.bright_yellow(),
            current_requests.to_string().bright_red(),
            rate_limit.to_string().bright_white()
        );
        return Err("Rate limit exceeded".into());
    }
    
    // 记录请求
    state.record_request(&provider.name);
    
    let https = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .build();
    let client = Client::builder().build::<_, hyper::Body>(https);
    
    let target_uri = format!("{}{}", provider.base_url, uri.path_and_query().map(|x| x.as_str()).unwrap_or("/"));
    let target_uri: hyper::Uri = target_uri.parse()?;
    
    let mut new_req = Request::builder()
        .method(method)
        .uri(&target_uri);
    
    // 复制原始请求头，只跳过需要重新设置的关键头部
    for (name, value) in headers {
        let name_lower = name.as_str().to_lowercase();
        if name_lower == "host" || name_lower == "authorization" {
            continue;
        }
        new_req = new_req.header(name, value);
    }
    
    // 设置新的Authorization和Host头
    let masked_token = provider.masked_token();
    let current_requests = state.get_current_requests(&provider.name);
    let rate_limit = state.get_rate_limit();
    println!("{} 使用Token: {} ({}/{})", 
        "🔑".cyan(), 
        masked_token.bright_yellow(),
        current_requests.to_string().bright_cyan(),
        rate_limit.to_string().bright_white()
    );
    
    new_req = new_req.header(AUTHORIZATION, format!("Bearer {}", provider.token));
    
    if let Some(host) = target_uri.host() {
        let target_host = if let Some(port) = target_uri.port_u16() {
            format!("{}:{}", host, port)
        } else {
            host.to_string()
        };
        new_req = new_req.header(HOST, HeaderValue::from_str(&target_host)?);
    }
    
    let new_req = new_req.body(Body::from(body_bytes.clone()))?;
    
    let response = client.request(new_req).await?;
    
    // 对于错误响应，记录错误信息但不打印详细内容
    if !response.status().is_success() {
        let status = response.status();
        println!("{} 请求失败: {} - {}", 
            "❌".red(), 
            status.as_u16().to_string().bright_red(),
            provider.name.bright_white()
        );
    }
    
    Ok(response)
}