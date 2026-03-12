//! CLI界面模块
//!
//! 纯交互层，只负责用户输入输出，通过 Gateway 与 Agent 交互
//!
//! 架构：
//! - AgentDefinition: Agent 定义/模板，独立存储，可复用
//! - Session: 运行时容器，包含消息历史
//! - AgentInstance: Agent 实例，在 Session 中运行

use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Input, Select};
use indicatif::{ProgressBar, ProgressStyle};

use crate::agent::{AgentInput, AgentOutput, InputMetadata, OutputType};
use crate::gateway::Gateway;

/// CLI应用
pub struct CliApp {
    gateway: Gateway,
    running: bool,
}

impl CliApp {
    /// 创建新的CLI应用
    pub fn new(gateway: Gateway) -> Result<Self> {
        Ok(Self {
            gateway,
            running: false,
        })
    }

    /// 运行CLI应用
    pub async fn run(&mut self) -> Result<()> {
        self.running = true;
        self.print_welcome();

        while self.running {
            let action = self.select_main_action()?;
            self.handle_action(action).await?;
        }

        Ok(())
    }

    /// 打印欢迎信息
    fn print_welcome(&self) {
        println!();
        println!("╔══════════════════════════════════════════════╗");
        println!("║     Self-Evolving Agent v0.1.0              ║");
        println!("║     一个能够自我进化的智能代理系统            ║");
        println!("╚══════════════════════════════════════════════╝");
        println!();

        let config = self.gateway.config_manager().config();
        println!("当前配置:");
        println!("  API入口: {}", config.llm.api_base);
        println!("  模型: {}", config.llm.model);
        println!("  日志级别: {}", config.log.level);
        println!();
    }

    /// 选择主操作
    fn select_main_action(&self) -> Result<MainAction> {
        let options = vec![
            "💬 开始对话",
            "📁 Session 管理",
            "🤖 Agent 定义管理",
            "⚙️  配置设置",
            "📖 查看日志",
            "❓ 帮助信息",
            "🚪 退出",
        ];

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("请选择操作")
            .items(&options)
            .default(0)
            .interact()?;

        Ok(match selection {
            0 => MainAction::Chat,
            1 => MainAction::SessionManage,
            2 => MainAction::AgentDefinitionManage,
            3 => MainAction::Config,
            4 => MainAction::ViewLogs,
            5 => MainAction::Help,
            6 => MainAction::Exit,
            _ => MainAction::Exit,
        })
    }

    /// 处理操作
    async fn handle_action(&mut self, action: MainAction) -> Result<()> {
        match action {
            MainAction::Chat => self.chat_loop().await?,
            MainAction::SessionManage => self.manage_sessions()?,
            MainAction::AgentDefinitionManage => self.manage_agent_definitions()?,
            MainAction::Config => self.configure()?,
            MainAction::ViewLogs => self.view_logs()?,
            MainAction::Help => self.show_help(),
            MainAction::Exit => {
                self.running = false;
                println!("\n再见！感谢使用 Self-Evolving Agent！");
            }
        }
        Ok(())
    }

    // ==================== 对话 ====================

    /// 对话循环
    async fn chat_loop(&mut self) -> Result<()> {
        // 检查配置
        let config = self.gateway.config_manager().config();
        if config.llm.api_key.is_empty() {
            println!("\n⚠️  尚未配置API Key，请先进行配置。");
            println!("正在跳转到配置界面...\n");
            return self.configure();
        }

        // 确保有 Session
        if self.gateway.current_session_id().is_none() {
            println!("\n⚠️  尚未创建 Session，正在创建默认 Session...\n");
            let name = format!("Session {}", chrono::Local::now().format("%Y%m%d_%H%M%S"));
            self.gateway.create_session(name)?;
        }

        // 确保有 Agent 实例
        if !self.gateway.has_agent_instances() {
            // 检查是否有 Agent 定义
            let definitions = self.gateway.list_agent_definitions();
            if definitions.is_empty() {
                println!("\n⚠️  尚未创建 Agent 定义，请先创建。\n");
                return self.manage_agent_definitions();
            }

            // 选择一个 Agent 定义来实例化
            println!("\n当前 Session 没有 Agent，请选择一个 Agent 定义来实例化：\n");
            let names: Vec<&str> = definitions.iter().map(|d| d.name.as_str()).collect();
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("选择 Agent 定义")
                .items(&names)
                .default(0)
                .interact()?;

            let definition = &definitions[selection];
            self.gateway.instantiate_agent(&definition.id)?;
            println!("\n✅ Agent '{}' 已实例化。\n", definition.name);
        }

        // 显示当前状态
        self.print_current_status();

        println!("\n📝 开始对话 (输入 'quit' 退出, 'save' 保存, 'clear' 清空历史)");
        println!("────────────────────────────────────────────\n");

        loop {
            let input: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("你")
                .interact()?;

            match input.trim() {
                "quit" | "exit" => {
                    println!();
                    break;
                }
                "save" => {
                    self.gateway.save_current_session()?;
                    println!("\n✅ Session 已保存。\n");
                }
                "clear" => {
                    self.gateway.clear_current_session_history()?;
                    println!("\n✅ Session 历史已清空。\n");
                }
                _ if input.trim().is_empty() => continue,
                _ => {
                    self.send_message(&input).await?;
                }
            }
        }

        Ok(())
    }

    /// 打印当前状态
    fn print_current_status(&self) {
        if let Some(session_id) = self.gateway.current_session_id() {
            let sessions = self.gateway.list_sessions();
            if let Some(session) = sessions.iter().find(|s| s.id == session_id) {
                println!("\n📁 当前 Session: {} ({} 条消息, {} 个 Agent)",
                    session.name,
                    session.message_count,
                    session.agent_count
                );
            }
        }
        if let Some(instance) = self.gateway.current_agent_instance() {
            let definition = self.gateway.get_agent_definition(instance.definition_id());
            if let Some(def) = definition {
                println!("🤖 当前 Agent: {} (定义: {})", 
                    def.name,
                    def.id.chars().take(8).collect::<String>()
                );
            }
        }
    }

    /// 发送消息
    async fn send_message(&mut self, content: &str) -> Result<()> {
        let agent_input = AgentInput {
            content: content.to_string(),
            metadata: Some(InputMetadata::default()),
        };

        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner()
            .template("{spinner} {msg}")
            .expect("Invalid template"));
        pb.set_message("思考中...");
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let output = self.gateway.send_message(agent_input).await?;

        pb.finish_and_clear();

        self.display_output(output);
        Ok(())
    }

    /// 显示输出
    fn display_output(&self, output: AgentOutput) {
        match output.output_type {
            OutputType::Response => {
                println!("\n🤖 助手: {}\n", output.content);
                if let Some(usage) = output.usage {
                    println!("   [tokens: {} prompt + {} completion = {} total]",
                        usage.prompt_tokens,
                        usage.completion_tokens,
                        usage.total_tokens
                    );
                }
            }
            OutputType::System => {
                println!("\n📢 系统: {}\n", output.content);
            }
            OutputType::Error => {
                println!("\n❌ 错误: {}\n", output.error.unwrap_or_else(|| "Unknown error".to_string()));
            }
        }
    }

    // ==================== Session 管理 ====================

    /// Session 管理
    fn manage_sessions(&mut self) -> Result<()> {
        loop {
            let sessions = self.gateway.list_sessions();

            println!("\n📁 Session 列表:");
            println!("────────────────────────────────────────────");

            if sessions.is_empty() {
                println!("  (暂无 Session)");
            } else {
                for info in &sessions {
                    let current = if self.gateway.current_session_id() == Some(&info.id) {
                        " [当前]"
                    } else {
                        ""
                    };
                    println!("  {} - {} 条消息, {} 个 Agent{}",
                        info.name,
                        info.message_count,
                        info.agent_count,
                        current
                    );
                }
            }
            println!();

            let options = if self.gateway.session_count() > 0 {
                vec!["创建 Session", "切换 Session", "删除 Session", "实例化 Agent", "返回"]
            } else {
                vec!["创建 Session", "返回"]
            };

            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("选择操作")
                .items(&options)
                .default(0)
                .interact()?;

            match selection {
                0 if options.len() == 2 => {
                    self.create_session()?;
                }
                0 => {
                    self.create_session()?;
                }
                1 if options.len() > 2 => {
                    self.switch_session(&sessions)?;
                }
                2 if options.len() > 3 => {
                    self.delete_session(&sessions)?;
                }
                3 if options.len() > 4 => {
                    self.instantiate_agent_in_session()?;
                }
                _ => break,
            }
        }

        Ok(())
    }

    /// 创建 Session
    fn create_session(&mut self) -> Result<()> {
        let name: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Session 名称")
            .default(format!("Session {}", chrono::Local::now().format("%Y%m%d_%H%M%S")))
            .interact()?;

        let id = self.gateway.create_session(name.clone())?;

        println!("\n✅ Session '{}' 已创建 (ID: {})。\n", name, &id[..8]);
        Ok(())
    }

    /// 切换 Session
    fn switch_session(&mut self, sessions: &[crate::session::SessionInfo]) -> Result<()> {
        if sessions.is_empty() {
            println!("\n暂无可切换的 Session。\n");
            return Ok(());
        }

        let names: Vec<String> = sessions.iter()
            .map(|s| {
                if self.gateway.current_session_id() == Some(&s.id) {
                    format!("{} [当前]", s.name)
                } else {
                    s.name.clone()
                }
            })
            .collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("选择 Session")
            .items(&names)
            .default(0)
            .interact()?;

        let session = &sessions[selection];
        self.gateway.switch_session(&session.id)?;

        println!("\n✅ 已切换到 Session '{}'\n", session.name);
        Ok(())
    }

    /// 删除 Session
    fn delete_session(&mut self, sessions: &[crate::session::SessionInfo]) -> Result<()> {
        if sessions.is_empty() {
            println!("\n暂无可删除的 Session。\n");
            return Ok(());
        }

        let names: Vec<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("选择要删除的 Session")
            .items(&names)
            .interact()?;

        let session = &sessions[selection];
        let name = session.name.clone();
        self.gateway.delete_session(&session.id)?;

        println!("\n✅ Session '{}' 已删除。\n", name);
        Ok(())
    }

    /// 在当前 Session 中实例化 Agent
    fn instantiate_agent_in_session(&mut self) -> Result<()> {
        let definitions = self.gateway.list_agent_definitions();
        if definitions.is_empty() {
            println!("\n⚠️  暂无 Agent 定义，请先创建。\n");
            return Ok(());
        }

        println!("\n选择要实例化的 Agent 定义：\n");
        let names: Vec<&str> = definitions.iter().map(|d| d.name.as_str()).collect();
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("选择 Agent 定义")
            .items(&names)
            .default(0)
            .interact()?;

        let definition = &definitions[selection];
        let id = self.gateway.instantiate_agent(&definition.id)?;

        println!("\n✅ Agent '{}' 已实例化 (ID: {})。\n", definition.name, &id[..8]);
        Ok(())
    }

    // ==================== Agent 定义管理 ====================

    /// Agent 定义管理
    fn manage_agent_definitions(&mut self) -> Result<()> {
        loop {
            let definitions = self.gateway.list_agent_definitions();

            println!("\n🤖 Agent 定义列表:");
            println!("────────────────────────────────────────────");

            if definitions.is_empty() {
                println!("  (暂无 Agent 定义)");
            } else {
                for info in &definitions {
                    let prompt_status = if info.has_system_prompt { "📝" } else { "  " };
                    println!("  {} {} (ID: {})", 
                        prompt_status,
                        info.name,
                        &info.id[..8]
                    );
                }
            }
            println!();

            let options = if definitions.is_empty() {
                vec!["创建 Agent 定义", "返回"]
            } else {
                vec!["创建 Agent 定义", "查看/编辑 Agent 定义", "删除 Agent 定义", "返回"]
            };

            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("选择操作")
                .items(&options)
                .default(0)
                .interact()?;

            match selection {
                0 if options.len() == 2 => {
                    self.create_agent_definition()?;
                }
                0 => {
                    self.create_agent_definition()?;
                }
                1 if options.len() > 2 => {
                    self.edit_agent_definition(&definitions)?;
                }
                2 if options.len() > 3 => {
                    self.delete_agent_definition(&definitions)?;
                }
                _ => break,
            }
        }

        Ok(())
    }

    /// 创建 Agent 定义
    fn create_agent_definition(&mut self) -> Result<()> {
        let name: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Agent 定义名称")
            .default(format!("Agent {}", chrono::Local::now().format("%H%M%S")))
            .interact()?;

        let system_prompt: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("系统提示词 (可选，直接回车跳过)")
            .allow_empty(true)
            .interact()?;

        let id = self.gateway.create_agent_definition(
            name.clone(),
            if system_prompt.is_empty() { None } else { Some(system_prompt) }
        )?;

        println!("\n✅ Agent 定义 '{}' 已创建 (ID: {})。\n", name, &id[..8]);
        Ok(())
    }

    /// 查看/编辑 Agent 定义
    fn edit_agent_definition(&mut self, definitions: &[crate::agent::AgentDefinitionInfo]) -> Result<()> {
        if definitions.is_empty() {
            return Ok(());
        }

        let names: Vec<&str> = definitions.iter().map(|d| d.name.as_str()).collect();
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("选择 Agent 定义")
            .items(&names)
            .interact()?;

        let info = &definitions[selection];
        
        // 显示详情
        if let Some(definition) = self.gateway.get_agent_definition(&info.id) {
            println!("\n📋 Agent 定义详情:");
            println!("────────────────────────────────────────────");
            println!("  名称: {}", definition.name);
            println!("  ID: {}", definition.id);
            if let Some(ref desc) = definition.description {
                println!("  描述: {}", desc);
            }
            println!("  系统提示词: {}", 
                definition.system_prompt.as_deref().unwrap_or("(未设置)")
            );
            println!("  模型: {}", definition.llm_config.model);
            println!();
        }

        let options = vec!["修改系统提示词", "修改名称", "返回"];
        let action = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("选择操作")
            .items(&options)
            .interact()?;

        match action {
            0 => {
                let prompt: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("新的系统提示词")
                    .interact()?;
                
                self.gateway.update_agent_definition(&info.id, 
                    crate::gateway::AgentDefinitionUpdates {
                        name: None,
                        system_prompt: Some(prompt),
                        llm_config: None,
                    }
                )?;
                println!("\n✅ 系统提示词已更新。\n");
            }
            1 => {
                let name: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("新的名称")
                    .interact()?;
                
                self.gateway.update_agent_definition(&info.id,
                    crate::gateway::AgentDefinitionUpdates {
                        name: Some(name),
                        system_prompt: None,
                        llm_config: None,
                    }
                )?;
                println!("\n✅ 名称已更新。\n");
            }
            _ => {}
        }

        Ok(())
    }

    /// 删除 Agent 定义
    fn delete_agent_definition(&mut self, definitions: &[crate::agent::AgentDefinitionInfo]) -> Result<()> {
        if definitions.is_empty() {
            return Ok(());
        }

        let names: Vec<&str> = definitions.iter().map(|d| d.name.as_str()).collect();
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("选择要删除的 Agent 定义")
            .items(&names)
            .interact()?;

        let info = &definitions[selection];
        let name = info.name.clone();
        self.gateway.delete_agent_definition(&info.id)?;

        println!("\n✅ Agent 定义 '{}' 已删除。\n", name);
        Ok(())
    }

    // ==================== 配置 ====================

    /// 配置设置
    fn configure(&mut self) -> Result<()> {
        let options = vec![
            "设置 API 入口",
            "设置 API Key",
            "设置模型",
            "设置日志级别",
            "设置温度参数",
            "显示当前配置",
            "重置为默认配置",
            "保存配置",
            "返回",
        ];

        loop {
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("配置设置")
                .items(&options)
                .default(0)
                .interact()?;

            let config = self.gateway.config_manager().config();

            match selection {
                0 => {
                    let api_base: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("API Base URL")
                        .default(config.llm.api_base.clone())
                        .interact()?;
                    self.gateway.config_manager_mut().config_mut().llm.api_base = api_base;
                    self.gateway.update_config()?;
                }
                1 => {
                    let api_key: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("API Key")
                        .interact()?;
                    self.gateway.config_manager_mut().config_mut().llm.api_key = api_key;
                    self.gateway.update_config()?;
                }
                2 => {
                    let model: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("模型名称")
                        .default(config.llm.model.clone())
                        .interact()?;
                    self.gateway.config_manager_mut().config_mut().llm.model = model;
                    self.gateway.update_config()?;
                }
                3 => {
                    let levels = vec!["error", "warning", "info", "debug"];
                    let level_idx = Select::with_theme(&ColorfulTheme::default())
                        .with_prompt("日志级别")
                        .items(&levels)
                        .default(2)
                        .interact()?;
                    self.gateway.config_manager_mut().config_mut().log.level = levels[level_idx].to_string();
                }
                4 => {
                    let temp: f32 = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("温度参数 (0.0-2.0)")
                        .default(config.llm.temperature)
                        .interact()?;
                    self.gateway.config_manager_mut().config_mut().llm.temperature = temp.clamp(0.0, 2.0);
                    self.gateway.update_config()?;
                }
                5 => {
                    let config = self.gateway.config_manager().config();
                    println!("\n当前配置:");
                    println!("  API入口: {}", config.llm.api_base);
                    println!("  API Key: {}...", &config.llm.api_key.chars().take(8).collect::<String>());
                    println!("  模型: {}", config.llm.model);
                    println!("  最大Tokens: {}", config.llm.max_tokens);
                    println!("  温度: {}", config.llm.temperature);
                    println!("  日志级别: {}", config.log.level);
                    println!();
                }
                6 => {
                    self.gateway.config_manager_mut().reset_to_default();
                    println!("\n✅ 配置已重置为默认值。\n");
                }
                7 => {
                    self.gateway.config_manager_mut().save()?;
                    println!("\n✅ 配置已保存到: {}\n",
                        self.gateway.config_manager().config_path().display());
                }
                _ => break,
            }
        }

        Ok(())
    }

    /// 查看日志
    fn view_logs(&self) -> Result<()> {
        println!("\n📖 日志设置:");
        println!("  日志目录: {}", self.gateway.logger().log_dir().display());
        println!("  当前级别: {}", self.gateway.logger().level());
        println!();
        println!("提示: 可使用文本编辑器打开日志文件进行查看。");
        println!();
        Ok(())
    }

    /// 显示帮助信息
    fn show_help(&self) {
        println!();
        println!("📖 Self-Evolving Agent 帮助信息");
        println!("────────────────────────────────────────────");
        println!();
        println!("架构说明:");
        println!("  AgentDefinition (定义) -> Session (容器) -> AgentInstance (实例)");
        println!();
        println!("  AgentDefinition: Agent 定义/模板，独立存储，可复用");
        println!("  Session: 运行时容器，包含消息历史");
        println!("  AgentInstance: Agent 实例，在 Session 中运行");
        println!();
        println!("对话命令:");
        println!("  quit/exit  - 退出当前对话");
        println!("  save       - 保存当前 Session");
        println!("  clear      - 清空当前 Session 历史");
        println!();
        println!("使用流程:");
        println!("  1. 创建 Agent 定义 (设置名称和系统提示词)");
        println!("  2. 创建 Session");
        println!("  3. 在 Session 中实例化 Agent");
        println!("  4. 开始对话");
        println!();
    }
}

/// 主操作枚举
#[derive(Debug, Clone, Copy)]
pub enum MainAction {
    Chat,
    SessionManage,
    AgentDefinitionManage,
    Config,
    ViewLogs,
    Help,
    Exit,
}