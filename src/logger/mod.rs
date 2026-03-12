//! 日志系统模块
//!
//! 支持多级日志：info、debug、warning、error
//! debug级别包含完整的API请求/响应记录

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warning,
    Info,
    Debug,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Warning => write!(f, "WARN"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Debug => write!(f, "DEBUG"),
        }
    }
}

/// 日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 日志级别
    pub level: LogLevel,
    /// 模块名
    pub module: String,
    /// 消息
    pub message: String,
    /// 附加数据（用于API请求/响应记录）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// API调用记录（用于debug级别）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiLog {
    /// 请求ID
    pub request_id: String,
    /// 请求时间
    pub request_time: DateTime<Utc>,
    /// 响应时间
    pub response_time: Option<DateTime<Utc>>,
    /// 请求URL
    pub url: String,
    /// 请求方法
    pub method: String,
    /// 请求头
    pub request_headers: serde_json::Value,
    /// 请求体
    pub request_body: Option<serde_json::Value>,
    /// 响应状态码
    pub response_status: Option<u16>,
    /// 响应头
    pub response_headers: Option<serde_json::Value>,
    /// 响应体
    pub response_body: Option<serde_json::Value>,
    /// 错误信息
    pub error: Option<String>,
}

/// 日志管理器内部状态
struct LoggerInner {
    /// 日志目录
    log_dir: PathBuf,
    /// 当前日志文件
    current_file: PathBuf,
    /// 当前配置的日志级别
    level: LogLevel,
}

/// 日志管理器（线程安全）
#[derive(Clone)]
pub struct Logger {
    inner: Arc<Mutex<LoggerInner>>,
}

impl Logger {
    /// 创建新的日志管理器
    pub fn new(log_dir: PathBuf, level: LogLevel) -> Result<Self> {
        fs::create_dir_all(&log_dir)?;

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let current_file = log_dir.join(format!("{}.log", today));

        Ok(Self {
            inner: Arc::new(Mutex::new(LoggerInner {
                log_dir,
                current_file,
                level,
            })),
        })
    }

    /// 获取日志目录
    pub fn log_dir(&self) -> PathBuf {
        self.inner.lock().unwrap().log_dir.clone()
    }

    /// 获取当前日志级别
    pub fn level(&self) -> LogLevel {
        self.inner.lock().unwrap().level
    }

    /// 设置日志级别
    pub fn set_level(&self, level: LogLevel) {
        self.inner.lock().unwrap().level = level;
    }

    /// 写入日志条目
    pub fn log(&self, entry: &LogEntry) -> Result<()> {
        let inner = self.inner.lock().unwrap();
        
        if entry.level as u8 > inner.level as u8 {
            return Ok(());
        }

        let json = serde_json::to_string(entry)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&inner.current_file)?;

        writeln!(file, "{}", json)?;

        // 控制台输出
        println!(
            "[{} {} {}] {}",
            entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
            entry.level,
            entry.module,
            entry.message
        );

        Ok(())
    }

    /// 记录info级别日志
    pub fn info(&self, module: &str, message: &str) -> Result<()> {
        self.log(&LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            module: module.to_string(),
            message: message.to_string(),
            data: None,
        })
    }

    /// 记录warning级别日志
    pub fn warning(&self, module: &str, message: &str) -> Result<()> {
        self.log(&LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Warning,
            module: module.to_string(),
            message: message.to_string(),
            data: None,
        })
    }

    /// 记录error级别日志
    pub fn error(&self, module: &str, message: &str) -> Result<()> {
        self.log(&LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Error,
            module: module.to_string(),
            message: message.to_string(),
            data: None,
        })
    }

    /// 记录debug级别日志（可包含数据）
    pub fn debug(&self, module: &str, message: &str, data: Option<serde_json::Value>) -> Result<()> {
        self.log(&LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Debug,
            module: module.to_string(),
            message: message.to_string(),
            data,
        })
    }

    /// 记录API调用（debug级别专用）
    pub fn log_api(&self, api_log: &ApiLog) -> Result<()> {
        let data = serde_json::to_value(api_log)?;
        self.debug(
            "llm::api",
            &format!("API调用: {} {}", api_log.method, api_log.url),
            Some(data),
        )
    }

    /// 滚动日志文件（按日期）
    pub fn rotate_if_needed(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let new_file = inner.log_dir.join(format!("{}.log", today));

        if inner.current_file != new_file {
            inner.current_file = new_file;
        }

        Ok(())
    }
}

/// 初始化全局日志订阅器
pub fn init_tracing(level: LogLevel) {
    let filter = match level {
        LogLevel::Error => "error",
        LogLevel::Warning => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
    };

    let subscriber = Registry::default()
        .with(EnvFilter::new(filter))
        .with(fmt::layer().with_target(false));

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_log_levels() {
        assert!(LogLevel::Error as u8 <= LogLevel::Warning as u8);
        assert!(LogLevel::Warning as u8 <= LogLevel::Info as u8);
        assert!(LogLevel::Info as u8 <= LogLevel::Debug as u8);
    }

    #[test]
    fn test_logger_creation() {
        let dir = tempdir().unwrap();
        let logger = Logger::new(dir.path().to_path_buf(), LogLevel::Info);
        assert!(logger.is_ok());
    }
}
