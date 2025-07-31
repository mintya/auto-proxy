//! Auto Proxy - 智能代理服务器
//! 
//! 这是一个支持多提供商的智能代理服务器，具有自动重试和故障转移功能。

pub mod config;
pub mod proxy;
pub mod provider;

pub use config::*;
pub use proxy::*;
pub use provider::*;