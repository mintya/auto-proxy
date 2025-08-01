//! 提供商相关的数据结构和功能

use serde::{Deserialize, Serialize};

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