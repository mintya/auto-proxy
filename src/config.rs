//! 配置文件读取和管理功能

use std::fs;
use std::path::{Path, PathBuf};
use dirs::home_dir;
use colored::*;
use crate::provider::Provider;

/// 读取提供商配置文件
/// 
/// # 参数
/// * `config_path` - 可选的配置文件路径，如果为None则使用默认路径
/// 
/// # 返回
/// * `Ok((Vec<Provider>, PathBuf))` - 成功读取的提供商列表和实际使用的配置文件路径
/// * `Err(String)` - 错误信息
pub fn read_providers_config(config_path: Option<PathBuf>) -> Result<(Vec<Provider>, PathBuf), String> {
    // 确定配置文件路径
    let (config_file, is_custom_path) = match config_path {
        Some(path) => (path, true),
        None => {
            // 默认路径为 ~/.claude-proxy-manager/providers.json
            let mut path = home_dir().unwrap_or_else(|| PathBuf::from("."));
            path.push(".claude-proxy-manager");
            path.push("providers.json");
            (path, false)
        }
    };
    
    println!("{} {}", "📁 读取配置文件:".cyan(), config_file.display().to_string().bright_white());
    
    // 检查文件是否存在
    if !config_file.exists() {
        if is_custom_path {
            // 如果是用户指定的配置文件不存在，则返回错误
            return Err(format!("❌ 指定的配置文件不存在: {}", config_file.display()));
        } else {
            // 如果是默认配置文件不存在，则创建目录和配置文件
            println!("{}", "⚠️  默认配置文件不存在，正在创建初始配置文件...".yellow());
            
            create_default_config(&config_file)?;
            
            println!("{} {}", "✅ 已创建初始配置文件:".green(), config_file.display().to_string().bright_white());
            println!("{}", "📝 请修改配置文件后重新启动程序".yellow().bold());
            std::process::exit(0);
        }
    }
    
    // 读取文件内容
    let content = fs::read_to_string(&config_file).map_err(|e| {
        format!("❌ 无法读取配置文件 {}: {}", config_file.display(), e)
    })?;
    
    // 解析JSON
    let providers: Vec<Provider> = serde_json::from_str(&content).map_err(|e| {
        format!("❌ 配置文件格式错误: {}", e)
    })?;
    
    if providers.is_empty() {
        return Err("❌ 配置文件中没有提供商信息".to_string());
    }
    
    println!("{} {} 个提供商", "✅ 成功加载".green(), providers.len().to_string().bright_white());
    
    Ok((providers, config_file))
}

/// 创建默认配置文件
fn create_default_config(config_file: &Path) -> Result<(), String> {
    // 创建目录
    if let Some(parent) = config_file.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                format!("❌ 无法创建配置目录 {}: {}", parent.display(), e)
            })?;
        }
    }
    
    // 获取示例配置文件路径
    let example_path = PathBuf::from("providers.json.example");
    if !example_path.exists() {
        // 如果示例文件不存在，则创建一个基本的配置
        let default_config = r#"[
  {
    "name": "your_name",
    "token": "sk-your_sk",
    "base_url": "https://your_base_url",
    "key_type": "AUTH_TOKEN"
  }
]"#;
        
        fs::write(config_file, default_config).map_err(|e| {
            format!("❌ 无法创建配置文件 {}: {}", config_file.display(), e)
        })?;
    } else {
        // 复制示例配置文件到目标位置
        let example_content = fs::read_to_string(&example_path).map_err(|e| {
            format!("❌ 无法读取示例配置文件 {}: {}", example_path.display(), e)
        })?;
        
        fs::write(config_file, example_content).map_err(|e| {
            format!("❌ 无法创建配置文件 {}: {}", config_file.display(), e)
        })?;
    }
    
    Ok(())
}