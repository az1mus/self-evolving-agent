/// Session management module for SEA (Self-Evolved Agent)
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::api::{Message, ToolCall};

/// Represents a conversation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
    pub messages: Vec<SessionMessage>,
    pub metadata: serde_json::Map<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_requests: Option<Vec<ApiRequestInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallInfo>>,
}

/// API request information for tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequestInfo {
    pub timestamp: DateTime<Local>,
    pub model: String,
    pub request_type: String,
    pub messages_count: usize,
    pub tools_used: Option<Vec<String>>,
    pub response_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Tool call information for tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub timestamp: DateTime<Local>,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub result: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<u64>,
}

/// Serializable message format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl From<Message> for SessionMessage {
    fn from(msg: Message) -> Self {
        Self {
            role: msg.role,
            content: msg.content,
            name: msg.name,
            tool_calls: msg.tool_calls,
            tool_call_id: msg.tool_call_id,
        }
    }
}

impl From<SessionMessage> for Message {
    fn from(msg: SessionMessage) -> Self {
        Message {
            role: msg.role,
            content: msg.content,
            name: msg.name,
            tool_calls: msg.tool_calls,
            tool_call_id: msg.tool_call_id,
        }
    }
}

impl Session {
    /// Create a new session
    pub fn new(name: &str, metadata: Option<serde_json::Map<String, serde_json::Value>>) -> Self {
        let now = Local::now();
        let session_id = format!(
            "sess_{}",
            now.format("%Y%m%d_%H%M%S_%f")
        );

        Self {
            id: session_id,
            name: name.to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            metadata: metadata.unwrap_or_default(),
            api_requests: Some(Vec::new()),
            tool_calls: Some(Vec::new()),
        }
    }

    /// Record an API request
    pub fn record_api_request(&mut self, info: ApiRequestInfo) {
        if let Some(ref mut requests) = self.api_requests {
            requests.push(info);
            self.updated_at = Local::now();
        }
    }

    /// Record a tool call
    pub fn record_tool_call(&mut self, info: ToolCallInfo) {
        if let Some(ref mut calls) = self.tool_calls {
            calls.push(info);
            self.updated_at = Local::now();
        }
    }

    /// Get API request history
    pub fn get_api_requests(&self) -> Option<&Vec<ApiRequestInfo>> {
        self.api_requests.as_ref()
    }

    /// Get tool call history
    pub fn get_tool_calls(&self) -> Option<&Vec<ToolCallInfo>> {
        self.tool_calls.as_ref()
    }

    /// Convert session to dictionary for serialization
    pub fn to_dict(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }

    /// Create session from dictionary
    pub fn from_dict(data: &serde_json::Value) -> Result<Self> {
        Ok(serde_json::from_value(data.clone())?)
    }
}

/// Manages conversation sessions
pub struct SessionManager {
    #[allow(dead_code)]
    config_dir: PathBuf,
    sessions_dir: PathBuf,
    active_session: Option<Session>,
    history_size: usize,
}

impl SessionManager {
    /// Create a new SessionManager
    pub fn new(config_dir: &Path, history_size: Option<usize>) -> Result<Self> {
        let sessions_dir = config_dir.join("sessions");

        // Create sessions directory if it doesn't exist
        if !sessions_dir.exists() {
            fs::create_dir_all(&sessions_dir)
                .with_context(|| format!("Failed to create sessions directory: {:?}", sessions_dir))?;
        }

        Ok(Self {
            config_dir: config_dir.to_path_buf(),
            sessions_dir,
            active_session: None,
            history_size: history_size.unwrap_or(100),
        })
    }

    /// Create a new session
    pub fn create_session(
        &mut self,
        name: &str,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<&Session> {
        let session = Session::new(name, metadata);
        self.save_session(&session)?;
        self.active_session = Some(session);
        log::info!(
            "Created new session: {} ({})",
            self.active_session.as_ref().unwrap().name,
            self.active_session.as_ref().unwrap().id
        );
        Ok(self.active_session.as_ref().unwrap())
    }

    /// Load a session by ID
    #[allow(dead_code)]
    pub fn load_session(&mut self, session_id: &str) -> Result<Option<&Session>> {
        let session_file = self.sessions_dir.join(format!("{}.json", session_id));

        if !session_file.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&session_file)
            .with_context(|| format!("Failed to read session file: {:?}", session_file))?;

        let data: serde_json::Value = serde_json::from_str(&content)?;
        let session = Session::from_dict(&data)?;

        self.active_session = Some(session);
        log::info!(
            "Loaded session: {} ({})",
            self.active_session.as_ref().unwrap().name,
            self.active_session.as_ref().unwrap().id
        );
        Ok(self.active_session.as_ref())
    }

    /// Save a session to disk
    pub fn save_session(&self, session: &Session) -> Result<()> {
        let session_file = self.sessions_dir.join(format!("{}.json", session.id));

        let data = session.to_dict()?;
        let content = serde_json::to_string_pretty(&data)?;

        fs::write(&session_file, content)
            .with_context(|| format!("Failed to write session file: {:?}", session_file))?;

        log::info!("Saved session: {} ({})", session.name, session.id);
        Ok(())
    }

    /// List all available sessions
    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();

        for entry in fs::read_dir(&self.sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Ok(session) = Session::from_dict(&data) {
                                sessions.push(session);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Error loading session file {:?}: {}", path, e);
                    }
                }
            }
        }

        // Sort by most recently updated
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// Delete a session by ID
    pub fn delete_session(&mut self, session_id: &str) -> Result<bool> {
        let session_file = self.sessions_dir.join(format!("{}.json", session_id));

        if session_file.exists() {
            fs::remove_file(&session_file)?;

            if self
                .active_session
                .as_ref()
                .map(|s| s.id == session_id)
                .unwrap_or(false)
            {
                self.active_session = None;
            }

            log::info!("Deleted session: {}", session_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Add a message to the active session
    pub fn add_message(&mut self, message: Message) -> Result<()> {
        {
            let session = self
                .active_session
                .as_mut()
                .context("No active session")?;

            session.messages.push(message.into());

            // Limit history size
            if session.messages.len() > self.history_size {
                session.messages = session.messages[session.messages.len() - self.history_size..].to_vec();
            }
        }

        let session = self.active_session.as_ref().context("No active session")?;
        self.save_session(session)
    }

    /// Clear the active session's messages
    #[allow(dead_code)]
    pub fn clear_session(&mut self) -> Result<()> {
        {
            let session = self
                .active_session
                .as_mut()
                .context("No active session")?;

            session.messages.clear();
        }

        let session = self.active_session.as_ref().context("No active session")?;
        self.save_session(session)
    }

    /// Get the active session
    pub fn get_active_session(&self) -> Option<&Session> {
        self.active_session.as_ref()
    }

    /// Get the active session mutably
    #[allow(dead_code)]
    pub fn get_active_session_mut(&mut self) -> Option<&mut Session> {
        self.active_session.as_mut()
    }

    /// Get messages from active session as API Message format
    pub fn get_messages(&self) -> Option<Vec<Message>> {
        self.active_session.as_ref().map(|session| {
            session
                .messages
                .iter()
                .cloned()
                .map(Message::from)
                .collect()
        })
    }

    /// Record an API request in the active session
    pub fn record_api_request(
        &mut self,
        model: &str,
        request_type: &str,
        messages_count: usize,
        tools_used: Option<Vec<String>>,
        response_tokens: Option<u32>,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<()> {
        let session = self
            .active_session
            .as_mut()
            .context("No active session")?;

        let info = ApiRequestInfo {
            timestamp: Local::now(),
            model: model.to_string(),
            request_type: request_type.to_string(),
            messages_count,
            tools_used,
            response_tokens,
            metadata,
        };

        session.record_api_request(info);
        
        // Clone session info before saving to avoid borrow issues
        let session_clone = session.clone();
        self.active_session = Some(session_clone);
        
        let session = self.active_session.as_ref().context("No active session")?;
        self.save_session(session)
    }

    /// Record a tool call in the active session
    pub fn record_tool_call(
        &mut self,
        tool_name: &str,
        arguments: serde_json::Value,
        result: &str,
        success: bool,
        error_message: Option<String>,
        execution_time_ms: Option<u64>,
    ) -> Result<()> {
        let session = self
            .active_session
            .as_mut()
            .context("No active session")?;

        let info = ToolCallInfo {
            timestamp: Local::now(),
            tool_name: tool_name.to_string(),
            arguments,
            result: result.to_string(),
            success,
            error_message,
            execution_time_ms,
        };

        session.record_tool_call(info);
        
        // Clone session info before saving to avoid borrow issues
        let session_clone = session.clone();
        self.active_session = Some(session_clone);
        
        let session = self.active_session.as_ref().context("No active session")?;
        self.save_session(session)
    }

    /// Get API request history from active session
    pub fn get_api_requests(&self) -> Option<&Vec<ApiRequestInfo>> {
        self.active_session.as_ref().and_then(|s| s.get_api_requests())
    }

    /// Get tool call history from active session
    pub fn get_tool_calls(&self) -> Option<&Vec<ToolCallInfo>> {
        self.active_session.as_ref().and_then(|s| s.get_tool_calls())
    }
}
