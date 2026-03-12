//! LLM API通信模块
//! 
//! 实现与LLM API的通信，支持请求/响应的完整记录

use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

use crate::logger::{ApiLog, Logger};

/// LLM客户端
pub struct LlmClient {
    /// HTTP客户端
    client: Client,
    /// 配置
    config: LlmClientConfig,
    /// 日志记录器
    logger: Option<Logger>,
}

/// LLM客户端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmClientConfig {
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for LlmClientConfig {
    fn default() -> Self {
        Self {
            api_base: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        }
    }
}

/// Chat消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }
    
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }
    
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

/// Chat请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

/// Chat响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Option<Usage>,
}

/// Chat选择项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

/// Token使用情况
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// API错误响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub code: Option<String>,
}

impl LlmClient {
    /// 创建新的LLM客户端
    pub fn new(config: LlmClientConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .context("Failed to create HTTP client")?;
        
        Ok(Self {
            client,
            config,
            logger: None,
        })
    }
    
    /// 设置日志记录器
    pub fn with_logger(mut self, logger: Logger) -> Self {
        self.logger = Some(logger);
        self
    }
    
    /// 更新配置
    pub fn update_config(&mut self, config: LlmClientConfig) {
        self.config = config;
    }
    
    /// 发送Chat请求
    pub async fn chat(&self, messages: Vec<ChatMessage>) -> Result<ChatResponse> {
        let request_id = Uuid::new_v4().to_string();
        let request_time = Utc::now();
        
        let url = format!("{}/chat/completions", self.config.api_base);
        
        let chat_request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            max_tokens: Some(self.config.max_tokens),
            temperature: Some(self.config.temperature),
        };
        
        let request_body = serde_json::to_value(&chat_request)?;
        
        // 记录请求
        let mut api_log = ApiLog {
            request_id: request_id.clone(),
            request_time,
            response_time: None,
            url: url.clone(),
            method: "POST".to_string(),
            request_headers: serde_json::json!({
                "Content-Type": "application/json",
                "Authorization": "Bearer ***" // 隐藏实际key
            }),
            request_body: Some(request_body.clone()),
            response_status: None,
            response_headers: None,
            response_body: None,
            error: None,
        };
        
        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&chat_request)
            .send()
            .await;
        
        match response {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let headers = resp.headers().clone();
                
                // 转换响应头为JSON
                let headers_json: serde_json::Value = headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect::<std::collections::HashMap<_, _>>()
                    .into_iter()
                    .collect();
                
                api_log.response_status = Some(status);
                api_log.response_headers = Some(headers_json);
                
                let body_text = resp.text().await?;
                let body_json: serde_json::Value = serde_json::from_str(&body_text)
                    .unwrap_or_else(|_| serde_json::json!({ "raw": body_text }));
                
                api_log.response_body = Some(body_json.clone());
                api_log.response_time = Some(Utc::now());
                
                // 记录API调用
                if let Some(ref logger) = self.logger {
                    let _ = logger.log_api(&api_log);
                }
                
                if status >= 400 {
                    // 尝试多种错误格式
                    let error_msg = body_json["error"]["message"]
                        .as_str()
                        .or_else(|| body_json["message"].as_str())
                        .or_else(|| body_json["error"].as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| body_json.to_string());
                    anyhow::bail!("API error ({}): {}", status, error_msg);
                }
                
                let chat_response: ChatResponse = serde_json::from_value(body_json)
                    .context("Failed to parse chat response")?;
                
                Ok(chat_response)
            }
            Err(e) => {
                api_log.error = Some(e.to_string());
                api_log.response_time = Some(Utc::now());
                
                if let Some(ref logger) = self.logger {
                    let _ = logger.log_api(&api_log);
                }
                
                Err(e).context("API request failed")
            }
        }
    }
    
    /// 流式Chat请求（返回简化实现）
    pub async fn chat_stream(&self, _messages: Vec<ChatMessage>) -> Result<String> {
        // TODO: 实现流式响应
        anyhow::bail!("Streaming not yet implemented");
    }
    
    /// 测试API连接
    pub async fn test_connection(&self) -> Result<bool> {
        let messages = vec![ChatMessage::user("Hello")];
        let request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            max_tokens: Some(10),
            temperature: Some(0.1),
        };
        
        let url = format!("{}/chat/completions", self.config.api_base);
        
        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&request)
            .send()
            .await;
        
        match response {
            Ok(resp) => {
                let status = resp.status();
                Ok(status.is_success() || status.as_u16() == 401) // 401表示key无效但连接成功
            }
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_chat_message_creation() {
        let sys = ChatMessage::system("You are a helpful assistant.");
        assert_eq!(sys.role, "system");
        
        let user = ChatMessage::user("Hello");
        assert_eq!(user.role, "user");
        
        let assistant = ChatMessage::assistant("Hi there!");
        assert_eq!(assistant.role, "assistant");
    }
    
    #[test]
    fn test_chat_request_serialization() {
        let request = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage::user("Hello")],
            max_tokens: Some(100),
            temperature: Some(0.7),
        };
        
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-4"));
        assert!(json.contains("Hello"));
    }
}
