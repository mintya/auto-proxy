//! 代理请求处理功能

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::convert::Infallible;
use hyper::{Body, Client, Request, Response};
use hyper_rustls::HttpsConnectorBuilder;
use http::header::{HeaderValue, AUTHORIZATION, HOST};
use tokio::time::sleep;
use colored::*;
use crate::provider::{Provider, RateLimiter, ProviderHealth};
use std::collections::HashMap;

/// 代理状态管理
pub struct ProxyState {
    /// 轮询计数器
    pub round_robin_counter: AtomicUsize,
    /// 每个提供商的速率限制器
    pub rate_limiters: std::sync::Mutex<HashMap<String, RateLimiter>>,
    /// 每个提供商的健康度追踪器
    pub provider_health: std::sync::Mutex<HashMap<String, ProviderHealth>>,
    /// 全局速率限制值
    pub rate_limit: usize,
}

impl ProxyState {
    pub fn new() -> Self {
        Self::new_with_rate_limit(5)
    }
    
    pub fn new_with_rate_limit(rate_limit: usize) -> Self {
        Self {
            round_robin_counter: AtomicUsize::new(0),
            rate_limiters: std::sync::Mutex::new(HashMap::new()),
            provider_health: std::sync::Mutex::new(HashMap::new()),
            rate_limit,
        }
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
    
    /// 记录提供商成功请求
    pub fn record_provider_success(&self, provider_name: &str) {
        let mut health_map = self.provider_health.lock().unwrap();
        let health = health_map.entry(provider_name.to_string())
            .or_insert_with(|| ProviderHealth::new());
        health.record_success();
    }
    
    /// 记录提供商失败请求
    pub fn record_provider_failure(&self, provider_name: &str) {
        let mut health_map = self.provider_health.lock().unwrap();
        let health = health_map.entry(provider_name.to_string())
            .or_insert_with(|| ProviderHealth::new());
        health.record_failure();
    }
    
    /// 获取提供商健康度分数
    pub fn get_provider_health_score(&self, provider_name: &str) -> u8 {
        let mut health_map = self.provider_health.lock().unwrap();
        let health = health_map.entry(provider_name.to_string())
            .or_insert_with(|| ProviderHealth::new());
        health.get_health_score()
    }
    
    /// 检查提供商是否健康
    pub fn is_provider_healthy(&self, provider_name: &str) -> bool {
        let mut health_map = self.provider_health.lock().unwrap();
        let health = health_map.entry(provider_name.to_string())
            .or_insert_with(|| ProviderHealth::new());
        health.is_healthy()
    }
    
    /// 检查所有供应商是否都不健康
    pub fn all_providers_unhealthy(&self, providers: &[Provider]) -> bool {
        for provider in providers {
            if self.is_provider_healthy(&provider.name) {
                return false;
            }
        }
        true
    }
    
    /// 检查所有供应商是否都完全不可用
    pub fn all_providers_down(&self, providers: &[Provider]) -> bool {
        for provider in providers {
            let health_score = self.get_provider_health_score(&provider.name);
            if health_score > 0 {
                return false;
            }
        }
        true
    }
    
    /// 紧急恢复所有供应商
    pub fn emergency_recovery_all(&self, providers: &[Provider]) {
        println!("{} 启动紧急恢复模式...", "🚨".bright_red());
        let mut health_map = self.provider_health.lock().unwrap();
        for provider in providers {
            let health = health_map.entry(provider.name.clone())
                .or_insert_with(|| ProviderHealth::new());
            health.emergency_recovery();
            println!("{} 恢复供应商: {} (健康度: 10%)", 
                "🔄".cyan(), 
                provider.name.bright_yellow()
            );
        }
    }
    
    /// 打印所有提供商的健康状态汇总
    pub fn print_providers_health_summary(&self, providers: &[Provider]) {
        println!();
        println!("{}", "📊 提供商健康状态汇总:".bright_cyan().bold());
        println!("{}", "─".repeat(50).bright_black());
        
        let mut healthy_count = 0;
        let mut total_health = 0u32;
        
        for (index, provider) in providers.iter().enumerate() {
            let health_score = self.get_provider_health_score(&provider.name);
            let current_requests = self.get_current_requests(&provider.name);
            let is_healthy = health_score > 20;
            let can_request = self.can_request(&provider.name);
            
            if is_healthy {
                healthy_count += 1;
            }
            total_health += health_score as u32;
            
            // 状态图标和颜色
            let (status_icon, health_color) = match health_score {
                90..=100 => ("🟢", "bright_green"),
                70..=89 => ("🟡", "bright_yellow"), 
                40..=69 => ("🟠", "yellow"),
                20..=39 => ("🔴", "bright_red"),
                _ => ("💀", "red"),
            };
            
            let rate_status = if can_request { "✅" } else { "🚫" };
            
            println!("{} {}. {} {} {}% | 速率:{}/{} {} | {}", 
                status_icon,
                (index + 1).to_string().bright_white(),
                provider.name.bright_cyan(),
                if health_score > 20 { "健康".bright_green() } else { "异常".bright_red() },
                health_score.to_string().color(health_color).bold(),
                current_requests.to_string().bright_cyan(),
                self.rate_limit.to_string().bright_white(),
                rate_status,
                if is_healthy { "可用".bright_green() } else { "不可用".bright_red() }
            );
        }
        
        println!("{}", "─".repeat(50).bright_black());
        let avg_health = if providers.is_empty() { 0 } else { total_health / providers.len() as u32 };
        println!("{} 健康供应商: {}/{} | 平均健康度: {}% | 系统状态: {}", 
            "🏥".cyan(),
            healthy_count.to_string().bright_green(),
            providers.len().to_string().bright_white(),
            avg_health.to_string().bright_yellow(),
            if healthy_count > 0 { "正常".bright_green() } else { "警告".bright_red() }
        );
        println!();
    }
    
    /// 使用轮询算法选择下一个健康的提供商
    pub fn select_next_provider(&self, providers: &[Provider]) -> Option<usize> {
        self.select_provider_with_strategy(providers, false)
    }
    
    /// 使用随机化策略选择提供商
    pub fn select_provider_randomly(&self, providers: &[Provider]) -> Option<usize> {
        self.select_provider_with_strategy(providers, true)
    }
    
    /// 选择提供商的通用方法
    fn select_provider_with_strategy(&self, providers: &[Provider], use_random: bool) -> Option<usize> {
        if providers.is_empty() {
            return None;
        }
        
        let provider_count = providers.len();
        let start_index = if use_random {
            // 使用随机起点，避免并发请求冲突
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            use std::time::{SystemTime, UNIX_EPOCH};
            
            let mut hasher = DefaultHasher::new();
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos().hash(&mut hasher);
            std::thread::current().id().hash(&mut hasher);
            (hasher.finish() as usize) % provider_count
        } else {
            self.round_robin_counter.fetch_add(1, Ordering::Relaxed) % provider_count
        };
        
        // 从当前索引开始轮询查找健康的提供商
        for i in 0..provider_count {
            let index = (start_index + i) % provider_count;
            let provider = &providers[index];
            
            // 检查速率限制和健康状态
            if self.can_request(&provider.name) && self.is_provider_healthy(&provider.name) {
                return Some(index);
            }
        }
        
        // 如果没有健康的提供商，则选择下一个可用的提供商（仅检查速率限制）
        for i in 0..provider_count {
            let index = (start_index + i) % provider_count;
            let provider = &providers[index];
            
            if self.can_request(&provider.name) {
                return Some(index);
            }
        }
        
        // 如果所有提供商都被速率限制，返回None而不是固定索引
        None
    }
}

/// 处理代理请求
pub async fn handle_request(req: Request<Body>, providers: Arc<Vec<Provider>>, state: Arc<ProxyState>) -> Result<Response<Body>, Infallible> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    
    println!("{} {} {} [Load Balancing]", 
        "🔄".cyan(), 
        method.to_string().bright_blue(), 
        uri.to_string().bright_white()
    );
    
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
    
    handle_load_balanced_request(&providers, &state, &method, &uri, &headers, &body_bytes).await
}

/// 使用负载均衡算法处理请求
async fn handle_load_balanced_request(
    providers: &Arc<Vec<Provider>>, 
    state: &Arc<ProxyState>,
    method: &hyper::Method,
    uri: &hyper::Uri,
    headers: &hyper::HeaderMap,
    body_bytes: &hyper::body::Bytes,
) -> Result<Response<Body>, Infallible> {
    let provider_count = providers.len();
    
    if provider_count == 0 {
        println!("{} 没有可用的提供商", "💥".red());
        return Ok(Response::builder()
            .status(503)
            .header("Retry-After", "60")
            .body(Body::from("No providers configured"))
            .unwrap());
    }
    
    // 检查是否需要紧急恢复
    if state.all_providers_down(&providers) {
        println!("{} 所有供应商都已下线，启动紧急恢复...", "🚨".bright_red());
        state.emergency_recovery_all(&providers);
    }
    
    // 快速失败检查：如果所有供应商都不健康且连续失败超过阈值
    let all_unhealthy = state.all_providers_unhealthy(&providers);
    if all_unhealthy {
        println!("{} 所有供应商都不健康，尝试有限重试...", "⚠️".yellow());
        // 在紧急模式下只尝试1轮，每个供应商最多1次重试
        return try_emergency_mode(&providers, &state, method, uri, headers, body_bytes).await;
    }
    
    // 正常模式：尝试最多2轮，每轮选择不同的提供商
    for round in 0..2 {
        if let Some(provider_index) = state.select_next_provider(&providers) {
            let provider = &providers[provider_index];
            let health_score = state.get_provider_health_score(&provider.name);
            
            println!("{} Round {}: {} (健康度: {}%)", 
                "🎯".cyan(), 
                round + 1,
                provider.name.bright_green(),
                health_score.to_string().bright_yellow()
            );
            
            // 根据健康度决定重试次数
            let max_retries = if health_score > 70 { 2 } else { 1 };
            
            for retry in 0..max_retries {
                if retry > 0 {
                    println!("{} 重试 {}/{}: {}", 
                        "🔄".yellow(), 
                        retry + 1, 
                        max_retries,
                        provider.name.bright_white()
                    );
                    sleep(Duration::from_millis(300)).await; // 减少重试间隔
                }
                
                match try_provider(&provider, &method, &uri, &headers, &body_bytes, state).await {
                    Ok(response) => {
                        let status = response.status();
                        if status.is_success() {
                            state.record_provider_success(&provider.name);
                            let new_health = state.get_provider_health_score(&provider.name);
                            println!("{} 成功: {} → {}% 健康度", 
                                "✅".green(), 
                                provider.name.bright_green(),
                                new_health.to_string().bright_green()
                            );
                            return Ok(response);
                        } else {
                            state.record_provider_failure(&provider.name);
                            let new_health = state.get_provider_health_score(&provider.name);
                            println!("{} 失败: {} [{}] → {}% 健康度", 
                                "❌".red(), 
                                provider.name.bright_white(),
                                status.to_string().bright_red(),
                                new_health.to_string().bright_red()
                            );
                            // 如果是客户端错误（4xx），不要重试
                            if status.is_client_error() {
                                return Ok(response);
                            }
                        }
                    }
                    Err(_e) => {
                        state.record_provider_failure(&provider.name);
                        let new_health = state.get_provider_health_score(&provider.name);
                        println!("{} 网络错误: {} → {}% 健康度", 
                            "❌".red(), 
                            provider.name.bright_white(),
                            new_health.to_string().bright_red()
                        );
                    }
                }
            }
            
            if round < 1 {
                println!("{} 故障转移到下一个供应商...", "🔄".yellow());
            }
        } else {
            println!("{} 轮询选择失败，尝试随机策略...", "⚠️".yellow());
            break;
        }
    }
    
    // 如果轮询失败，尝试随机策略作为最后手段
    println!("{} 启用随机选择策略，避免并发冲突...", "🎲".magenta());
    for attempt in 0..3 {
        if let Some(provider_index) = state.select_provider_randomly(&providers) {
            let provider = &providers[provider_index];
            let health_score = state.get_provider_health_score(&provider.name);
            
            println!("{} 随机尝试 {}/3: {} (健康度: {}%)", 
                "🎲".magenta(), 
                attempt + 1,
                provider.name.bright_cyan(),
                health_score.to_string().bright_yellow()
            );
            
            match try_provider(&provider, &method, &uri, &headers, &body_bytes, state).await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        state.record_provider_success(&provider.name);
                        let new_health = state.get_provider_health_score(&provider.name);
                        println!("{} 随机策略成功: {} → {}% 健康度", 
                            "🎉".green(), 
                            provider.name.bright_green(),
                            new_health.to_string().bright_green()
                        );
                        return Ok(response);
                    } else {
                        state.record_provider_failure(&provider.name);
                        // 如果是客户端错误，直接返回
                        if status.is_client_error() {
                            return Ok(response);
                        }
                    }
                }
                Err(_) => {
                    state.record_provider_failure(&provider.name);
                }
            }
        } else {
            println!("{} 所有供应商都被速率限制", "⚠️".yellow());
            break;
        }
        
        // 随机策略间的短暂延迟
        if attempt < 2 {
            sleep(Duration::from_millis(200)).await;
        }
    }
    
    // 打印最终的健康状态汇总
    state.print_providers_health_summary(&providers);
    
    // 负载均衡失败
    println!("{} 负载均衡失败 - 所有供应商都不可用", "💥".red());
    Ok(Response::builder()
        .status(503)
        .header("Retry-After", "30")
        .body(Body::from("Service temporarily unavailable - all providers failed"))
        .unwrap())
}

/// 紧急模式处理：所有供应商都不健康时
async fn try_emergency_mode(
    providers: &Arc<Vec<Provider>>, 
    state: &Arc<ProxyState>,
    method: &hyper::Method,
    uri: &hyper::Uri,
    headers: &hyper::HeaderMap,
    body_bytes: &hyper::body::Bytes,
) -> Result<Response<Body>, Infallible> {
    println!("{} 紧急模式启动 - 快速检测所有供应商", "🚨".bright_red());
    
    // 在紧急模式下，给每个供应商一次机会
    for (_index, provider) in providers.iter().enumerate() {
        println!("{} 紧急测试: {}", 
            "⚡".yellow(), 
            provider.name.bright_yellow()
        );
        
        match try_provider(&provider, &method, &uri, &headers, &body_bytes, state).await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    state.record_provider_success(&provider.name);
                    let new_health = state.get_provider_health_score(&provider.name);
                    println!("{} 紧急恢复成功! {} → {}% 健康度", 
                        "🎉".green(), 
                        provider.name.bright_green(),
                        new_health.to_string().bright_green()
                    );
                    return Ok(response);
                } else {
                    state.record_provider_failure(&provider.name);
                    // 如果是客户端错误，直接返回
                    if status.is_client_error() {
                        return Ok(response);
                    }
                }
            }
            Err(_) => {
                state.record_provider_failure(&provider.name);
            }
        }
    }
    
    // 打印紧急模式后的健康状态汇总
    state.print_providers_health_summary(&providers);
    
    // 紧急模式也失败了
    println!("{} 紧急模式失败 - 所有供应商都无法响应", "💥".bright_red());
    Ok(Response::builder()
        .status(503)
        .header("Retry-After", "120") // 建议2分钟后重试
        .body(Body::from("Service unavailable - all providers are down. Please try again in 2 minutes."))
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
        println!("{} 速率限制: {} ({}/{})", 
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
    let health_score = state.get_provider_health_score(&provider.name);
    println!("{} {} | 速率:{}/{} | 健康度:{}%", 
        "🔑".cyan(), 
        masked_token.bright_yellow(),
        current_requests.to_string().bright_cyan(),
        rate_limit.to_string().bright_white(),
        health_score.to_string().bright_green()
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
        println!("{} HTTP错误: {} [{}]", 
            "❌".red(), 
            provider.name.bright_white(),
            status.as_u16().to_string().bright_red()
        );
    }
    
    Ok(response)
}