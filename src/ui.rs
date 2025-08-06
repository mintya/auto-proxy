//! 终端UI模块 - 实现顶部状态栏和底部滚动日志

use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use std::io::{self, Write};
use crossterm::{
    terminal::{self, ClearType},
    cursor::{self, MoveTo},
    style::{Color, SetForegroundColor, ResetColor, Print},
    execute, queue,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind, MouseButton, EnableMouseCapture, DisableMouseCapture},
};
use chrono::{DateTime, Local};
use crate::provider::Provider;
use crate::proxy::ProxyState;
use crate::network::NetworkStatus;

/// 文本对齐方式
#[derive(Clone, Copy)]
enum TextAlign {
    Center,
    Right,
}

/// 服务商按钮位置信息
#[derive(Clone)]
pub struct ProviderButton {
    pub provider_name: String,
    pub row: u16,
    pub start_col: u16,
    pub end_col: u16,
}

/// 日志条目
#[derive(Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub level: LogLevel,
    pub message: String,
}

/// 日志级别
#[derive(Clone, Debug)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
    Debug,
}

impl LogLevel {
    pub fn color(&self) -> Color {
        match self {
            LogLevel::Info => Color::Cyan,
            LogLevel::Success => Color::Green,
            LogLevel::Warning => Color::Yellow,
            LogLevel::Error => Color::Red,
            LogLevel::Debug => Color::DarkGrey,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            LogLevel::Info => "ℹ️",
            LogLevel::Success => "✅",
            LogLevel::Warning => "⚠️",
            LogLevel::Error => "❌",
            LogLevel::Debug => "🔍",
        }
    }
}

/// 终端UI管理器
pub struct TerminalUI {
    logs: Arc<Mutex<VecDeque<LogEntry>>>,
    max_logs: usize,
    is_initialized: bool,
    provider_buttons: Vec<ProviderButton>,
}

impl TerminalUI {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            logs: Arc::new(Mutex::new(VecDeque::new())),
            max_logs: 100,
            is_initialized: false,
            provider_buttons: Vec::new(),
        })
    }

    /// 统一的表格行格式化函数（无分隔符，纯固定宽度）
    fn format_table_row(
        status: &str,
        name: &str, 
        health: &str,
        rate: &str,
        token: &str,
        status_code: &str,
        action: &str,
    ) -> String {
        format!("{}{}{}{}{}{}{}", status, name, health, rate, token, status_code, action)
    }

    /// 格式化文本到指定宽度（考虑中文字符和emoji的实际显示宽度）
    fn format_text_with_width(text: &str, width: usize, _align: TextAlign) -> String {
        let display_w = display_width(text);
        
        if display_w >= width {
            // 如果文本太长，截断它
            let mut result = String::new();
            let mut current_width = 0;
            for ch in text.chars() {
                let ch_width = match ch {
                    '🚀' | '📊' | '🟢' | '🟡' | '🟠' | '🔴' | '💀' | '✅' | '🚫' | '❌' => 2,
                    c if c as u32 >= 0x4E00 && c as u32 <= 0x9FFF => 2,
                    _ => 1,
                };
                if current_width + ch_width <= width - 1 {  // 为省略号留空间
                    result.push(ch);
                    current_width += ch_width;
                } else {
                    result.push('…');
                    current_width += 1;
                    break;
                }
            }
            // 填充到指定宽度
            while current_width < width {
                result.push(' ');
                current_width += 1;
            }
            return result;
        }

        let padding = width - display_w;
        // 支持居中和右对齐
        match _align {
            TextAlign::Center => {
                let left_pad = padding / 2;
                let right_pad = padding - left_pad;
                format!("{}{}{}", " ".repeat(left_pad), text, " ".repeat(right_pad))
            }
            TextAlign::Right => {
                format!("{}{}", " ".repeat(padding), text)
            }
        }
    }

    /// 初始化终端UI
    pub fn initialize(&mut self) -> io::Result<()> {
        if self.is_initialized {
            return Ok(());
        }

        // 启用原始模式和备用屏幕缓冲区
        terminal::enable_raw_mode()?;
        execute!(
            io::stdout(),
            terminal::EnterAlternateScreen,
            cursor::Hide,
            EnableMouseCapture
        )?;

        self.is_initialized = true;
        self.clear_screen()?;
        Ok(())
    }

    /// 清理终端UI
    /// 清理终端状态
    pub fn cleanup(&mut self) -> io::Result<()> {
        if !self.is_initialized {
            return Ok(());
        }

        // 确保清理顺序正确，避免终端状态混乱
        let cleanup_result = execute!(
            io::stdout(),
            DisableMouseCapture,
            cursor::Show,
            terminal::Clear(ClearType::All),
            terminal::LeaveAlternateScreen
        );
        
        // 无论execute!是否成功，都尝试禁用raw模式
        let raw_mode_result = terminal::disable_raw_mode();
        
        self.is_initialized = false;
        
        // 如果任何一个操作失败，返回错误但不崩溃
        match (cleanup_result, raw_mode_result) {
            (Ok(_), Ok(_)) => Ok(()),
            (Err(e), _) => {
                eprintln!("⚠️ Terminal cleanup failed: {}", e);
                // 强制终端重置
                let _ = execute!(io::stdout(), terminal::LeaveAlternateScreen, cursor::Show);
                Err(e)
            },
            (_, Err(e)) => {
                eprintln!("⚠️ Raw mode disable failed: {}", e);
                Err(io::Error::new(io::ErrorKind::Other, e))
            }
        }
    }

    /// 清屏
    fn clear_screen(&self) -> io::Result<()> {
        execute!(
            io::stdout(),
            terminal::Clear(ClearType::All),
            MoveTo(0, 0)
        )?;
        Ok(())
    }

    /// 添加日志条目
    pub fn log(&self, level: LogLevel, message: String) {
        let entry = LogEntry {
            timestamp: Local::now(),
            level,
            message,
        };

        let mut logs = match self.logs.lock() {
            Ok(logs) => logs,
            Err(poisoned) => {
                eprintln!("⚠️ UI logs mutex poisoned, recovering...");
                poisoned.into_inner()
            }
        };
        logs.push_back(entry);
        
        // 保持日志数量限制
        while logs.len() > self.max_logs {
            logs.pop_front();
        }
    }

    /// 渲染整个界面
    pub fn render(&mut self, providers: &[Provider], state: &ProxyState, server_info: &ServerInfo) -> io::Result<()> {
        if !self.is_initialized {
            return Ok(());
        }

        let (cols, rows) = terminal::size()?;
        
        // 动态计算状态栏高度 - 显示所有提供商
        let base_height = 7; // 基本信息行数（顶部边框、服务器信息行、分隔线、提供商概览行、分隔线、表头行、底部边框）
        let provider_lines = providers.len(); // 显示所有提供商
        let dynamic_status_height = (base_height + provider_lines) as u16;
        
        let mut stdout = io::stdout();

        // 移动到顶部开始绘制，不要完全清屏避免闪烁
        queue!(stdout, MoveTo(0, 0))?;

        // 绘制状态栏
        self.render_status_bar(&mut stdout, providers, state, server_info, cols, dynamic_status_height)?;

        // 绘制分隔线
        queue!(stdout, MoveTo(0, dynamic_status_height))?;
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        for _ in 0..cols {
            queue!(stdout, Print("─"))?;
        }
        queue!(stdout, ResetColor)?;

        // 绘制帮助信息
        queue!(stdout, MoveTo(0, dynamic_status_height + 1))?;
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        queue!(stdout, Print("按键: [Q]退出 | 鼠标: 点击[启用/禁用]按钮切换服务商状态"))?;
        queue!(stdout, ResetColor)?;

        // 绘制日志区域
        let log_start_row = dynamic_status_height + 2;
        let log_height = rows.saturating_sub(log_start_row);
        self.render_logs(&mut stdout, log_start_row, log_height, cols)?;

        stdout.flush()?;
        Ok(())
    }

    /// 绘制状态栏
    fn render_status_bar(
        &mut self,
        stdout: &mut io::Stdout,
        providers: &[Provider],
        state: &ProxyState,
        server_info: &ServerInfo,
        cols: u16,
        status_height: u16,
    ) -> io::Result<()> {
        // 绘制顶部边框
        queue!(stdout, MoveTo(0, 0))?;
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        queue!(stdout, Print("┌"))?;
        for _ in 1..(cols - 1) {
            queue!(stdout, Print("─"))?;
        }
        queue!(stdout, Print("┐"))?;
        queue!(stdout, ResetColor)?;

        // 第1行：服务器信息（带边框）
        queue!(stdout, MoveTo(0, 1))?;
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        queue!(stdout, Print("│"))?;
        queue!(stdout, ResetColor)?;
        
        queue!(stdout, SetForegroundColor(Color::Cyan))?;
        queue!(stdout, Print(" 🚀 Auto Proxy"))?;
        queue!(stdout, ResetColor)?;
        
        let server_info_text = format!(" | 端口: {} | 速率限制: {}/分钟 | 运行时间: {}", 
            server_info.port,
            server_info.rate_limit,
            format_duration(server_info.uptime())
        );
        queue!(stdout, Print(server_info_text.clone()))?;
        
        // 添加网络状态
        let network_status = server_info.get_network_status();
        let network_text = format!(" | 网络: {} {}", network_status.status_icon(), network_status.status_text());
        queue!(stdout, Print(network_text.clone()))?;
        
            // 计算已使用的显示宽度并填充空格到右边框
            let app_name_width = display_width(" 🚀 Auto Proxy");
            let used_width = app_name_width + display_width(&server_info_text) + display_width(&network_text);
            if used_width < (cols - 2) as usize {
                for _ in 0..((cols - 2) as usize - used_width) {
                    queue!(stdout, Print(" "))?;
                }
            }
        
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        queue!(stdout, Print("│"))?;
        queue!(stdout, ResetColor)?;

        // 绘制分隔线
        queue!(stdout, MoveTo(0, 2))?;
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        queue!(stdout, Print("├"))?;
        for _ in 1..(cols - 1) {
            queue!(stdout, Print("─"))?;
        }
        queue!(stdout, Print("┤"))?;
        queue!(stdout, ResetColor)?;

        // 第2行：提供商概览（带边框）
        queue!(stdout, MoveTo(0, 3))?;
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        queue!(stdout, Print("│"))?;
        queue!(stdout, ResetColor)?;
        
        let healthy_count = providers.iter()
            .filter(|p| state.is_provider_healthy(&p.name))
            .count();
        
        let total_health: u32 = providers.iter()
            .map(|p| state.get_provider_health_score(&p.name) as u32)
            .sum();
        let avg_health = if providers.is_empty() { 0 } else { total_health / providers.len() as u32 };

        let overview_text = format!(" 📊 提供商: {}/{} 健康 | 平均健康度: {}% | 状态: ", 
            healthy_count, providers.len(), avg_health);
        queue!(stdout, Print(overview_text.clone()))?;
        
        let status_text = if healthy_count > 0 {
            queue!(stdout, SetForegroundColor(Color::Green))?;
            queue!(stdout, Print("正常"))?;
            "正常"
        } else {
            queue!(stdout, SetForegroundColor(Color::Red))?;
            queue!(stdout, Print("异常"))?;
            "异常"
        };
        queue!(stdout, ResetColor)?;
        
        // 使用显示宽度计算填充空格到右边框
        let used_width = display_width(&overview_text) + display_width(status_text);
        if used_width < (cols - 2) as usize {
            for _ in 0..((cols - 2) as usize - used_width) {
                queue!(stdout, Print(" "))?;
            }
        }
        
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        queue!(stdout, Print("│"))?;
        queue!(stdout, ResetColor)?;

        // 清空之前的按钮位置记录
        self.provider_buttons.clear();

        // 定义纯固定宽度列布局（无分隔符）
        const COL_STATUS: usize = 8;      // "🟢 01  "
        const COL_NAME: usize = 20;       // "Claude-3.5-Sonnet  "
        const COL_HEALTH: usize = 8;     // "  100%   "
        const COL_RATE: usize = 12;       // " 5/10  ✅  "
        const COL_TOKEN: usize = 15;      // "1.2K(12.3%)        "
        const COL_STATUS_CODE: usize = 8; // " 200    "
        const COL_ACTION: usize = 10;     // "  ✅启用  "

        // 第4行：分隔线
        queue!(stdout, MoveTo(0, 4))?;
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        queue!(stdout, Print("├"))?;
        for _ in 1..(cols - 1) {
            queue!(stdout, Print("─"))?;
        }
        queue!(stdout, Print("┤"))?;
        queue!(stdout, ResetColor)?;
        
        // 第5行开始：表头 + 数据行，使用统一的固定宽度渲染函数
        
        // 渲染表头行
        queue!(stdout, MoveTo(0, 5))?;
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        queue!(stdout, Print("│"))?;
        queue!(stdout, ResetColor)?;
        
        // 表头内容使用新的格式化函数
        let header_content = Self::format_table_row(
            &Self::format_text_with_width("状态", COL_STATUS, TextAlign::Center),
            &Self::format_text_with_width("服务商名称", COL_NAME, TextAlign::Center),
            &Self::format_text_with_width("健康", COL_HEALTH, TextAlign::Center),
            &Self::format_text_with_width("速率限制", COL_RATE, TextAlign::Center),
            &Self::format_text_with_width("Token使用", COL_TOKEN, TextAlign::Center),
            &Self::format_text_with_width("状态码", COL_STATUS_CODE, TextAlign::Center),
            &Self::format_text_with_width("操作", COL_ACTION, TextAlign::Center),
        );
        
        queue!(stdout, SetForegroundColor(Color::White))?;
        queue!(stdout, Print(header_content))?;
        queue!(stdout, ResetColor)?;
        
        // 计算固定表格宽度（无分隔符）- 现在这个宽度是准确的，因为我们的格式化函数保证了每列的宽度
        let fixed_table_width = COL_STATUS + COL_NAME + COL_HEALTH + COL_RATE + COL_TOKEN + COL_STATUS_CODE + COL_ACTION;
        
        // 填充表头的剩余空间（不需要条件检查，直接填充到边框位置）
        let remaining_space = if cols >= 2 { (cols - 2) as usize } else { 0 };
        if remaining_space > fixed_table_width {
            for _ in 0..(remaining_space - fixed_table_width) {
                queue!(stdout, Print(" "))?;
            }
        }
        
        // 表头右边框
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        queue!(stdout, Print("│"))?;
        queue!(stdout, ResetColor)?;
        
        // 第6行开始：数据行（与表头使用统一的固定宽度布局）
        for (i, provider) in providers.iter().enumerate() {
            let row = 6 + i as u16;
            queue!(stdout, MoveTo(0, row))?;
            
            // 左边框
            queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
            queue!(stdout, Print("│"))?;
            queue!(stdout, ResetColor)?;
            
            let health_score = state.get_provider_health_score(&provider.name);
            let current_requests = state.get_current_requests(&provider.name);
            let can_request = state.can_request(&provider.name);
            let last_status = state.get_last_status_code(&provider.name);
            let is_disabled = state.interactive_manager.is_provider_disabled(&provider.name);
            
            // 状态图标
            let (status_icon, health_color) = match health_score {
                90..=100 => ("🟢", Color::Green),
                70..=89 => ("🟡", Color::Yellow),
                40..=69 => ("🟠", Color::DarkYellow),
                20..=39 => ("🔴", Color::Red),
                _ => ("💀", Color::DarkRed),
            };

            // 获取token数据
            let token_usage = state.get_token_usage(&provider.name);
            let usage_percentage = state.get_provider_usage_percentage(&provider.name);
            
            // 使用新的格式化函数处理各个字段
            // 状态列：图标 + 序号
            let status_field = format!("{} {:2}", status_icon, i + 1);
            let status_display = Self::format_text_with_width(&status_field, COL_STATUS, TextAlign::Center);
            
            // 服务商名称列
            let name_display = Self::format_text_with_width(&provider.name, COL_NAME, TextAlign::Center);
            
            // 健康度列 - 使用右对齐
            let health_text = format!("{}%", health_score);
            let health_display = Self::format_text_with_width(&health_text, COL_HEALTH, TextAlign::Right);
            
            // 速率限制列
            let rate_text = format!("{}/{} {}", current_requests, state.get_rate_limit(), if can_request { "✅" } else { "🚫" });
            let rate_display = Self::format_text_with_width(&rate_text, COL_RATE, TextAlign::Center);
            
            // Token使用列 - 使用右对齐
            let token_text = if token_usage > 0 {
                format!("{}({:.1}%)", format_tokens(token_usage), usage_percentage)
            } else {
                "0(0.0%)".to_string()
            };
            let token_display = Self::format_text_with_width(&token_text, COL_TOKEN, TextAlign::Right);
            
            // 状态码列
            let status_code_text = match last_status {
                Some(0) => "网络错误".to_string(),
                Some(code) => code.to_string(),
                None => "--".to_string(),
            };
            let status_code_display = Self::format_text_with_width(&status_code_text, COL_STATUS_CODE, TextAlign::Center);
            
            // 操作列
            let action_text = if is_disabled { "❌禁用" } else { "✅启用" };
            let action_display = Self::format_text_with_width(action_text, COL_ACTION, TextAlign::Center);

            // 使用统一的行格式化函数（无分隔符，纯固定宽度）
            if is_disabled {
                queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
                let row_content = Self::format_table_row(
                    &status_display, &name_display, &health_display, &rate_display, 
                    &token_display, &status_code_display, &action_display
                );
                queue!(stdout, Print(row_content))?;
                queue!(stdout, ResetColor)?;
            } else {
                // 正常显示，分字段着色但仍使用固定宽度布局
                queue!(stdout, Print(status_display))?;
                
                queue!(stdout, SetForegroundColor(Color::Cyan))?;
                queue!(stdout, Print(name_display.clone()))?;
                queue!(stdout, ResetColor)?;
                
                queue!(stdout, SetForegroundColor(health_color))?;
                queue!(stdout, Print(health_display.clone()))?;
                queue!(stdout, ResetColor)?;
                
                if can_request {
                    queue!(stdout, SetForegroundColor(Color::Green))?;
                } else {
                    queue!(stdout, SetForegroundColor(Color::Red))?;
                }
                queue!(stdout, Print(rate_display.clone()))?;
                queue!(stdout, ResetColor)?;
                
                queue!(stdout, SetForegroundColor(Color::Magenta))?;
                queue!(stdout, Print(token_display.clone()))?;
                queue!(stdout, ResetColor)?;
                
                let status_color = if let Some(code) = last_status {
                    if code == 0 { Color::DarkGrey }
                    else if code >= 200 && code < 300 { Color::Green }
                    else if code >= 400 && code < 500 { Color::Yellow }
                    else if code >= 500 { Color::Red }
                    else { Color::DarkGrey }
                } else {
                    Color::DarkGrey
                };
                queue!(stdout, SetForegroundColor(status_color))?;
                queue!(stdout, Print(status_code_display.clone()))?;
                queue!(stdout, ResetColor)?;
                
                if is_disabled {
                    queue!(stdout, SetForegroundColor(Color::DarkRed))?;
                } else {
                    queue!(stdout, SetForegroundColor(Color::Green))?;
                }
                queue!(stdout, Print(action_display.clone()))?;
                queue!(stdout, ResetColor)?;
            }

            // 计算按钮位置（基于纯固定列宽，无分隔符）
            let button_start_col = (COL_STATUS + COL_NAME + COL_HEALTH + COL_RATE + COL_TOKEN + COL_STATUS_CODE + 1) as u16; // 到操作列开始的位置
            let button_end_col = button_start_col + COL_ACTION as u16;
            
            self.provider_buttons.push(ProviderButton {
                provider_name: provider.name.clone(),
                row: 6 + i as u16,  // 提供商数据从第6行开始
                start_col: button_start_col,
                end_col: button_end_col,
            });
            
            // 填充到固定表格宽度后的剩余空间（使用与表头相同的逻辑）
            let remaining_space = if cols >= 2 { (cols - 2) as usize } else { 0 };
            if remaining_space > fixed_table_width {
                for _ in 0..(remaining_space - fixed_table_width) {
                    queue!(stdout, Print(" "))?;
                }
            }
            
            // 右边框
            queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
            queue!(stdout, Print("│"))?;
            queue!(stdout, ResetColor)?;
        }

        // 绘制底部边框
        queue!(stdout, MoveTo(0, status_height - 1))?;
        queue!(stdout, SetForegroundColor(Color::DarkGrey))?;
        queue!(stdout, Print("└"))?;
        for _ in 1..(cols - 1) {
            queue!(stdout, Print("─"))?;
        }
        queue!(stdout, Print("┘"))?;
        queue!(stdout, ResetColor)?;

        Ok(())
    }

    /// 绘制日志区域
    fn render_logs(
        &self,
        stdout: &mut io::Stdout,
        start_row: u16,
        height: u16,
        cols: u16,
    ) -> io::Result<()> {
        let logs = self.logs.lock().unwrap();
        
        if logs.is_empty() {
            return Ok(());
        }
        
        // 显示最新的日志（从底部开始）
        let total_logs = logs.len();
        let visible_count = height as usize;
        let start = total_logs.saturating_sub(visible_count);
        let visible_logs: Vec<_> = logs.iter().skip(start).collect();

        // 从底部开始绘制日志（最新的在底部）
        for (i, log_entry) in visible_logs.iter().enumerate() {
            let row = start_row + i as u16;
            if row >= start_row + height {
                break;
            }
            
            queue!(stdout, MoveTo(0, row))?;

            // 时间戳 - 使用更亮的颜色
            queue!(stdout, SetForegroundColor(Color::White))?;
            queue!(stdout, Print(log_entry.timestamp.format("%H:%M:%S")))?;
            queue!(stdout, ResetColor)?;

            // 图标和消息
            queue!(stdout, Print(" "))?;
            queue!(stdout, Print(log_entry.level.icon()))?;
            queue!(stdout, Print(" "))?;
            queue!(stdout, SetForegroundColor(log_entry.level.color()))?;
            
            // 截断过长的消息
            let max_msg_len = cols.saturating_sub(12) as usize; // 为时间戳和图标留空间
            let message = if log_entry.message.len() > max_msg_len {
                format!("{}...", &log_entry.message[..max_msg_len.saturating_sub(3)])
            } else {
                log_entry.message.clone()
            };
            
            queue!(stdout, Print(message))?;
            queue!(stdout, ResetColor)?;

            // 清除行的剩余部分
            let used_length = 12 + log_entry.message.len().min(max_msg_len);
            if used_length < cols as usize {
                for _ in 0..(cols as usize - used_length) {
                    queue!(stdout, Print(" "))?;
                }
            }
        }

        // 清除日志区域的空白行
        let logs_shown = visible_logs.len().min(height as usize);
        for i in logs_shown..(height as usize) {
            let row = start_row + i as u16;
            queue!(stdout, MoveTo(0, row))?;
            for _ in 0..cols {
                queue!(stdout, Print(" "))?;
            }
        }

        // 不显示滚动指示器

        Ok(())
    }

    /// 检查是否有退出键按下
    /// 检查键盘输入并返回动作
    pub fn check_key_input(&mut self) -> io::Result<String> {
        if !self.is_initialized {
            return Ok("none".to_string());
        }

        // 非阻塞检查键盘输入，使用很短的超时避免阻塞
        if let Ok(has_event) = event::poll(std::time::Duration::from_millis(1)) {
            if !has_event {
                return Ok("none".to_string());
            }
        } else {
            return Ok("none".to_string());
        }

        match event::read() {
            Ok(Event::Key(KeyEvent { code, modifiers, .. })) => {
                match code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => return Ok("exit".to_string()),
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => return Ok("exit".to_string()),
                    KeyCode::Esc => return Ok("exit".to_string()),
                    _ => {}
                }
            },
            Ok(Event::Mouse(MouseEvent { kind, column, row, .. })) => {
                if let MouseEventKind::Down(MouseButton::Left) = kind {
                    // 检查点击是否在某个服务商按钮上
                    for button in &self.provider_buttons {
                        if row == button.row && column >= button.start_col && column <= button.end_col {
                            return Ok(format!("toggle:{}", button.provider_name));
                        }
                    }
                }
            },
            Ok(_) => {
                // 忽略其他事件
            },
            Err(_) => {
                // 事件读取错误，忽略
            }
        }
        Ok("none".to_string())
    }

    pub fn check_exit_key(&mut self) -> io::Result<bool> {
        if !self.is_initialized {
            return Ok(false);
        }

        // 非阻塞检查键盘输入
        if event::poll(std::time::Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(KeyEvent { code, modifiers, .. }) => {
                    match code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(true),
                        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => return Ok(true),
                        KeyCode::Esc => return Ok(true),
                        _ => {}
                    }
                },
                _ => {}
            }
        }
        Ok(false)
    }

    /// 获取日志记录器的克隆
    pub fn logger(&self) -> Logger {
        Logger {
            logs: Arc::clone(&self.logs),
            max_logs: self.max_logs,
        }
    }
}

impl Drop for TerminalUI {
    fn drop(&mut self) {
        if self.is_initialized {
            // 尝试正常清理
            if let Err(e) = self.cleanup() {
                eprintln!("⚠️ Failed to cleanup terminal in Drop: {}", e);
                
                // 强制终端重置，确保终端不会处于损坏状态
                let _ = execute!(
                    io::stdout(),
                    DisableMouseCapture,
                    cursor::Show,
                    terminal::LeaveAlternateScreen
                );
                let _ = terminal::disable_raw_mode();
                
                eprintln!("🔧 Forced terminal reset completed");
            }
        }
    }
}

/// 日志记录器
#[derive(Clone)]
pub struct Logger {
    logs: Arc<Mutex<VecDeque<LogEntry>>>,
    max_logs: usize,
}

impl Logger {
    pub fn info(&self, message: String) {
        self.log(LogLevel::Info, message);
    }

    pub fn success(&self, message: String) {
        self.log(LogLevel::Success, message);
    }

    pub fn warning(&self, message: String) {
        self.log(LogLevel::Warning, message);
    }

    pub fn error(&self, message: String) {
        self.log(LogLevel::Error, message);
    }

    pub fn debug(&self, message: String) {
        self.log(LogLevel::Debug, message);
    }

    fn log(&self, level: LogLevel, message: String) {
        let entry = LogEntry {
            timestamp: Local::now(),
            level,
            message,
        };

        let mut logs = match self.logs.lock() {
            Ok(logs) => logs,
            Err(poisoned) => {
                eprintln!("⚠️ UI logs mutex poisoned, recovering...");
                poisoned.into_inner()
            }
        };
        logs.push_back(entry);
        
        while logs.len() > self.max_logs {
            logs.pop_front();
        }
    }
}

/// 服务器信息
pub struct ServerInfo {
    pub port: u16,
    pub rate_limit: usize,
    pub start_time: DateTime<Local>,
    pub network_status: std::sync::Mutex<NetworkStatus>,
}

impl ServerInfo {
    pub fn new(port: u16, rate_limit: usize) -> Self {
        Self {
            port,
            rate_limit,
            start_time: Local::now(),
            network_status: std::sync::Mutex::new(NetworkStatus::new()),
        }
    }

    pub fn uptime(&self) -> chrono::Duration {
        Local::now() - self.start_time
    }

    pub fn update_network_status(&self, status: NetworkStatus) {
        if let Ok(mut current_status) = self.network_status.lock() {
            *current_status = status;
        }
    }

    pub fn get_network_status(&self) -> NetworkStatus {
        match self.network_status.lock() {
            Ok(status) => status.clone(),
            Err(poisoned) => {
                eprintln!("⚠️ Network status mutex poisoned, recovering...");
                poisoned.into_inner().clone()
            }
        }
    }
}

/// 格式化持续时间
fn format_duration(duration: chrono::Duration) -> String {
    let total_seconds = duration.num_seconds();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{}h{}m{}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m{}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// 格式化Token数量，使用K/M后缀
fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

/// 计算字符串的显示宽度（考虑emoji和中文字符）
fn display_width(s: &str) -> usize {
    let mut width = 0;
    let mut chars = s.chars().peekable();
    
    while let Some(ch) = chars.next() {
        width += match ch {
            // Emoji通常占用2个字符宽度
            '🚀' | '📊' | '🟢' | '🟡' | '🟠' | '🔴' | '💀' | '✅' | '🚫' |
            '❌' | '🔍' => 2,
            // 其他emoji类字符
            'ℹ' | '⚠' => {
                // 检查是否有组合字符
                if chars.peek() == Some(&'\u{fe0f}') {
                    chars.next(); // 消耗组合字符
                }
                2
            },
            // 中文字符占用2个字符宽度
            c if c as u32 >= 0x4E00 && c as u32 <= 0x9FFF => 2,
            // 组合字符不占用宽度
            '\u{fe0f}' => 0,
            // 其他字符占用1个字符宽度
            _ => 1,
        };
    }
    width
}
