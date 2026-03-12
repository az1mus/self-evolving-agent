//! 配置管理模块
//! 
//! 管理API入口、API Key等配置项
//! 支持持久化存储

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// LLM API配置
    pub llm: LlmConfig,
    /// 日志配置
    pub log: LogConfig,
    /// 会话配置
    pub session: SessionConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            log: LogConfig::default(),
            session: SessionConfig::default(),
        }
    }
}

/// LLM API配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// API基础URL
    pub api_base: String,
    /// API Key
    pub api_key: String,
    /// 模型名称
    pub model: String,
    /// 最大tokens
    pub max_tokens: u32,
    /// 温度参数
    pub temperature: f32,
}

impl Default for LlmConfig {
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

/// 日志配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// 日志级别：error, warning, info, debug
    pub level: String,
    /// 是否记录完整API请求/响应
    pub log_api_details: bool,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            log_api_details: false,
        }
    }
}

/// 会话配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// 自动保存会话
    pub auto_save: bool,
    /// 最大历史消息数
    pub max_history: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            auto_save: true,
            max_history: 100,
        }
    }
}

/// 配置管理器
pub struct ConfigManager {
    /// 配置文件路径
    config_path: PathBuf,
    /// 当前配置
    config: Config,
}

impl ConfigManager {
    /// 创建新的配置管理器
    pub fn new() -> Result<Self> {
        let config_dir = Self::get_config_dir()?;
        fs::create_dir_all(&config_dir)?;
        
        let config_path = config_dir.join("config.toml");
        let config = if config_path.exists() {
            Self::load_config(&config_path)?
        } else {
            Config::default()
        };
        
        Ok(Self {
            config_path,
            config,
        })
    }
    
    /// 获取配置目录
    fn get_config_dir() -> Result<PathBuf> {
        let project_dirs = ProjectDirs::from("com", "az1mus", "self-evolving-agent")
            .context("Failed to get project directories")?;
        Ok(project_dirs.config_dir().to_path_buf())
    }
    
    /// 加载配置
    fn load_config(path: &PathBuf) -> Result<Config> {
        let content = fs::read_to_string(path)
            .context("Failed to read config file")?;
        let config: Config = toml::from_str(&content)
            .context("Failed to parse config file")?;
        Ok(config)
    }
    
    /// 保存配置
    pub fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(&self.config)
            .context("Failed to serialize config")?;
        fs::write(&self.config_path, content)
            .context("Failed to write config file")?;
        Ok(())
    }
    
    /// 获取当前配置
    pub fn config(&self) -> &Config {
        &self.config
    }
    
    /// 获取可变配置引用
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }
    
    /// 更新LLM配置
    pub fn update_llm_config(&mut self, api_base: Option<String>, api_key: Option<String>, model: Option<String>) {
        if let Some(base) = api_base {
            self.config.llm.api_base = base;
        }
        if let Some(key) = api_key {
            self.config.llm.api_key = key;
        }
        if let Some(m) = model {
            self.config.llm.model = m;
        }
    }
    
    /// 更新日志配置
    pub fn update_log_config(&mut self, level: Option<String>, log_api_details: Option<bool>) {
        if let Some(l) = level {
            self.config.log.level = l;
        }
        if let Some(lad) = log_api_details {
            self.config.log.log_api_details = lad;
        }
    }
    
    /// 获取配置文件路径
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }
    
    /// 重置为默认配置
    pub fn reset_to_default(&mut self) {
        self.config = Config::default();
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new().expect("Failed to create ConfigManager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.llm.api_base, "https://api.openai.com/v1");
        assert_eq!(config.log.level, "info");
    }
    
    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.llm.model, parsed.llm.model);
    }
}
