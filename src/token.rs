use serde_json;

/// Token 计算相关功能
pub struct TokenCalculator;

impl TokenCalculator {
    /// 估算请求Token使用量（输入tokens）
    pub fn estimate_request_usage(body_bytes: &hyper::body::Bytes, uri: &hyper::Uri) -> u64 {
        // 尝试解析JSON请求体获取更准确的token计算
        if let Ok(body_str) = std::str::from_utf8(body_bytes) {
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(body_str) {
                // 基于实际内容计算tokens
                return Self::estimate_from_json(&json_value, uri);
            }
        }
        
        // 回退到基础估算
        Self::fallback_estimation(body_bytes, uri)
    }

    /// 估算响应Token使用量（输出tokens）
    pub fn estimate_response_usage(response_body: &[u8]) -> u64 {
        // 尝试解析响应JSON
        if let Ok(body_str) = std::str::from_utf8(response_body) {
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(body_str) {
                return Self::estimate_response_from_json(&json_value);
            }
        }
        
        // 回退到基于长度的估算
        Self::estimate_text_tokens(&String::from_utf8_lossy(response_body))
    }

    /// 估算总Token使用量（请求+响应）
    pub fn estimate_total_usage(
        request_body: &hyper::body::Bytes, 
        uri: &hyper::Uri,
        response_body: &[u8]
    ) -> (u64, u64, u64) {
        let input_tokens = Self::estimate_request_usage(request_body, uri);
        let output_tokens = Self::estimate_response_usage(response_body);
        let total_tokens = input_tokens + output_tokens;
        
        (input_tokens, output_tokens, total_tokens)
    }

    /// 从响应JSON估算token数量
    fn estimate_response_from_json(json: &serde_json::Value) -> u64 {
        let mut total_tokens = 0u64;
        
        if let Some(obj) = json.as_object() {
            // OpenAI/Claude API响应格式
            if let Some(choices) = obj.get("choices").and_then(|v| v.as_array()) {
                for choice in choices {
                    if let Some(message) = choice.get("message") {
                        if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
                            total_tokens += Self::estimate_text_tokens(content);
                        }
                    }
                    // Claude completion text
                    if let Some(text) = choice.get("text").and_then(|v| v.as_str()) {
                        total_tokens += Self::estimate_text_tokens(text);
                    }
                }
            }
            
            // Claude API直接内容
            else if let Some(content) = obj.get("content") {
                total_tokens += Self::estimate_content_tokens(content);
            }
            
            // 单独的文本内容
            else if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                total_tokens += Self::estimate_text_tokens(text);
            }
            
            // 检查usage字段（如果API提供了准确的token计数）
            if let Some(usage) = obj.get("usage") {
                if let Some(completion_tokens) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
                    return completion_tokens; // 优先使用API提供的准确数字
                }
            }
        }
        
        total_tokens.max(1) // 确保至少返回1个token
    }

    /// 基于请求估算响应token数量（当无法读取响应体时的估算）
    pub fn estimate_response_from_request(request_body: &hyper::body::Bytes, uri: &hyper::Uri) -> u64 {
        let input_tokens = Self::estimate_request_usage(request_body, uri);
        
        // 基于请求复杂度估算响应长度
        // 一般AI响应的token数大约是请求的0.5-2倍
        let response_ratio = if input_tokens < 100 {
            1.5 // 简单问题通常有相对较长的回答
        } else if input_tokens < 500 {
            1.2 // 中等问题
        } else if input_tokens < 1000 {
            0.8 // 复杂问题可能有较短但精确的回答
        } else {
            0.6 // 非常长的输入通常需要简洁回答
        };
        
        ((input_tokens as f64) * response_ratio) as u64
    }

    /// 估算完整对话的token使用量（请求+估算响应）
    pub fn estimate_conversation_usage(request_body: &hyper::body::Bytes, uri: &hyper::Uri) -> (u64, u64, u64) {
        let input_tokens = Self::estimate_request_usage(request_body, uri);
        let estimated_output_tokens = Self::estimate_response_from_request(request_body, uri);
        let total_tokens = input_tokens + estimated_output_tokens;
        
        (input_tokens, estimated_output_tokens, total_tokens)
    }

    /// 估算Token使用量（保持向后兼容，现在使用更准确的双向估算）
    pub fn estimate_usage(body_bytes: &hyper::body::Bytes, uri: &hyper::Uri) -> u64 {
        let (_input_tokens, _output_tokens, total_tokens) = Self::estimate_conversation_usage(body_bytes, uri);
        total_tokens
    }

    /// 基于JSON内容的更准确token估算
    fn estimate_from_json(json: &serde_json::Value, uri: &hyper::Uri) -> u64 {
        let mut total_tokens = 0u64;
        
        // 基础API调用开销
        total_tokens += 10;
        
        // 检查不同的API格式
        if let Some(obj) = json.as_object() {
            // Claude API格式
            if let Some(messages) = obj.get("messages").and_then(|v| v.as_array()) {
                for message in messages {
                    if let Some(content) = message.get("content") {
                        total_tokens += Self::estimate_content_tokens(content);
                    }
                }
            }
            
            // OpenAI ChatCompletion格式
            else if let Some(messages) = obj.get("messages").and_then(|v| v.as_array()) {
                for message in messages {
                    if let Some(content) = message.get("content").and_then(|v| v.as_str()) {
                        total_tokens += Self::estimate_text_tokens(content);
                    }
                }
            }
            
            // 单个prompt格式
            else if let Some(prompt) = obj.get("prompt").and_then(|v| v.as_str()) {
                total_tokens += Self::estimate_text_tokens(prompt);
            }
            
            // 通用内容字段
            else if let Some(input) = obj.get("input") {
                total_tokens += Self::estimate_content_tokens(input);
            }
            
            // 检查system prompt
            if let Some(system) = obj.get("system").and_then(|v| v.as_str()) {
                total_tokens += Self::estimate_text_tokens(system);
            }
            
            // 检查max_tokens设置来估算响应大小
            if let Some(max_tokens) = obj.get("max_tokens").and_then(|v| v.as_u64()) {
                // 假设平均使用50%的max_tokens
                total_tokens += max_tokens / 2;
            } else {
                // 默认响应token估算
                total_tokens += 150;
            }
        }
        
        // 路径相关的额外token
        let path_tokens = match uri.path() {
            path if path.contains("messages") || path.contains("chat") => 5,
            path if path.contains("completions") => 3,
            _ => 2,
        };
        total_tokens += path_tokens;
        
        // 合理范围限制
        total_tokens.max(15).min(100000)
    }

    /// 估算内容的token数量（支持字符串和数组格式）
    fn estimate_content_tokens(content: &serde_json::Value) -> u64 {
        match content {
            serde_json::Value::String(text) => Self::estimate_text_tokens(text),
            serde_json::Value::Array(arr) => {
                let mut tokens = 0;
                for item in arr {
                    if let Some(obj) = item.as_object() {
                        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                            tokens += Self::estimate_text_tokens(text);
                        }
                        // 图片或其他媒体类型额外成本
                        if obj.get("type").and_then(|v| v.as_str()).unwrap_or("") == "image" {
                            tokens += 85; // Claude图片token估算
                        }
                    }
                }
                tokens
            }
            _ => 20, // 其他类型的默认估算
        }
    }

    /// 基于文本内容的token估算（改进版）
    fn estimate_text_tokens(text: &str) -> u64 {
        if text.is_empty() {
            return 0;
        }
        
        // 更准确的token估算方法
        let char_count = text.chars().count() as u64;
        let word_count = text.split_whitespace().count() as u64;
        
        // 中文字符通常每个字符约1个token
        let chinese_chars = text.chars().filter(|c| {
            let cp = *c as u32;
            (0x4E00..=0x9FFF).contains(&cp) || // CJK统一汉字
            (0x3400..=0x4DBF).contains(&cp) || // CJK扩展A
            (0x20000..=0x2A6DF).contains(&cp)  // CJK扩展B
        }).count() as u64;
        
        // 英文单词平均1.3个token每个单词
        let english_tokens = ((word_count as f64) * 1.3) as u64;
        let chinese_tokens = chinese_chars;
        
        // 取较大值，加上标点和格式开销
        let base_tokens = english_tokens.max(chinese_tokens).max(char_count / 4);
        
        // 代码和JSON内容通常token密度更高
        let multiplier = if text.contains('{') && text.contains('}') ||
                           text.contains("```") ||
                           text.contains("function") ||
                           text.contains("class ") {
            1.5
        } else {
            1.0
        };
        
        ((base_tokens as f64) * multiplier) as u64
    }

    /// 回退的基础token估算方法
    fn fallback_estimation(body_bytes: &hyper::body::Bytes, uri: &hyper::Uri) -> u64 {
        let base_tokens = 15; // 增加基础开销
        let body_length = body_bytes.len() as u64;
        let path_length = uri.path().len() as u64;
        
        // 改进的估算：假设平均token密度
        let body_tokens = body_length / 3; // 稍微提高估算精度
        let path_tokens = path_length / 4;
        
        let estimated = body_tokens + path_tokens + base_tokens;
        
        estimated.max(10).min(80000)
    }
}

/// 计算字符串的显示宽度（中文字符占2个宽度）
pub fn calculate_display_width(text: &str) -> usize {
    text.chars().map(|c| {
        let cp = c as u32;
        if (0x4E00..=0x9FFF).contains(&cp) || // CJK统一汉字
           (0x3400..=0x4DBF).contains(&cp) || // CJK扩展A  
           (0x20000..=0x2A6DF).contains(&cp) || // CJK扩展B
           (0xFF00..=0xFFEF).contains(&cp) { // 全角字符
            2
        } else {
            1
        }
    }).sum()
}