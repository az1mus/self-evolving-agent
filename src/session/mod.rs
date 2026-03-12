//! Session管理模块
//!
//! 实现会话管理，包括会话创建、保存、加载和历史记录
//! Session 是运行时容器，包含消息历史和 Agent 实例引用
//! Agent 定义独立存储，Session 通过 AgentInstance 引用

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::agent::AgentInstanceId;
use crate::llm::ChatMessage;

/// 会话ID
pub type SessionId = String;

/// 会话
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// 会话ID
    pub id: SessionId,
    /// 会话名称
    pub name: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
    /// 消息历史
    pub messages: VecDeque<SessionMessage>,
    /// Agent 实例 ID 列表
    pub agent_instance_ids: HashSet<AgentInstanceId>,
    /// 元数据
    pub metadata: serde_json::Value,
}

impl Session {
    /// 创建新会话
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            created_at: now,
            updated_at: now,
            messages: VecDeque::new(),
            agent_instance_ids: HashSet::new(),
            metadata: serde_json::json!({}),
        }
    }

    /// 添加 Agent 实例到会话
    pub fn add_agent_instance(&mut self, instance_id: AgentInstanceId) {
        self.agent_instance_ids.insert(instance_id);
        self.updated_at = Utc::now();
    }

    /// 从会话移除 Agent 实例
    pub fn remove_agent_instance(&mut self, instance_id: &str) -> bool {
        let removed = self.agent_instance_ids.remove(instance_id);
        if removed {
            self.updated_at = Utc::now();
        }
        removed
    }

    /// 检查是否包含指定 Agent 实例
    pub fn has_agent_instance(&self, instance_id: &str) -> bool {
        self.agent_instance_ids.contains(instance_id)
    }

    /// 获取所有 Agent 实例 ID
    pub fn agent_instance_ids(&self) -> &HashSet<AgentInstanceId> {
        &self.agent_instance_ids
    }

    /// 获取 Agent 实例数量
    pub fn agent_count(&self) -> usize {
        self.agent_instance_ids.len()
    }

    /// 添加用户消息
    pub fn add_user_message(&mut self, content: impl Into<String>, agent_instance_id: Option<AgentInstanceId>) {
        self.messages.push_back(SessionMessage {
            role: MessageRole::User,
            content: content.into(),
            timestamp: Utc::now(),
            agent_instance_id,
            metadata: None,
        });
        self.updated_at = Utc::now();
    }

    /// 添加助手消息
    pub fn add_assistant_message(&mut self, content: impl Into<String>, agent_instance_id: Option<AgentInstanceId>) {
        self.messages.push_back(SessionMessage {
            role: MessageRole::Assistant,
            content: content.into(),
            timestamp: Utc::now(),
            agent_instance_id,
            metadata: None,
        });
        self.updated_at = Utc::now();
    }

    /// 转换为LLM API消息格式（不含系统提示词，由 Agent 添加）
    pub fn to_chat_messages(&self) -> Vec<ChatMessage> {
        self.messages
            .iter()
            .map(|msg| match msg.role {
                MessageRole::User => ChatMessage::user(&msg.content),
                MessageRole::Assistant => ChatMessage::assistant(&msg.content),
                MessageRole::System => ChatMessage::system(&msg.content),
            })
            .collect()
    }

    /// 获取最后N条消息
    pub fn get_recent_messages(&self, n: usize) -> Vec<&SessionMessage> {
        self.messages.iter().rev().take(n).rev().collect()
    }

    /// 清空消息历史
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.updated_at = Utc::now();
    }

    /// 获取消息数量
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
}

/// 会话消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    /// 角色
    pub role: MessageRole,
    /// 内容
    pub content: String,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 关联的 Agent 实例 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_instance_id: Option<AgentInstanceId>,
    /// 元数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// 消息角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

/// 会话管理器（用于持久化存储）
pub struct SessionManager {
    /// 会话存储目录
    session_dir: PathBuf,
    /// 最大历史消息数
    max_history: usize,
}

impl SessionManager {
    /// 创建新的会话管理器
    pub fn new(session_dir: PathBuf, max_history: usize) -> Result<Self> {
        fs::create_dir_all(&session_dir)
            .context("Failed to create session directory")?;

        Ok(Self {
            session_dir,
            max_history,
        })
    }

    /// 保存会话
    pub fn save_session(&self, session: &Session) -> Result<()> {
        let file_path = self.session_dir.join(format!("{}.json", session.id));
        let json = serde_json::to_string_pretty(session)
            .context("Failed to serialize session")?;
        fs::write(&file_path, json)
            .context("Failed to write session file")?;
        Ok(())
    }

    /// 加载会话
    pub fn load_session(&self, id: &str) -> Result<Session> {
        let file_path = self.session_dir.join(format!("{}.json", id));
        let json = fs::read_to_string(&file_path)
            .context("Failed to read session file")?;
        let session: Session = serde_json::from_str(&json)
            .context("Failed to parse session file")?;
        Ok(session)
    }

    /// 列出所有会话
    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let mut sessions = Vec::new();

        for entry in fs::read_dir(&self.session_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "json") {
                if let Ok(json) = fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<Session>(&json) {
                        sessions.push(SessionInfo {
                            id: session.id,
                            name: session.name,
                            created_at: session.created_at,
                            updated_at: session.updated_at,
                            message_count: session.messages.len(),
                            agent_count: session.agent_instance_ids.len(),
                        });
                    }
                }
            }
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// 删除会话
    pub fn delete_session(&self, id: &str) -> Result<()> {
        let file_path = self.session_dir.join(format!("{}.json", id));
        fs::remove_file(&file_path)
            .context("Failed to delete session file")?;
        Ok(())
    }

    /// 裁剪历史消息
    pub fn trim_history(&self, session: &mut Session) {
        while session.messages.len() > self.max_history {
            session.messages.pop_front();
        }
    }

    /// 获取存储目录
    pub fn session_dir(&self) -> &PathBuf {
        &self.session_dir
    }
}

/// 会话摘要信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub agent_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = Session::new("Test Session");
        assert_eq!(session.name, "Test Session");
        assert!(session.messages.is_empty());
        assert!(session.agent_instance_ids.is_empty());
    }

    #[test]
    fn test_add_messages() {
        let mut session = Session::new("Test");
        session.add_user_message("Hello", None);
        session.add_assistant_message("Hi there!", Some("instance-1".to_string()));

        assert_eq!(session.message_count(), 2);
    }

    #[test]
    fn test_agent_instance_management() {
        let mut session = Session::new("Test");

        session.add_agent_instance("instance-1".to_string());
        session.add_agent_instance("instance-2".to_string());

        assert_eq!(session.agent_count(), 2);
        assert!(session.has_agent_instance("instance-1"));

        session.remove_agent_instance("instance-1");
        assert_eq!(session.agent_count(), 1);
        assert!(!session.has_agent_instance("instance-1"));
    }
}