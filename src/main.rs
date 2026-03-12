//! Self-Evolving Agent 入口点
//!
//! 主程序入口

use anyhow::Result;
use directories::ProjectDirs;

use self_evolving_agent::{
    CliApp, ConfigManager, Gateway, Logger, LogLevel,
};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化配置管理器
    let config_manager = ConfigManager::new()?;
    let config = config_manager.config();

    // 设置日志
    let log_level = match config.log.level.as_str() {
        "error" => LogLevel::Error,
        "warning" => LogLevel::Warning,
        "debug" => LogLevel::Debug,
        _ => LogLevel::Info,
    };

    let log_dir = ProjectDirs::from("com", "az1mus", "self-evolving-agent")
        .map(|d| d.data_dir().join("logs"))
        .unwrap_or_else(|| std::env::current_dir().unwrap().join("logs"));

    let logger = Logger::new(log_dir, log_level)?;

    // 创建 Gateway
    let gateway = Gateway::new(config_manager, logger)?;

    // 创建并运行 CLI
    let mut app = CliApp::new(gateway)?;
    app.run().await?;

    Ok(())
}
