use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crossterm::{
    event::{self, Event, KeyCode, MouseEventKind, MouseButton},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
    cursor::{MoveTo, Show, Hide},
    style::Print,
};
use std::io::{stdout, Write};
use crate::provider::Provider;
use crate::proxy::ProxyState;
use crate::token::calculate_display_width;
use colored::*;

/// 交互式服务商管理界面
pub struct InteractiveProviderManager {
    pub disabled_providers: Arc<Mutex<HashMap<String, bool>>>,
    pub provider_rows: Arc<Mutex<Vec<ProviderRow>>>,
}

#[derive(Clone)]
pub struct ProviderRow {
    pub index: usize,
    pub provider_name: String,
    pub y_position: u16,
    pub toggle_button_x: u16,
    pub toggle_button_width: u16,
}

impl InteractiveProviderManager {
    pub fn new() -> Self {
        Self {
            disabled_providers: Arc::new(Mutex::new(HashMap::new())),
            provider_rows: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 检查服务商是否被禁用
    pub fn is_provider_disabled(&self, provider_name: &str) -> bool {
        // 使用 try_lock 避免死锁
        match self.disabled_providers.try_lock() {
            Ok(disabled) => {
                disabled.get(provider_name).unwrap_or(&false).clone()
            },
            Err(_) => {
                // 如果无法获取锁，默认为未禁用
                false
            }
        }
    }

    /// 切换服务商启用/禁用状态
    pub fn toggle_provider(&self, provider_name: &str) -> bool {
        // 使用 try_lock 避免死锁，如果无法获取锁则返回当前状态
        match self.disabled_providers.try_lock() {
            Ok(mut disabled) => {
                let current_state = disabled.get(provider_name).unwrap_or(&false).clone();
                let new_state = !current_state;
                disabled.insert(provider_name.to_string(), new_state);
                new_state
            },
            Err(_) => {
                // 如果无法获取锁，返回当前状态（通过另一个读取方法）
                self.is_provider_disabled(provider_name)
            }
        }
    }

    /// 显示交互式服务商状态列表
    pub fn show_interactive_status(&self, providers: &[Provider], state: &ProxyState) -> std::io::Result<()> {
        enable_raw_mode()?;
        execute!(stdout(), Hide, Clear(ClearType::All), MoveTo(0, 0))?;

        // 创建一个本地的 provider_rows 变量
        let mut local_provider_rows = Vec::new();
        
        let mut current_y = 3;

        // 显示标题
        execute!(stdout(), MoveTo(0, 0))?;
        println!("{}", "📊 交互式服务商管理 (ESC退出, 点击切换启用状态)".bright_cyan().bold());
        println!("{}", "═".repeat(80).bright_black());

        // 表头
        println!("{} {} {:<15} {:<4} {:<4} {:<8} {:<6} {:<6}", 
            "状态".bright_white().bold(),
            "序号".bright_white().bold(),
            "名称".bright_white().bold(),
            "健康".bright_white().bold(),
            "健康度".bright_white().bold(),
            "速率限制".bright_white().bold(),
            "状态".bright_white().bold(),
            "启用".bright_white().bold()
        );
        println!("{}", "─".repeat(80).bright_black());

        // 显示每个服务商
        for (index, provider) in providers.iter().enumerate() {
            let health_score = state.get_provider_health_score(&provider.name);
            let is_healthy = state.is_provider_healthy(&provider.name);
            let current_requests = state.get_current_requests(&provider.name);
            let can_request = state.can_request(&provider.name);
            let is_disabled = self.is_provider_disabled(&provider.name);

            let (status_icon, health_color) = match health_score {
                90..=100 => ("🟢", "bright_green"),
                70..=89 => ("🟡", "bright_yellow"), 
                40..=69 => ("🟠", "yellow"),
                20..=39 => ("🔴", "bright_red"),
                _ => ("💀", "red"),
            };

            let name_display_width = calculate_display_width(&provider.name);
            let name_padding = if name_display_width < 15 { 15 - name_display_width } else { 1 };
            
            let health_text = if health_score > 20 { "健康" } else { "异常" };
            let status_text = if is_healthy { "可用" } else { "不可用" };
            let rate_status = if can_request { "✅" } else { "🚫" };
            
            // 启用/禁用按钮
            let toggle_button = if is_disabled { 
                "[❌禁用]".bright_red()
            } else { 
                "[✅启用]".bright_green()
            };
            
            let toggle_button_x = 65; // 按钮的X位置

            execute!(stdout(), MoveTo(0, current_y))?;

            if is_disabled {
                // 禁用的服务商显示为灰色
                print!("{} {:<2} {}{} {:<4} {:<4}% │ 速率: {:<2}/{:<2} {} │ {:<6} │ {}", 
                    status_icon.bright_black(),
                    index + 1,
                    provider.name.bright_black(),
                    " ".repeat(name_padding),
                    health_text.bright_black(),
                    health_score.to_string().bright_black(),
                    current_requests.to_string().bright_black(),
                    state.get_rate_limit().to_string().bright_black(),
                    rate_status.bright_black(),
                    status_text.bright_black(),
                    toggle_button
                );
            } else {
                print!("{} {:<2} {}{} {:<4} {:<4}% │ 速率: {:<2}/{:<2} {} │ {:<6} │ {}", 
                    status_icon,
                    index + 1,
                    provider.name.bright_cyan(),
                    " ".repeat(name_padding),
                    if health_score > 20 { health_text.bright_green() } else { health_text.bright_red() },
                    health_score.to_string().color(health_color).bold(),
                    current_requests.to_string().bright_cyan(),
                    state.get_rate_limit().to_string().bright_white(),
                    rate_status,
                    if is_healthy { status_text.bright_green() } else { status_text.bright_red() },
                    toggle_button
                );
            }

            stdout().flush()?;

            local_provider_rows.push(ProviderRow {
                index,
                provider_name: provider.name.clone(),
                y_position: current_y,
                toggle_button_x,
                toggle_button_width: 8,
            });

            current_y += 1;
        }
        
        // 将本地的 provider_rows 保存到 self.provider_rows 中
        if let Ok(mut rows) = self.provider_rows.try_lock() {
            rows.clear();
            rows.extend(local_provider_rows.clone());
        }

        println!();
        println!("{}", "═".repeat(80).bright_black());
        println!("💡 提示: 点击右侧的启用/禁用按钮来切换服务商状态，按ESC退出");

        // 事件循环
        // 添加防抖变量，防止快速连续点击
        let mut last_click_time = std::time::Instant::now();
        let debounce_duration = std::time::Duration::from_millis(300); // 300毫秒防抖
        
        loop {
            // 使用非阻塞方式检查事件，设置较短的超时时间
            if event::poll(std::time::Duration::from_millis(50))? {
                if let Ok(event) = event::read() {
                    match event {
                        Event::Key(key) => {
                            if key.code == KeyCode::Esc {
                                break;
                            }
                        }
                        Event::Mouse(mouse) => {
                            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                                let now = std::time::Instant::now();
                                // 检查是否超过防抖时间
                                if now.duration_since(last_click_time) >= debounce_duration {
                                    last_click_time = now;
                                    
                                    // 使用本地的 provider_rows 副本
                                    // 检查点击位置是否在某个服务商的切换按钮上
                                    for row in &local_provider_rows {
                                        if mouse.row == row.y_position &&
                                           mouse.column >= row.toggle_button_x &&
                                           mouse.column < row.toggle_button_x + row.toggle_button_width {
                                                
                                                // 切换服务商状态
                                                let new_disabled_state = self.toggle_provider(&row.provider_name);
                                                
                                                // 重新渲染这一行
                                                if let Err(e) = self.refresh_provider_row(&providers[row.index], row, state, new_disabled_state) {
                                                    eprintln!("Error refreshing provider {}: {}", row.provider_name, e);
                                                }
                                                
                                                // 强制刷新输出
                                                stdout().flush()?;
                                                
                                                // 短暂延迟，确保UI更新完成
                                                std::thread::sleep(std::time::Duration::from_millis(10));
                                                
                                                break; // 找到并处理了一个按钮，退出循环
                                            }
                                        }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            
            // 短暂休眠，减少CPU使用率
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        disable_raw_mode()?;
        execute!(stdout(), Show, Clear(ClearType::All))?;
        Ok(())
    }

    /// 刷新单个服务商行的显示
    fn refresh_provider_row(&self, provider: &Provider, row: &ProviderRow, state: &ProxyState, is_disabled: bool) -> std::io::Result<()> {
        // 使用 try_lock 获取状态信息，避免死锁
        let health_score = state.get_provider_health_score(&provider.name);
        let is_healthy = state.is_provider_healthy(&provider.name);
        let current_requests = state.get_current_requests(&provider.name);
        let can_request = state.can_request(&provider.name);

        let (status_icon, health_color) = match health_score {
            90..=100 => ("🟢", "bright_green"),
            70..=89 => ("🟡", "bright_yellow"), 
            40..=69 => ("🟠", "yellow"),
            20..=39 => ("🔴", "bright_red"),
            _ => ("💀", "red"),
        };

        let name_display_width = calculate_display_width(&provider.name);
        let name_padding = if name_display_width < 15 { 15 - name_display_width } else { 1 };
        
        let health_text = if health_score > 20 { "健康" } else { "异常" };
        let status_text = if is_healthy { "可用" } else { "不可用" };
        let rate_status = if can_request { "✅" } else { "🚫" };
        
        let toggle_button = if is_disabled { 
            "[❌禁用]".bright_red()
        } else { 
            "[✅启用]".bright_green()
        };

        // 清除当前行，确保没有残留字符
        execute!(stdout(), MoveTo(0, row.y_position), Clear(ClearType::CurrentLine))?;
        execute!(stdout(), MoveTo(0, row.y_position))?;

        // 使用 execute! 而不是 print!，以便更好地处理错误
        if is_disabled {
            execute!(stdout(), 
                Print(format!("{} {:<2} {}{} {:<4} {:<4}% │ 速率: {:<2}/{:<2} {} │ {:<6} │ {}", 
                    status_icon.bright_black(),
                    row.index + 1,
                    provider.name.bright_black(),
                    " ".repeat(name_padding),
                    health_text.bright_black(),
                    health_score.to_string().bright_black(),
                    current_requests.to_string().bright_black(),
                    state.get_rate_limit().to_string().bright_black(),
                    rate_status.bright_black(),
                    status_text.bright_black(),
                    toggle_button
                ))
            )?;
        } else {
            execute!(stdout(), 
                Print(format!("{} {:<2} {}{} {:<4} {:<4}% │ 速率: {:<2}/{:<2} {} │ {:<6} │ {}", 
                    status_icon,
                    row.index + 1,
                    provider.name.bright_cyan(),
                    " ".repeat(name_padding),
                    if health_score > 20 { health_text.bright_green() } else { health_text.bright_red() },
                    health_score.to_string().color(health_color).bold(),
                    current_requests.to_string().bright_cyan(),
                    state.get_rate_limit().to_string().bright_white(),
                    rate_status,
                    if is_healthy { status_text.bright_green() } else { status_text.bright_red() },
                    toggle_button
                ))
            )?;
        }

        // 确保立即刷新输出
        stdout().flush()?;
        
        // 短暂延迟，确保UI更新完成
        std::thread::sleep(std::time::Duration::from_millis(10));
        
        Ok(())
    }
    
    /// 刷新所有服务商的显示
    pub fn refresh_providers(&self, providers: &Vec<Provider>, state: &ProxyState) -> std::io::Result<()> {
        // 创建本地变量
        let mut local_rows = Vec::new();
        let mut old_positions = Vec::new();
        
        // 获取当前行位置用于清除
        if let Ok(rows) = self.provider_rows.try_lock() {
            for row in rows.iter() {
                old_positions.push(row.y_position);
            }
        } else {
            // 如果无法获取锁，说明另一个线程正在更新，直接返回
            return Ok(());
        }
        
        // 清除之前的行
        for y_position in old_positions {
            execute!(stdout(), MoveTo(0, y_position), Clear(ClearType::CurrentLine))?;
        }
        
        // 重新计算行位置
        let mut y_position = 3; // 从第3行开始显示服务商
        let toggle_button_x = 65; // 按钮的X位置
        
        for (index, provider) in providers.iter().enumerate() {
            let row = ProviderRow {
                index,
                provider_name: provider.name.clone(),
                y_position,
                toggle_button_x,
                toggle_button_width: 8,
            };
            
            // 使用 try_lock 检查禁用状态
            let is_disabled = self.is_provider_disabled(&provider.name);
            
            // 刷新单个服务商行，添加错误处理
            if let Err(e) = self.refresh_provider_row(provider, &row, state, is_disabled) {
                // 记录错误但继续处理其他服务商
                eprintln!("Error refreshing provider {}: {}", provider.name, e);
            }
            
            local_rows.push(row);
            y_position += 1;
        }
        
        // 更新 provider_rows
        if let Ok(mut rows) = self.provider_rows.try_lock() {
            rows.clear();
            rows.extend(local_rows);
        }
        
        // 确保立即刷新输出
        stdout().flush()?;
        
        Ok(())
    }
}

impl Default for InteractiveProviderManager {
    fn default() -> Self {
        Self::new()
    }
}