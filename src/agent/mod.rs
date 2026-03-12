//! Agent 模块
//!
//! 实现对话代理的核心逻辑
//! Agent 分为两部分：
//! - AgentDefinition: Agent 定义/模板，独立存储，可复用
//! - AgentInstance: Agent 实例，在 Session 中运行

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::llm::{ChatMessage, ChatResponse, LlmClient, LlmClientConfig};
use crate::logger::Logger;
use crate::session::SessionId;

/// Agent 定义 ID
pub type AgentDefinitionId = String;

/// Agent 实例 ID
pub type AgentInstanceId = String;

// ==================== Agent Definition ====================

/// Agent 定义/模板
/// 独立存储，可被多个 Session 复用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    /// 定义 ID
    pub id: AgentDefinitionId,
    /// 名称
    pub name: String,
    /// 描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 系统提示词
    pub system_prompt: Option<String>,
    /// LLM 配置
    pub llm_config: LlmClientConfig,
    /// 创建时间
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 更新时间
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl AgentDefinition {
    /// 创建新的 Agent 定义
    pub fn new(name: String, system_prompt: Option<String>, llm_config: LlmClientConfig) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            description: None,
            system_prompt,
            llm_config,
            created_at: now,
            updated_at: now,
        }
    }

    /// 设置描述
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// 更新系统提示词
    pub fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = Some(prompt);
        self.updated_at = chrono::Utc::now();
    }

    /// 更新 LLM 配置
    pub fn set_llm_config(&mut self, config: LlmClientConfig) {
        self.llm_config = config;
        self.updated_at = chrono::Utc::now();
    }
}

// ==================== Agent Instance ====================

/// Agent 实例
/// 在 Session 中运行，引用 AgentDefinition
pub struct AgentInstance {
    /// 实例 ID
    id: AgentInstanceId,
    /// 引用的 Agent 定义 ID
    definition_id: AgentDefinitionId,
    /// 所属 Session ID
    session_id: SessionId,
    /// LLM 客户端（运行时）
    llm_client: LlmClient,
    /// 日志记录器
    logger: Option<Arc<Logger>>,
    /// 是否活跃
    active: bool,
}

impl AgentInstance {
    /// 创建新的 Agent 实例
    pub fn new(
        id: AgentInstanceId,
        definition_id: AgentDefinitionId,
        session_id: SessionId,
        llm_config: LlmClientConfig,
    ) -> Result<Self> {
        let llm_client = LlmClient::new(llm_config)?;

        Ok(Self {
            id,
            definition_id,
            session_id,
            llm_client,
            logger: None,
            active: true,
        })
    }

    /// 设置日志记录器
    pub fn with_logger(mut self, logger: Arc<Logger>) -> Self {
        self.llm_client = self.llm_client.with_logger((*logger).clone());
        self.logger = Some(logger);
        self
    }

    /// 获取实例 ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 获取定义 ID
    pub fn definition_id(&self) -> &str {
        &self.definition_id
    }

    /// 获取 Session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// 是否活跃
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// 停用
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// 激活
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// 处理消息
    /// messages: Session 的消息历史（不含系统提示词）
    /// system_prompt: Agent 定义的系统提示词
    pub async fn process(
        &mut self,
        messages: Vec<ChatMessage>,
        system_prompt: Option<&str>,
    ) -> AgentOutput {
        // 记录
        if let Some(ref logger) = self.logger {
            let _ = logger.debug("agent", "Processing messages", None);
        }

        // 构建完整消息：系统提示词 + 会话消息
        let mut full_messages = Vec::new();

        if let Some(prompt) = system_prompt {
            full_messages.push(ChatMessage::system(prompt));
        }

        full_messages.extend(messages);

        // 调用 LLM
        match self.llm_client.chat(full_messages).await {
            Ok(response) => self.handle_success(response),
            Err(e) => self.handle_error(e),
        }
    }

    /// 处理成功响应
    fn handle_success(&mut self, response: ChatResponse) -> AgentOutput {
        if let Some(choice) = response.choices.first() {
            let content = choice.message.content.clone();

            if let Some(ref logger) = self.logger {
                let _ = logger.info(
                    "agent",
                    &format!("Response generated, tokens: {:?}", response.usage),
                );
            }

            AgentOutput {
                content,
                output_type: OutputType::Response,
                usage: response.usage.map(TokenUsage::from),
                success: true,
                error: None,
            }
        } else {
            self.handle_error(anyhow::anyhow!("No response from LLM"))
        }
    }

    /// 处理错误
    fn handle_error(&mut self, e: anyhow::Error) -> AgentOutput {
        if let Some(ref logger) = self.logger {
            let _ = logger.error("agent", &format!("Error: {}", e));
        }

        AgentOutput {
            content: String::new(),
            output_type: OutputType::Error,
            usage: None,
            success: false,
            error: Some(e.to_string()),
        }
    }

    /// 更新 LLM 配置
    pub fn update_llm_config(&mut self, config: LlmClientConfig) {
        self.llm_client.update_config(config);
    }
}

// ==================== Input/Output ====================

/// Agent 输入消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInput {
    /// 用户输入内容
    pub content: String,
    /// 可选的元数据
    pub metadata: Option<InputMetadata>,
}

/// 输入元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputMetadata {
    /// 来源标识
    pub source: String,
    /// 时间戳
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
    /// 额外参数
    pub extra: Option<serde_json::Value>,
}

impl Default for InputMetadata {
    fn default() -> Self {
        Self {
            source: "cli".to_string(),
            timestamp: Some(chrono::Utc::now()),
            extra: None,
        }
    }
}

/// Agent 输出消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    /// 回复内容
    pub content: String,
    /// 输出类型
    pub output_type: OutputType,
    /// Token 使用情况
    pub usage: Option<TokenUsage>,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    pub error: Option<String>,
}

/// 输出类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputType {
    Response,
    System,
    Error,
}

/// Token 使用统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl From<crate::llm::Usage> for TokenUsage {
    fn from(usage: crate::llm::Usage) -> Self {
        Self {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        }
    }
}

// ==================== Agent Info ====================

/// Agent 定义摘要信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinitionInfo {
    pub id: AgentDefinitionId,
    pub name: String,
    pub description: Option<String>,
    pub has_system_prompt: bool,
}

/// Agent 实例摘要信息
#[derive(Debug, Clone)]
pub struct AgentInstanceInfo {
    pub id: AgentInstanceId,
    pub definition_id: AgentDefinitionId,
    pub definition_name: String,
    pub is_current: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_definition_creation() {
        let config = LlmClientConfig::default();
        let def = AgentDefinition::new(
            "Test Agent".to_string(),
            Some("You are helpful.".to_string()),
            config,
        );

        assert_eq!(def.name, "Test Agent");
        assert!(def.system_prompt.is_some());
    }

    #[test]
    fn test_agent_input_serialization() {
        let input = AgentInput {
            content: "Hello".to_string(),
            metadata: None,
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("Hello"));
    }
}