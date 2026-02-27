/// Configuration management module for SEA (Self-Evolved Agent)
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Default configuration directory
const DEFAULT_CONFIG_DIR: &str = ".sea";
/// Default configuration file name
const DEFAULT_CONFIG_FILE: &str = "config.yaml";

/// SEA configuration data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub timeout: u64,
    pub history_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4-turbo".to_string(),
            temperature: 0.7,
            max_tokens: 2048,
            timeout: 30,
            history_size: 100,
        }
    }
}

/// Legacy configuration format wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyConfig {
    settings: Config,
}

/// Manages SEA configuration
pub struct ConfigManager {
    config_file: PathBuf,
    config_dir: PathBuf,
    config: Option<Config>,
}

impl ConfigManager {
    /// Create a new ConfigManager with the given config file path
    pub fn new(config_file: Option<PathBuf>) -> Result<Self> {
        let config_file = config_file.unwrap_or_else(|| {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(DEFAULT_CONFIG_DIR).join(DEFAULT_CONFIG_FILE)
        });

        let config_dir = config_file.parent().unwrap_or(Path::new(".")).to_path_buf();

        // Create config directory if it doesn't exist
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)
                .with_context(|| format!("Failed to create config directory: {:?}", config_dir))?;
        }

        Ok(Self {
            config_file,
            config_dir,
            config: None,
        })
    }

    /// Load configuration from file, with fallback to defaults
    pub fn load_config(&mut self) -> Result<&Config> {
        if let Some(ref config) = self.config {
            return Ok(config);
        }

        let config = if self.config_file.exists() {
            let content = fs::read_to_string(&self.config_file)
                .with_context(|| format!("Failed to read config file: {:?}", self.config_file))?;

            // Try to parse as legacy format first
            if let Ok(legacy) = serde_yaml::from_str::<LegacyConfig>(&content) {
                legacy.settings
            } else if let Ok(cfg) = serde_yaml::from_str::<Config>(&content) {
                cfg
            } else {
                log::warn!("Failed to parse config, using defaults");
                Config::default()
            }
        } else {
            log::info!("Config file not found, using defaults");
            Config::default()
        };

        self.config = Some(config);
        Ok(self.config.as_ref().unwrap())
    }

    /// Save configuration to file
    pub fn save_config(&mut self, config: Option<&Config>) -> Result<()> {
        let config = if let Some(cfg) = config {
            cfg.clone()
        } else if let Some(ref cfg) = self.config {
            cfg.clone()
        } else {
            self.load_config()?.clone()
        };

        // Create backup if config file exists
        if self.config_file.exists() {
            let backup_path = self.config_file.with_extension("yaml.bak");
            fs::rename(&self.config_file, &backup_path).ok();
        }

        // Write new config
        let content = serde_yaml::to_string(&config)
            .with_context(|| "Failed to serialize config")?;
        fs::write(&self.config_file, content)
            .with_context(|| format!("Failed to write config file: {:?}", self.config_file))?;

        log::info!("Saved configuration to {:?}", self.config_file);
        Ok(())
    }

    /// Get a configuration value by key
    pub fn get(&mut self, key: &str) -> Result<ConfigValue> {
        let config = self.load_config()?;
        match key {
            "base_url" => Ok(ConfigValue::String(config.base_url.clone())),
            "api_key" => Ok(ConfigValue::String(config.api_key.clone())),
            "model" => Ok(ConfigValue::String(config.model.clone())),
            "temperature" => Ok(ConfigValue::Float(config.temperature)),
            "max_tokens" => Ok(ConfigValue::Int(config.max_tokens as i64)),
            "timeout" => Ok(ConfigValue::Int(config.timeout as i64)),
            "history_size" => Ok(ConfigValue::Int(config.history_size as i64)),
            _ => anyhow::bail!("Configuration key '{}' not found", key),
        }
    }

    /// Set a configuration value by key
    pub fn set(&mut self, key: &str, value: ConfigValue) -> Result<()> {
        let config = self.load_config()?;
        let mut config = config.clone();

        match key {
            "base_url" => config.base_url = value.into_string()?,
            "api_key" => config.api_key = value.into_string()?,
            "model" => config.model = value.into_string()?,
            "temperature" => config.temperature = value.into_float()?,
            "max_tokens" => config.max_tokens = value.into_int()? as u32,
            "timeout" => config.timeout = value.into_int()? as u64,
            "history_size" => config.history_size = value.into_int()? as usize,
            _ => anyhow::bail!("Configuration key '{}' not found", key),
        }

        self.config = Some(config);
        self.save_config(None)
    }

    /// Reset configuration to defaults
    pub fn reset(&mut self) -> Result<()> {
        self.config = Some(Config::default());
        self.save_config(None)?;
        log::info!("Configuration reset to defaults");
        Ok(())
    }

    /// Get the config directory
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }
}

/// Configuration value enum for flexible get/set operations
#[derive(Debug, Clone)]
pub enum ConfigValue {
    String(String),
    Int(i64),
    Float(f64),
}

impl ConfigValue {
    pub fn into_string(self) -> Result<String> {
        match self {
            ConfigValue::String(s) => Ok(s),
            _ => anyhow::bail!("Expected string value"),
        }
    }

    pub fn into_int(self) -> Result<i64> {
        match self {
            ConfigValue::Int(i) => Ok(i),
            _ => anyhow::bail!("Expected integer value"),
        }
    }

    pub fn into_float(self) -> Result<f64> {
        match self {
            ConfigValue::Float(f) => Ok(f),
            ConfigValue::Int(i) => Ok(i as f64),
            _ => anyhow::bail!("Expected float value"),
        }
    }
}

/// Get config manager, creating it if necessary
pub fn config_manager() -> Result<ConfigManager> {
    ConfigManager::new(None)
}
