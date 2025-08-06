//! 代理请求处理功能

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::convert::Infallible;
use hyper::{Body, Client, Request, Response};
use hyper_rustls::HttpsConnectorBuilder;
use http::header::{HeaderValue, AUTHORIZATION, HOST};
use colored::*;
use crate::provider::{Provider, RateLimiter, ProviderHealth};
use crate::token::{TokenCalculator, calculate_display_width};
use crate::interactive::InteractiveProviderManager;
use std::collections::HashMap;

/// 代理状态管理
pub struct ProxyState {
    /// 轮询计数器
    pub round_robin_counter: AtomicUsize,
    /// 每个提供商的速率限制器
    pub rate_limiters: std::sync::Mutex<HashMap<String, RateLimiter>>,
    /// 每个提供商的健康度追踪器
    pub provider_health: std::sync::Mutex<HashMap<String, ProviderHealth>>,
    /// 每个提供商的最后响应状态码
    pub last_status_codes: std::sync::Mutex<HashMap<String, u16>>,
    /// 每个提供商的成功Token使用量统计
    pub token_usage: std::sync::Mutex<HashMap<String, u64>>,
    /// 全局速率限制值
    pub rate_limit: usize,
    /// 交互式管理器
    pub interactive_manager: Arc<InteractiveProviderManager>,
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
            last_status_codes: std::sync::Mutex::new(HashMap::new()),
            token_usage: std::sync::Mutex::new(HashMap::new()),
            rate_limit,
            interactive_manager: Arc::new(InteractiveProviderManager::new()),
        }
    }

    /// 安全获取mutex锁，处理中毒情况
    fn safe_mutex_lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<T> {
        match mutex.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("⚠️ Mutex poisoned, recovering...");
                poisoned.into_inner()
            }
        }
    }

    /// 获取速率限制值
    pub fn get_rate_limit(&self) -> usize {
        self.rate_limit
    }
    
    /// 检查提供商是否可以发起请求（速率限制）
    pub fn can_request(&self, provider_name: &str) -> bool {
        let mut limiters = Self::safe_mutex_lock(&self.rate_limiters);
        let limiter = limiters.entry(provider_name.to_string())
            .or_insert_with(|| RateLimiter::new(self.rate_limit));
        limiter.can_request()
    }
    
    /// 记录一次请求到指定提供商
    pub fn record_request(&self, provider_name: &str) {
        let mut limiters = Self::safe_mutex_lock(&self.rate_limiters);
        let limiter = limiters.entry(provider_name.to_string())
            .or_insert_with(|| RateLimiter::new(self.rate_limit));
        limiter.record_request();
    }
    
    /// 获取提供商当前请求数量
    pub fn get_current_requests(&self, provider_name: &str) -> usize {
        let mut limiters = Self::safe_mutex_lock(&self.rate_limiters);
        let limiter = limiters.entry(provider_name.to_string())
            .or_insert_with(|| RateLimiter::new(self.rate_limit));
        limiter.current_requests()
    }
    
    /// 记录提供商成功请求
    pub fn record_provider_success(&self, provider_name: &str) {
        let mut health_map = Self::safe_mutex_lock(&self.provider_health);
        let health = health_map.entry(provider_name.to_string())
            .or_insert_with(|| ProviderHealth::new());
        health.record_success();
    }
    
    /// 记录提供商失败请求
    pub fn record_provider_failure(&self, provider_name: &str) {
        let mut health_map = Self::safe_mutex_lock(&self.provider_health);
        let health = health_map.entry(provider_name.to_string())
            .or_insert_with(|| ProviderHealth::new());
        health.record_failure();
    }

    /// 记录提供商响应状态码
    pub fn record_status_code(&self, provider_name: &str, status_code: u16) {
        let mut status_codes = Self::safe_mutex_lock(&self.last_status_codes);
        status_codes.insert(provider_name.to_string(), status_code);
    }

    /// 获取提供商最后状态码
    pub fn get_last_status_code(&self, provider_name: &str) -> Option<u16> {
        let status_codes = Self::safe_mutex_lock(&self.last_status_codes);
        status_codes.get(provider_name).copied()
    }
    
    /// 记录成功Token使用量（估算值）
    pub fn record_token_usage(&self, provider_name: &str, tokens: u64) {
        let mut usage_map = Self::safe_mutex_lock(&self.token_usage);
        let current_usage = usage_map.entry(provider_name.to_string()).or_insert(0);
        *current_usage += tokens;
    }
    
    /// 获取提供商Token使用量
    pub fn get_token_usage(&self, provider_name: &str) -> u64 {
        let usage_map = Self::safe_mutex_lock(&self.token_usage);
        usage_map.get(provider_name).copied().unwrap_or(0)
    }
    
    /// 获取所有提供商的Token使用量总和
    pub fn get_total_token_usage(&self) -> u64 {
        let usage_map = Self::safe_mutex_lock(&self.token_usage);
        usage_map.values().sum()
    }
    
    /// 获取提供商Token使用百分比
    pub fn get_provider_usage_percentage(&self, provider_name: &str) -> f32 {
        let total = self.get_total_token_usage();
        if total == 0 {
            return 0.0;
        }
        let provider_usage = self.get_token_usage(provider_name);
        (provider_usage as f32 / total as f32) * 100.0
    }
    
    /// 获取提供商健康度分数
    pub fn get_provider_health_score(&self, provider_name: &str) -> u8 {
        let mut health_map = Self::safe_mutex_lock(&self.provider_health);
        let health = health_map.entry(provider_name.to_string())
            .or_insert_with(|| ProviderHealth::new());
        health.get_health_score()
    }
    
    /// 检查提供商是否健康
    pub fn is_provider_healthy(&self, provider_name: &str) -> bool {
        let mut health_map = Self::safe_mutex_lock(&self.provider_health);
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

    /// 检查所有供应商是否都被禁用
    pub fn all_providers_disabled(&self, providers: &[Provider]) -> bool {
        if providers.is_empty() {
            return true;
        }
        
        for provider in providers {
            if !self.interactive_manager.is_provider_disabled(&provider.name) {
                return false;
            }
        }
        true
    }
    
    /// 紧急恢复所有供应商
    pub fn emergency_recovery_all(&self, providers: &[Provider]) {
        let mut health_map = Self::safe_mutex_lock(&self.provider_health);
        for provider in providers {
            let health = health_map.entry(provider.name.clone())
                .or_insert_with(|| ProviderHealth::new());
            health.emergency_recovery();
        }
    }
    
    /// 打印所有提供商的健康状态汇总
    pub fn print_providers_health_summary(&self, providers: &[Provider]) {
        println!();
        println!("{}", "📊 提供商健康状态汇总".bright_cyan().bold());
        println!("{}", "═".repeat(70).bright_black());
        println!("{}  {} {:<15} {:<4} {:<4}   {:<8} {:<4}   {}", 
            "状态".bright_white().bold(),
            "序号".bright_white().bold(),
            "名称".bright_white().bold(),
            "健康".bright_white().bold(),
            "健康度".bright_white().bold(),
            "速率限制".bright_white().bold(),
            "状态".bright_white().bold(),
            "可用性".bright_white().bold()
        );
        println!("{}", "─".repeat(70).bright_black());
        
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
            
            // 计算各字段的显示宽度（考虑中文字符）
            let name_display_width = calculate_display_width(&provider.name);
            let name_padding = if name_display_width < 15 { 15 - name_display_width } else { 1 };
            
            let health_text = if health_score > 20 { "健康" } else { "异常" };
            let status_text = if is_healthy { "可用" } else { "不可用" };
            
            println!("{} {:<2} {}{} {:<4} {:<4}% │ 速率: {:<2}/{:<2} {} │ {}", 
                status_icon,
                index + 1,
                provider.name.bright_cyan(),
                " ".repeat(name_padding),
                if health_score > 20 { health_text.bright_green() } else { health_text.bright_red() },
                health_score.to_string().color(health_color).bold(),
                current_requests.to_string().bright_cyan(),
                self.rate_limit.to_string().bright_white(),
                rate_status,
                if is_healthy { status_text.bright_green() } else { status_text.bright_red() }
            );
        }
        
        println!("{}", "═".repeat(70).bright_black());
        let avg_health = if providers.is_empty() { 0 } else { total_health / providers.len() as u32 };
        println!("{} 健康供应商: {:<2}/{:<2} │ 平均健康度: {:<3}% │ 系统状态: {}", 
            "🏥".cyan(),
            healthy_count.to_string().bright_green(),
            providers.len().to_string().bright_white(),
            avg_health.to_string().bright_yellow(),
            if healthy_count > 0 { "正常".bright_green() } else { "警告".bright_red() }
        );
        println!("{}", "═".repeat(70).bright_black());
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
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(std::time::Duration::from_secs(0)).as_nanos().hash(&mut hasher);
            std::thread::current().id().hash(&mut hasher);
            (hasher.finish() as usize) % provider_count
        } else {
            self.round_robin_counter.fetch_add(1, Ordering::Relaxed) % provider_count
        };
        
        // 从当前索引开始轮询查找健康的提供商
        for i in 0..provider_count {
            let index = (start_index + i) % provider_count;
            let provider = &providers[index];
            
            // 检查是否被禁用
            if self.interactive_manager.is_provider_disabled(&provider.name) {
                continue;
            }
            
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
    handle_request_with_logger(req, providers, state, None).await
}

/// 带日志记录器的请求处理器
pub async fn handle_request_with_logger(
    req: Request<Body>, 
    providers: Arc<Vec<Provider>>, 
    state: Arc<ProxyState>,
    logger: Option<Arc<crate::ui::Logger>>
) -> Result<Response<Body>, Infallible> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    
    let body_bytes = match hyper::body::to_bytes(req.into_body()).await {
        Ok(bytes) => bytes,
        Err(_e) => {
            return Ok(Response::builder()
                .status(400)
                .body(Body::from("Bad Request"))
                .unwrap_or_else(|_| Response::new(Body::from("Internal Error"))));
        }
    };
    
    handle_load_balanced_request(&providers, &state, &method, &uri, &headers, &body_bytes, logger).await
}

/// 使用负载均衡算法处理请求
async fn handle_load_balanced_request(
    providers: &Arc<Vec<Provider>>, 
    state: &Arc<ProxyState>,
    method: &hyper::Method,
    uri: &hyper::Uri,
    headers: &hyper::HeaderMap,
    body_bytes: &hyper::body::Bytes,
    logger: Option<Arc<crate::ui::Logger>>,
) -> Result<Response<Body>, Infallible> {
    let provider_count = providers.len();
    
    if provider_count == 0 {
        return Ok(Response::builder()
            .status(503)
            .header("Retry-After", "60")
            .body(Body::from("No providers configured"))
            .unwrap_or_else(|_| Response::new(Body::from("Service Unavailable"))));
    }

    // 检查所有提供商是否被禁用
    if state.all_providers_disabled(&providers) {
        return Ok(Response::builder()
            .status(503)
            .header("Retry-After", "30")
            .body(Body::from("All providers are disabled by user. Please enable at least one provider."))
            .unwrap_or_else(|_| Response::new(Body::from("Service Unavailable"))));
    }
    
    // 检查是否需要紧急恢复
    if state.all_providers_down(&providers) {
        state.emergency_recovery_all(&providers);
    }
    
    // 快速失败检查：如果所有供应商都不健康且连续失败超过阈值
    let all_unhealthy = state.all_providers_unhealthy(&providers);
    if all_unhealthy {
        // 在紧急模式下只尝试1轮，每个供应商最多1次重试
        return try_emergency_mode(&providers, &state, method, uri, headers, body_bytes, logger).await;
    }
    
    // 优化模式：直接尝试每个提供商，失败立即转移，不重试
    // 先尝试轮询选择健康的提供商
    for _attempt in 0..provider_count {
        if let Some(provider_index) = state.select_next_provider(&providers) {
            let provider = &providers[provider_index];
            
            // 立即记录转发日志
            let forward_msg = format!("🔄 {} {} 转发至 {}", method, uri, provider.name);
            if let Some(ref logger) = logger {
                logger.info(forward_msg);
            } else {
                eprintln!("{}", forward_msg);
            }
            
            match try_provider(&provider, &method, &uri, &headers, &body_bytes, state).await {
                Ok(response) => {
                    let status = response.status();
                    let status_code = status.as_u16();
                    state.record_status_code(&provider.name, status_code);
                    
                    // 记录响应日志
                    if status.is_success() {
                        let success_msg = format!("✅ {} {} → {} [{}]", method, uri, provider.name, status_code);
                        if let Some(ref logger) = logger {
                            logger.success(success_msg);
                        } else {
                            eprintln!("{}", success_msg);
                        }
                        state.record_provider_success(&provider.name);
                        
                        // 估算Token使用量（根据请求的内容长度和基本固定成本）
                        let estimated_tokens = TokenCalculator::estimate_usage(&body_bytes, &uri);
                        state.record_token_usage(&provider.name, estimated_tokens);
                        
                        return Ok(response);
                    } else {
                        state.record_provider_failure(&provider.name);
                        
                        // 使用HTTP状态码标准描述
                        let status_description = status.to_string();
                        let error_msg = format!("❌ {} {} → {} [{}]", method, uri, provider.name, status_description);
                        if let Some(ref logger) = logger {
                            logger.warning(error_msg);
                        } else {
                            eprintln!("{}", error_msg);
                        }
                        
                        // 如果这是最后一个提供商，返回错误响应；否则继续尝试下一个
                        continue; // 立即尝试下一个提供商
                    }
                }
                Err(e) => {
                    state.record_provider_failure(&provider.name);
                    state.record_status_code(&provider.name, 0);
                    let error_msg = format!("❌ {} {} → {} [网络错误: {}]", method, uri, provider.name, e);
                    if let Some(ref logger) = logger {
                        logger.error(error_msg);
                    } else {
                        eprintln!("{}", error_msg);
                    }
                    continue; // 立即尝试下一个提供商
                }
            }
        } else {
            // 没有更多健康的提供商可用
            break;
        }
    }
    
    // 负载均衡失败
    Ok(Response::builder()
        .status(503)
        .header("Retry-After", "30")
        .body(Body::from("Service temporarily unavailable - all providers failed"))
        .unwrap_or_else(|_| Response::new(Body::from("Service Unavailable"))))
}

/// 紧急模式处理：所有供应商都不健康时
async fn try_emergency_mode(
    providers: &Arc<Vec<Provider>>, 
    state: &Arc<ProxyState>,
    method: &hyper::Method,
    uri: &hyper::Uri,
    headers: &hyper::HeaderMap,
    body_bytes: &hyper::body::Bytes,
    logger: Option<Arc<crate::ui::Logger>>,
) -> Result<Response<Body>, Infallible> {
    
    // 在紧急模式下，给每个供应商一次机会，但跳过被禁用的供应商
    for (_index, provider) in providers.iter().enumerate() {
        // 检查是否被禁用 - 即使在紧急模式下也要跳过被禁用的供应商
        if state.interactive_manager.is_provider_disabled(&provider.name) {
            continue;
        }
        
        // 立即记录紧急模式转发日志
        let emergency_msg = format!("🚨 紧急模式 {} {} 转发至 {}", method, uri, provider.name);
        if let Some(ref logger) = logger {
            logger.warning(emergency_msg);
        } else {
            eprintln!("{}", emergency_msg);
        }
        
        match try_provider(&provider, &method, &uri, &headers, &body_bytes, state).await {
            Ok(response) => {
                let status = response.status();
                let status_code = status.as_u16();
                state.record_status_code(&provider.name, status_code);
                
                // 记录响应日志
                if status.is_success() {
                    let success_msg = format!("✅ 紧急模式 {} {} → {} [{}]", method, uri, provider.name, status_code);
                    if let Some(ref logger) = logger {
                        logger.success(success_msg);
                    } else {
                        eprintln!("{}", success_msg);
                    }
                    state.record_provider_success(&provider.name);
                    
                    // 估算Token使用量
                    let estimated_tokens = TokenCalculator::estimate_usage(&body_bytes, &uri);
                    state.record_token_usage(&provider.name, estimated_tokens);
                    
                    return Ok(response);
                } else {
                    state.record_provider_failure(&provider.name);
                    
                    // 使用HTTP状态码标准描述
                    let status_description = status.to_string();
                    let error_msg = format!("❌ 紧急模式 {} {} → {} [{}]", method, uri, provider.name, status_description);
                    if let Some(ref logger) = logger {
                        logger.error(error_msg);
                    } else {
                        eprintln!("{}", error_msg);
                    }
                }
            }
            Err(e) => {
                state.record_provider_failure(&provider.name);
                state.record_status_code(&provider.name, 0);
                let error_msg = format!("❌ 紧急模式 {} {} → {} [网络错误: {}]", method, uri, provider.name, e);
                if let Some(ref logger) = logger {
                    logger.error(error_msg);
                } else {
                    eprintln!("{}", error_msg);
                }
            }
        }
    }
    
    // 紧急模式也失败了
    Ok(Response::builder()
        .status(503)
        .header("Retry-After", "120") // 建议2分钟后重试
        .body(Body::from("Service unavailable - all providers are down. Please try again in 2 minutes."))
        .unwrap_or_else(|_| Response::new(Body::from("Emergency mode failed"))))
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
    
    Ok(response)
}
