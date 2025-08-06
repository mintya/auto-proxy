//! 提供商相关的数据结构和功能

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
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
            .unwrap_or(std::time::Duration::from_secs(0))
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
            .unwrap_or(std::time::Duration::from_secs(0))
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
            .unwrap_or(std::time::Duration::from_secs(0))
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

/// 供应商健康度追踪器
#[derive(Debug)]
pub struct ProviderHealth {
    /// 健康度分数 (0-100)
    health_score: AtomicU8,
    /// 连续失败次数
    consecutive_failures: AtomicU8,
    /// 连续成功次数
    consecutive_successes: AtomicU8,
    /// 最后更新时间
    last_updated: AtomicU64,
}

impl ProviderHealth {
    pub fn new() -> Self {
        Self {
            health_score: AtomicU8::new(100), // 初始健康度100%
            consecutive_failures: AtomicU8::new(0),
            consecutive_successes: AtomicU8::new(0),
            last_updated: AtomicU64::new(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        }
    }
    
    /// 记录成功请求
    pub fn record_success(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_secs();
            
        self.last_updated.store(now, Ordering::Relaxed);
        let previous_failures = self.consecutive_failures.load(Ordering::Relaxed);
        self.consecutive_failures.store(0, Ordering::Relaxed);
        let successes = self.consecutive_successes.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        
        // 成功时的恢复速度：之前失败越多，恢复越快
        let current_health = self.health_score.load(Ordering::Relaxed);
        if current_health < 100 {
            let recovery = if previous_failures > 0 {
                // 从失败中恢复：根据之前失败次数调整恢复速度
                match previous_failures {
                    1..=2 => 10,   // 轻微失败后快速恢复
                    3..=4 => 15,   // 中度失败后中等恢复
                    5..=10 => 25,  // 严重失败后大幅恢复
                    _ => 35,       // 极度失败后极速恢复
                }
            } else {
                // 正常连续成功：渐进恢复
                std::cmp::min(successes * 3, 15) // 每次成功最多恢复15分
            };
            
            let recovery = std::cmp::min(recovery, 100 - current_health);
            self.health_score.store(current_health + recovery, Ordering::Relaxed);
        }
    }
    
    /// 记录失败请求
    pub fn record_failure(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_secs();
            
        self.last_updated.store(now, Ordering::Relaxed);
        self.consecutive_successes.store(0, Ordering::Relaxed);
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        
        // 指数级健康度下降：从慢到快
        let current_health = self.health_score.load(Ordering::Relaxed);
        let penalty = match failures {
            1 => 5,   // 第1次失败：轻微惩罚
            2 => 10,  // 第2次失败：开始加重
            3 => 20,  // 第3次失败：显著下降
            4 => 35,  // 第4次失败：大幅下降
            5..=10 => 50, // 第5-10次：严重惩罚
            _ => current_health, // 超过10次：直接降到0
        };
        
        let new_health = current_health.saturating_sub(penalty);
        self.health_score.store(new_health, Ordering::Relaxed);
    }
    
    /// 获取当前健康度分数
    pub fn get_health_score(&self) -> u8 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_secs();
        let last_updated = self.last_updated.load(Ordering::Relaxed);
        
        // 如果超过5分钟没有更新，逐步恢复健康度
        if now.saturating_sub(last_updated) > 300 { // 5分钟
            let current_health = self.health_score.load(Ordering::Relaxed);
            if current_health < 100 {
                let recovery = std::cmp::min(5, 100 - current_health); // 每5分钟恢复5分
                self.health_score.store(current_health + recovery, Ordering::Relaxed);
                self.last_updated.store(now, Ordering::Relaxed);
            }
        }
        
        self.health_score.load(Ordering::Relaxed)
    }
    
    /// 检查是否健康（健康度 > 20）
    pub fn is_healthy(&self) -> bool {
        self.get_health_score() > 20
    }
    
    /// 检查是否完全不可用（健康度 = 0）
    pub fn is_completely_down(&self) -> bool {
        self.get_health_score() == 0
    }
    
    /// 强制进行健康恢复尝试（紧急模式）
    pub fn emergency_recovery(&self) {
        let current_health = self.health_score.load(Ordering::Relaxed);
        if current_health == 0 {
            // 给予最小健康度以允许重试
            self.health_score.store(10, Ordering::Relaxed);
            self.consecutive_failures.store(0, Ordering::Relaxed);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            self.last_updated.store(now, Ordering::Relaxed);
        }
    }
    
    /// 获取连续失败次数
    pub fn get_consecutive_failures(&self) -> u8 {
        self.consecutive_failures.load(Ordering::Relaxed)
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
    
}