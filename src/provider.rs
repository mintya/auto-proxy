//! 提供商相关的数据结构和功能

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 速率限制器
#[derive(Debug)]
pub struct RateLimiter {
    /// 请求时间戳队列（动态大小）
    timestamps: Vec<AtomicU64>,
    /// 当前索引
    current_index: std::sync::atomic::AtomicUsize,
    /// 请求计数
    count: std::sync::atomic::AtomicUsize,
    /// 速率限制值
    limit: usize,
}

impl RateLimiter {
    pub fn new(limit: usize) -> Self {
        // 创建指定大小的原子时间戳数组
        let timestamps: Vec<AtomicU64> = (0..limit).map(|_| AtomicU64::new(0)).collect();
        
        Self {
            timestamps,
            current_index: std::sync::atomic::AtomicUsize::new(0),
            count: std::sync::atomic::AtomicUsize::new(0),
            limit,
        }
    }
    
    /// 检查是否可以发起请求（每分钟最多limit次）
    pub fn can_request(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let current_count = self.count.load(Ordering::Relaxed);
        
        // 如果还没有达到limit次请求，直接允许
        if current_count < self.limit {
            return true;
        }
        
        // 检查最早的请求时间戳是否超过60秒
        let oldest_index = (self.current_index.load(Ordering::Relaxed) + self.limit - (self.limit - 1)) % self.limit;
        let oldest_timestamp = self.timestamps[oldest_index].load(Ordering::Relaxed);
        
        // 如果最早的请求时间超过60秒，则允许新请求
        now.saturating_sub(oldest_timestamp) >= 60
    }
    
    /// 记录一次请求
    pub fn record_request(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let current_count = self.count.load(Ordering::Relaxed);
        
        if current_count < self.limit {
            // 还没有填满队列，直接添加
            let index = self.current_index.fetch_add(1, Ordering::Relaxed);
            self.timestamps[index].store(now, Ordering::Relaxed);
            self.count.fetch_add(1, Ordering::Relaxed);
        } else {
            // 队列已满，覆盖最旧的记录
            let index = self.current_index.fetch_add(1, Ordering::Relaxed) % self.limit;
            self.timestamps[index].store(now, Ordering::Relaxed);
        }
    }
    
    /// 获取当前窗口内的请求数量
    pub fn current_requests(&self) -> usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let current_count = self.count.load(Ordering::Relaxed);
        if current_count < self.limit {
            return current_count;
        }
        
        // 计算60秒内的请求数量
        let mut count = 0;
        for i in 0..self.limit {
            let timestamp = self.timestamps[i].load(Ordering::Relaxed);
            if timestamp > 0 && now.saturating_sub(timestamp) < 60 {
                count += 1;
            }
        }
        count
    }
    
    /// 获取速率限制值
    pub fn limit(&self) -> usize {
        self.limit
    }
}

/// 代理提供商配置
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Provider {
    /// 提供商名称
    pub name: String,
    /// 认证令牌
    pub token: String,
    /// 基础URL
    pub base_url: String,
    /// 密钥类型
    pub key_type: String,
    /// 是否为优先服务商
    #[serde(default)]
    pub preferred: bool,
}

impl Provider {
    /// 获取屏蔽后的token用于日志显示
    pub fn masked_token(&self) -> String {
        if self.token.len() > 8 {
            format!("{}****{}", &self.token[..4], &self.token[self.token.len()-4..])
        } else {
            "****".to_string()
        }
    }
    
    /// 设置为优先服务商
    pub fn set_preferred(&mut self, preferred: bool) {
        self.preferred = preferred;
    }
    
    /// 检查是否为优先服务商
    pub fn is_preferred(&self) -> bool {
        self.preferred
    }
}