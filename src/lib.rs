//! Self-Evolving Agent
//!
//! 一个能够自我进化的智能代理系统，支持工具进化与提示词进化。
//!
//! 架构：
//! - AgentDefinition: Agent 定义/模板，独立存储，可复用
//! - Session: 运行时容器，包含消息历史
//! - AgentInstance: Agent 实例，在 Session 中运行

pub mod agent;
pub mod cli;
pub mod config;
pub mod gateway;
pub mod llm;
pub mod logger;
pub mod session;

pub use agent::{
    AgentDefinition, AgentDefinitionId, AgentDefinitionInfo,
    AgentInstance, AgentInstanceId, AgentInstanceInfo,
    AgentInput, AgentOutput,
};
pub use cli::CliApp;
pub use config::ConfigManager;
pub use gateway::{Gateway, GatewayEvent};
pub use llm::LlmClient;
pub use logger::{LogLevel, Logger};
pub use session::{Session, SessionId, SessionInfo, SessionManager};
