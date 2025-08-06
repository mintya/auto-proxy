//! 网络状态检测模块

use std::time::Duration;
use reqwest;

#[derive(Debug, Clone)]
pub struct NetworkStatus {
    pub is_online: bool,
    pub latency_ms: Option<u64>,
    pub dns_working: bool,
    pub external_ip: Option<String>,
    pub error_message: Option<String>,
}

impl NetworkStatus {
    pub fn new() -> Self {
        Self {
            is_online: false,
            latency_ms: None,
            dns_working: false,
            external_ip: None,
            error_message: None,
        }
    }

    /// 检测网络状态
    pub async fn detect() -> Self {
        let mut status = NetworkStatus::new();
        
        // 1. 检测DNS解析
        let _dns_start = std::time::Instant::now();
        match tokio::net::lookup_host("8.8.8.8:53").await {
            Ok(_) => {
                status.dns_working = true;
            }
            Err(e) => {
                status.error_message = Some(format!("DNS解析失败: {}", e));
                return status;
            }
        }

        // 2. 检测网络连通性和延迟 - 减少超时时间加快检测
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(3))  // 从5秒减少到3秒
            .build() {
                Ok(client) => client,
                Err(e) => {
                    status.error_message = Some(format!("HTTP客户端创建失败: {}", e));
                    return status;
                }
            };

        let connectivity_start = std::time::Instant::now();
        
        // 尝试并发连接多个知名网站来测试连通性 - 提高检测速度
        let test_urls = [
            "https://httpbin.org/ip",
            "https://api.ipify.org?format=json", 
            "https://ifconfig.me/ip",
        ];

        // 使用并发请求而不是串行请求
        let mut tasks = Vec::new();
        for url in test_urls.iter() {
            let client_clone = client.clone();
            let url_str = url.to_string();
            tasks.push(tokio::spawn(async move {
                client_clone.get(&url_str).send().await
            }));
        }

        // 等待第一个成功的响应
        for task in tasks {
            if let Ok(Ok(response)) = task.await {
                let latency = connectivity_start.elapsed().as_millis() as u64;
                status.latency_ms = Some(latency);
                status.is_online = true;

                // 尝试获取外部IP
                if let Ok(text) = response.text().await {
                    status.external_ip = Self::extract_ip_from_response(&text);
                }
                break;
            }
        }

        // 3. 如果上面都失败，尝试简单的TCP连接测试
        if !status.is_online {
            if let Ok(_) = tokio::net::TcpStream::connect("8.8.8.8:53").await {
                status.is_online = true;
                status.latency_ms = Some(connectivity_start.elapsed().as_millis() as u64);
            }
        }

        status
    }

    /// 从响应中提取IP地址
    fn extract_ip_from_response(text: &str) -> Option<String> {
        // 尝试JSON格式
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
            if let Some(ip) = json.get("ip").and_then(|v| v.as_str()) {
                return Some(ip.to_string());
            }
            if let Some(ip) = json.get("origin").and_then(|v| v.as_str()) {
                return Some(ip.to_string());
            }
        }

        // 尝试纯文本格式
        let trimmed = text.trim();
        if Self::is_valid_ip(trimmed) {
            Some(trimmed.to_string())
        } else {
            None
        }
    }

    /// 简单的IP地址验证
    fn is_valid_ip(s: &str) -> bool {
        s.parse::<std::net::IpAddr>().is_ok()
    }

    /// 获取网络状态描述
    pub fn status_text(&self) -> String {
        if !self.is_online {
            return "离线".to_string();
        }

        let mut parts = vec!["在线".to_string()];
        
        if let Some(latency) = self.latency_ms {
            parts.push(format!("延迟{}ms", latency));
        }

        if self.dns_working {
            parts.push("DNS正常".to_string());
        }

        parts.join(" | ")
    }

    /// 获取状态图标
    pub fn status_icon(&self) -> &'static str {
        if !self.is_online {
            "🔴"
        } else if let Some(latency) = self.latency_ms {
            if latency < 100 {
                "🟢" // 延迟低
            } else if latency < 300 {
                "🟡" // 延迟中等
            } else {
                "🟠" // 延迟高
            }
        } else {
            "🟢"
        }
    }
}