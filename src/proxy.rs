//! 代理请求处理功能

use std::sync::Arc;
use std::time::Duration;
use std::convert::Infallible;
use hyper::{Body, Client, Request, Response};
use hyper_tls::HttpsConnector;
use http::header::{HeaderValue, AUTHORIZATION, HOST};
use tokio::time::sleep;
use colored::*;
use crate::provider::Provider;

/// 处理代理请求
/// 
/// # 参数
/// * `req` - 原始HTTP请求
/// * `providers` - 提供商列表
/// 
/// # 返回
/// * `Ok(Response<Body>)` - 成功的响应
/// * `Err(Infallible)` - 不可能的错误（用于满足hyper的类型要求）
pub async fn handle_request(req: Request<Body>, providers: Arc<Vec<Provider>>) -> Result<Response<Body>, Infallible> {
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
    
    // 尝试每个提供商
    for (provider_index, provider) in providers.iter().enumerate() {
        println!("{} 尝试提供商: {} ({})", 
            "🎯".yellow(), 
            provider.name.bright_green(), 
            provider.base_url.bright_blue()
        );
        
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
                        println!("{} 请求成功: {}", "✅".green(), status.to_string().bright_green());
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
        
        // 如果不是最后一个提供商，继续尝试下一个
        if provider_index < providers.len() - 1 {
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
    let https = HttpsConnector::new();
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