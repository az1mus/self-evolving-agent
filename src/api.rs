/// API client module for SEA (Self-Evolved Agent)
/// Handles communication with LLM providers using OpenAI-compatible API
use anyhow::{Context, Result};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};

use crate::config::{config_manager, Config};
use crate::tools::Tool;

/// Represents a message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn new(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_name(role: &str, content: &str, name: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
            name: Some(name.to_string()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_tool_calls(role: &str, content: &str, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
            name: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_tool_calls_string(role: &str, content: String, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: role.to_string(),
            content,
            name: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    pub fn with_tool_result(role: &str, content: &str, tool_call_id: &str, name: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
            name: Some(name.to_string()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
        }
    }
}

/// Represents a tool call from the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_tool_type")]
    pub tool_type: String,
    pub function: FunctionCall,
}

fn default_tool_type() -> String {
    "function".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Represents a response from the chat API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub usage: Option<Usage>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// API error types
#[derive(Debug, thiserror::Error)]
pub enum APIError {
    #[error("HTTP error {0}: {1}")]
    HttpError(u16, String),
}

/// Client for communicating with LLM APIs
pub struct APIClient {
    config: Config,
    client: Client,
}

impl APIClient {
    /// Create a new API client with the given configuration
    pub fn new(config: Config) -> Result<Self> {
        let client = Client::builder()
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    "Authorization",
                    format!("Bearer {}", config.api_key)
                        .parse()
                        .with_context(|| "Invalid API key")?,
                );
                headers.insert(
                    "Content-Type",
                    "application/json".parse().with_context(|| "Invalid content type")?,
                );
                headers
            })
            .timeout(std::time::Duration::from_secs(config.timeout))
            .build()
            .with_context(|| "Failed to create HTTP client")?;

        Ok(Self { config, client })
    }

    /// Send a chat completion request to the API
    pub async fn chat_completion(
        &self,
        messages: &[Message],
        tools: Option<&[&Box<dyn Tool>]>,
        stream: bool,
    ) -> Result<ChatResponse> {
        let mut payload = serde_json::json!({
            "model": self.config.model,
            "messages": messages.iter().map(|m| {
                let mut msg_json = serde_json::json!({
                    "role": m.role,
                    "content": m.content
                });
                if let Some(ref name) = m.name {
                    msg_json["name"] = serde_json::json!(name);
                }
                if let Some(ref tool_calls) = m.tool_calls {
                    msg_json["tool_calls"] = serde_json::json!(tool_calls);
                }
                if let Some(ref tool_call_id) = m.tool_call_id {
                    msg_json["tool_call_id"] = serde_json::json!(tool_call_id);
                }
                msg_json
            }).collect::<Vec<_>>(),
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
        });

        if let Some(tools) = tools {
            let tools_json: Vec<_> = tools
                .iter()
                .map(|tool| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name(),
                            "description": tool.description(),
                            "parameters": tool.parameters()
                        }
                    })
                })
                .collect();
            payload["tools"] = serde_json::Value::Array(tools_json);
            log::debug!("Sending {} tools to API", tools.len());
        }

        payload["stream"] = serde_json::Value::Bool(stream);

        log::debug!("Sending chat completion request to {}", self.config.base_url);
        log::trace!("Request payload: {:?}", payload);

        let url = format!("{}/chat/completions", self.config.base_url);
        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .with_context(|| "Failed to send chat completion request")?;

        self.parse_chat_response(response).await
    }

    /// Parse a chat completion response
    async fn parse_chat_response(&self, response: Response) -> Result<ChatResponse> {
        let status = response.status();

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            log::error!("HTTP error {}: {}", status, error_text);
            return Err(APIError::HttpError(status.as_u16(), error_text).into());
        }

        let data: serde_json::Value = response
            .json()
            .await
            .with_context(|| "Failed to parse response JSON")?;

        log::trace!("Response data: {:?}", data);

        // Extract the response
        let choice = data["choices"][0]["message"]
            .as_object()
            .with_context(|| "Invalid response format: missing choices[0].message")?;

        let content = choice.get("content").and_then(|v| v.as_str()).map(String::from);

        // Check for tool calls
        let tool_calls = if let Some(tool_calls_val) = choice.get("tool_calls") {
            if let Some(arr) = tool_calls_val.as_array() {
                log::info!("API returned {} tool call(s)", arr.len());
                let calls: Result<Vec<ToolCall>> = arr
                    .iter()
                    .filter_map(|call| {
                        call.get("function").map(|func| {
                            let name = func["name"].as_str().unwrap_or("").to_string();
                            let arguments = func["arguments"].as_str().unwrap_or("{}").to_string();
                            Ok(ToolCall {
                                id: call["id"].as_str().unwrap_or("").to_string(),
                                tool_type: "function".to_string(),
                                function: FunctionCall { name, arguments },
                            })
                        })
                    })
                    .collect();
                Some(calls?)
            } else {
                None
            }
        } else {
            log::debug!("API returned no tool calls, content only");
            None
        };

        let usage = data.get("usage").map(|u| Usage {
            prompt_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: u["total_tokens"].as_u64().unwrap_or(0) as u32,
        });

        let model = data.get("model").and_then(|v| v.as_str()).map(String::from);

        Ok(ChatResponse {
            content,
            tool_calls,
            usage,
            model,
        })
    }

    /// List available models from the API
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/models", self.config.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| "Failed to list models")?;

        let data: serde_json::Value = response
            .json()
            .await
            .with_context(|| "Failed to parse models response")?;

        let models = data
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }
}

/// Create an API client using the global configuration
pub async fn create_client_from_config() -> Result<APIClient> {
    let mut config_mgr = config_manager()?;
    let config = config_mgr.load_config()?.clone();
    APIClient::new(config)
}
