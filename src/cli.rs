/// Command Line Interface for SEA (Self-Evolved Agent)
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use dialoguer::Input;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::api::{create_client_from_config, Message, ToolCall};
use crate::config::{config_manager, ConfigValue};
use crate::session::SessionManager;
use crate::tools::{create_tool_manager, router::ToolRouter, Tool, ToolManager};
use crate::tools::builtins::register_builtin_tools;
use crate::tools::router::{scan_python_scripts, match_python_scripts};
use crate::utils;

/// System prompt for intent detection (no tools)
const SYSTEM_PROMPT_INTENT: &str = r#"你是一个智能助手。请分析用户请求，判断是否需要使用工具来完成。

如果你认为需要使用工具，请在回复的第一行输出：[TOOL_NEEDED] 并简要说明需要什么工具。
如果你不需要工具就能回答，请直接回答。

示例：
用户："列出当前目录的文件"
你："[TOOL_NEEDED] 我需要列出文件的工具"

用户："你好"
你："你好！有什么可以帮助你的吗？"
"#;

/// System prompt for tool selection specialist
const SYSTEM_PROMPT_TOOL_SELECT: &str = r#"你是一个工具选择专家。你的任务是从给定的工具列表中选择最合适的工具来完成任务。

用户会提供：
1. 任务描述
2. 可用工具列表

你只需要选择合适的工具并调用它。不要添加额外解释。
"#;

/// System prompt for Python script generation
const SYSTEM_PROMPT_PYTHON_GEN: &str = r#"你是一个专业的 Python 开发者。你的任务是根据用户需求编写一个 Python 脚本。

要求：
1. 脚本必须从命令行参数接收 JSON 格式的参数（sys.argv[1]）
2. 脚本必须有清晰的 docstring，包含功能描述
3. 脚本必须处理可能的错误情况
4. 输出应该清晰、简洁

脚本格式示例：
```python
#!/usr/bin/env python3
"""
功能描述
"""

import sys
import json

def main():
    if len(sys.argv) < 2:
        print("Error: No arguments provided")
        sys.exit(1)
    
    args = json.loads(sys.argv[1])
    # 处理参数并输出结果
    print("执行结果")

if __name__ == "__main__":
    main()
```

只返回 Python 代码，不要有其他解释。
"#;

/// System prompt for final response generation
const SYSTEM_PROMPT_FINAL: &str = r#"你是一个友好的智能助手。请根据工具执行的结果，给用户一个清晰、完整的回答。

如果工具执行成功，请总结结果。
如果工具执行失败，请礼貌地解释原因并提供建议。
"#;

/// Self-Evolved Agent (SEA) - A CLI tool for interacting with LLMs
#[derive(Parser)]
#[command(name = "sea")]
#[command(author = "az1mus <1745488741@qq.com>")]
#[command(version = "0.1.0")]
#[command(about = "Self-Evolved Agent (SEA) - A CLI tool for interacting with LLMs", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Send a single query to the LLM
    Query {
        /// The message to send
        message: Option<String>,

        /// Model to use for the query
        #[arg(short, long)]
        model: Option<String>,

        /// Sampling temperature
        #[arg(short, long)]
        temperature: Option<f64>,
    },

    /// Start an interactive chat session
    Chat,

    /// Manage configuration settings
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// List available models from the API
    Models,

    /// List available tools
    Tools,

    /// Manage conversation sessions
    Session {
        #[command(subcommand)]
        command: SessionCommands,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Set a configuration value
    Set {
        /// Configuration key
        #[arg(value_enum)]
        key: ConfigKey,
        /// Configuration value
        value: String,
    },

    /// Get a configuration value
    Get {
        /// Configuration key
        #[arg(value_enum)]
        key: ConfigKey,
    },

    /// List all configuration values
    List,

    /// Reset configuration to defaults
    Reset,
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum ConfigKey {
    BaseUrl,
    ApiKey,
    Model,
    Temperature,
    MaxTokens,
    Timeout,
    HistorySize,
}

impl ConfigKey {
    fn as_str(&self) -> &'static str {
        match self {
            ConfigKey::BaseUrl => "base_url",
            ConfigKey::ApiKey => "api_key",
            ConfigKey::Model => "model",
            ConfigKey::Temperature => "temperature",
            ConfigKey::MaxTokens => "max_tokens",
            ConfigKey::Timeout => "timeout",
            ConfigKey::HistorySize => "history_size",
        }
    }
}

#[derive(Subcommand)]
pub enum SessionCommands {
    /// List all available sessions
    List,

    /// Delete a session by ID
    Delete {
        /// Session ID to delete
        session_id: String,
    },
}

/// Print a styled message
fn print_message(title: &str, content: &str, _style: &str) {
    println!();
    println!("{}", format!("═══ {} ═══", title).bold().blue());
    println!("{}", content);
    println!("{}", "═══════════════════════════════════════".blue());
}

/// Print an error message
fn print_error(title: &str, message: &str) {
    eprintln!();
    eprintln!("{}", format!("═══ {} ═══", title).bold().red());
    eprintln!("{}", message);
    eprintln!("{}", "═══════════════════════════════════════".red());
}

/// Get the Python scripts directory
fn get_scripts_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".sea").join("scripts"))
        .unwrap_or_else(|| PathBuf::from("./.sea/scripts"))
}

/// Ensure the Python scripts directory exists
fn ensure_scripts_dir() -> Result<PathBuf> {
    let scripts_dir = get_scripts_dir();
    if !scripts_dir.exists() {
        fs::create_dir_all(&scripts_dir)
            .with_context(|| format!("Failed to create scripts directory: {:?}", scripts_dir))?;
        log::info!("Created scripts directory: {:?}", scripts_dir);
    }
    Ok(scripts_dir)
}

/// Run a single query
pub async fn run_query(
    message: Option<String>,
    model: Option<String>,
    temperature: Option<f64>,
) -> Result<()> {
    let message = if let Some(msg) = message {
        msg
    } else {
        print!("Enter your message: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        input.trim().to_string()
    };

    let mut config_mgr = config_manager()?;
    let mut config = config_mgr.load_config()?.clone();

    // Override config with command line options if provided
    if let Some(m) = model {
        config.model = m;
    }
    if let Some(t) = temperature {
        config.temperature = t;
    }

    let client = crate::api::APIClient::new(config)?;
    let mut tool_manager = create_tool_manager();
    register_builtin_tools(&mut tool_manager);

    // Create message history (just the user message for single queries)
    let mut messages = vec![Message::new("user", &message)];

    // Get response from the model
    let response = client
        .chat_completion(&messages, Some(&tool_manager.get_available_tools()), false)
        .await?;

    // Process any tool calls
    if let Some(tool_calls) = &response.tool_calls {
        for tool_call in tool_calls {
            match execute_tool_call(&tool_manager, tool_call) {
                Ok(result) => {
                    // Add tool result to messages with tool_call_id
                    messages.push(Message::with_tool_result(
                        "tool",
                        &result,
                        &tool_call.id,
                        &tool_call.function.name,
                    ));

                    // Get final response after tool execution
                    let final_response = client
                        .chat_completion(&messages, Some(&tool_manager.get_available_tools()), false)
                        .await?;

                    if let Some(content) = &final_response.content {
                        print_message("Final Response", content, "green");
                    }
                }
                Err(e) => {
                    print_error("Tool Error", &format!("Error executing tool: {}", e));
                }
            }
        }
    } else if let Some(content) = &response.content {
        print_message("Response", content, "green");
    }

    Ok(())
}

/// Execute a tool call
fn execute_tool_call(tool_manager: &ToolManager, tool_call: &ToolCall) -> Result<String> {
    println!(
        "{}",
        format!("Executing tool: {}", tool_call.function.name).cyan()
    );

    tool_manager.execute_tool_call(&tool_call.function.name, &tool_call.function.arguments)
}

/// Start an interactive chat session
pub async fn run_chat() -> Result<()> {
    let mut config_mgr = config_manager()?;
    let config = config_mgr.load_config()?.clone();

    let client = crate::api::APIClient::new(config)?;
    let mut tool_manager = create_tool_manager();
    register_builtin_tools(&mut tool_manager);
    let tool_router = ToolRouter::new();

    // Ensure scripts directory exists
    let scripts_dir = ensure_scripts_dir()?;

    let mut session_manager = SessionManager::new(
        config_mgr.config_dir(),
        None,
    )?;

    // Create or resume a session
    let session = if let Some(s) = session_manager.get_active_session() {
        s.clone()
    } else {
        let session_name: String = Input::new()
            .with_prompt("Enter session name")
            .default("default".to_string())
            .interact_text()?;
        session_manager.create_session(&session_name, None)?.clone()
    };

    println!(
        "{}",
        format!("Starting chat session: {}", session.name).bold().green()
    );
    println!("Type 'exit' or 'quit' to end the session");

    loop {
        print!("\n{}: ", "You".yellow().bold());
        io::stdout().flush()?;

        let mut user_input = String::new();
        io::stdin().read_line(&mut user_input)?;
        let user_input = user_input.trim();

        if ["exit", "quit", "q"].contains(&user_input.to_lowercase().as_str()) {
            break;
        }

        // Add user message to session
        session_manager.add_message(Message::new("user", user_input))?;
        log::info!("User message recorded: {}", user_input);

        // Get all messages in the session for context
        let messages = session_manager
            .get_messages()
            .context("No active session")?;

        // ═══════════════════════════════════════════════════════════════
        // Step 1: Intent Detection - 判断是否需要工具
        // ═══════════════════════════════════════════════════════════════
        log::info!("Step 1: Detecting intent...");

        let mut intent_messages = messages.clone();
        intent_messages.insert(0, Message::new("system", SYSTEM_PROMPT_INTENT));

        let intent_response = client
            .chat_completion(&intent_messages, None, false)
            .await?;

        let intent_content = intent_response.content.unwrap_or_default();
        log::debug!("Intent response: {}", intent_content);

        if !intent_content.trim_start().starts_with("[TOOL_NEEDED]") {
            // No tool needed, direct response
            log::info!("No tool needed, using direct response");
            println!(
                "\n{}",
                format!("═══ Assistant ═══").blue().bold()
            );
            println!("{}", intent_content);
            println!("{}", "═══════════════════════════════════════".blue());
            session_manager.add_message(Message::new("assistant", &intent_content))?;
            continue;
        }

        log::info!("Tool needed detected: {}", intent_content.lines().next().unwrap_or(""));

        // ═══════════════════════════════════════════════════════════════
        // Step 2: Tool Selection - 从内置工具中选择
        // ═══════════════════════════════════════════════════════════════
        log::info!("Step 2: Selecting from built-in tools...");

        let selected_tool_names = tool_router.select_tools(user_input);
        log::info!("Router selected {} tools: {:?}", selected_tool_names.len(), selected_tool_names);

        let filtered_tools = tool_router.get_filtered_tools(&tool_manager, &selected_tool_names);

        // Check if any built-in tool matches (excluding execute_python)
        let has_builtin_match = filtered_tools.iter()
            .any(|t| t.name() != "execute_python");

        if has_builtin_match {
            // ===== Execute built-in tool =====
            log::info!("Step 2a: Executing built-in tool...");

            let mut tool_select_messages = messages.clone();
            tool_select_messages.insert(0, Message::new("system", SYSTEM_PROMPT_TOOL_SELECT));

            let tool_names: Vec<&str> = filtered_tools.iter().map(|t| t.name()).collect();
            let tool_context = format!(
                "用户需要：{}。可用工具：[{}]",
                user_input,
                tool_names.join(", ")
            );
            tool_select_messages.push(Message::new("user", &tool_context));

            let tool_response = client
                .chat_completion(&tool_select_messages, Some(&filtered_tools), false)
                .await?;

            if let Some(tool_calls) = &tool_response.tool_calls {
                for tool_call in tool_calls {
                    match execute_tool_call_and_respond(
                        &tool_manager,
                        tool_call,
                        user_input,
                        &client,
                        &messages,
                        &mut session_manager,
                    ).await {
                        Ok(_) => {}
                        Err(e) => {
                            print_error("Tool Error", &format!("Error executing tool: {}", e));
                        }
                    }
                }
            } else {
                log::warn!("Expected tool calls but got none");
                println!("我需要调用工具，但未能成功选择。{}", intent_content);
            }
            continue;
        }

        // ═══════════════════════════════════════════════════════════════
        // Step 3: Scan Python Scripts - 扫描现有 Python 脚本
        // ═══════════════════════════════════════════════════════════════
        log::info!("Step 3: Scanning Python scripts in {:?}", scripts_dir);

        let available_scripts = scan_python_scripts(&scripts_dir);
        log::info!("Found {} Python scripts", available_scripts.len());

        if !available_scripts.is_empty() {
            let matched_scripts = match_python_scripts(user_input, &available_scripts);
            log::info!("Matched {} Python scripts: {:?}", matched_scripts.len(), 
                matched_scripts.iter().map(|s| &s.name).collect::<Vec<_>>());

            if !matched_scripts.is_empty() {
                // ===== Execute Python script =====
                log::info!("Step 3a: Executing matched Python script...");

                // Let LLM choose which script and generate args
                let mut script_select_messages = messages.clone();
                script_select_messages.insert(0, Message::new("system", SYSTEM_PROMPT_TOOL_SELECT));

                // Provide script info with paths for LLM to use
                let script_infos: Vec<String> = matched_scripts.iter()
                    .map(|s| format!("{}: {}", s.name, s.path.display()))
                    .collect();
                let script_context = format!(
                    "用户需要：{}。可用 Python 脚本：[{}]。请使用 execute_python 工具执行合适的脚本，需要提供 script_path 参数。",
                    user_input,
                    script_infos.join(", ")
                );
                script_select_messages.push(Message::new("user", &script_context));

                // Create a temporary tool manager with only execute_python
                let mut python_tool_manager = create_tool_manager();
                if let Some(python_tool) = tool_manager.get_tool("execute_python") {
                    python_tool_manager.register_tool(Box::new(PythonToolWrapper::new(
                        python_tool.name(),
                        python_tool.description(),
                        python_tool.parameters(),
                    )));
                }

                let script_response = client
                    .chat_completion(&script_select_messages, Some(&python_tool_manager.get_available_tools()), false)
                    .await?;

                if let Some(tool_calls) = &script_response.tool_calls {
                    for tool_call in tool_calls {
                        if tool_call.function.name == "execute_python" {
                            match execute_tool_call_and_respond(
                                &tool_manager,
                                tool_call,
                                user_input,
                                &client,
                                &messages,
                                &mut session_manager,
                            ).await {
                                Ok(_) => {}
                                Err(e) => {
                                    print_error("Tool Error", &format!("Error executing script: {}", e));
                                }
                            }
                        }
                    }
                }
                continue;
            }
        }

        // ═══════════════════════════════════════════════════════════════
        // Step 4: Generate Python Script - 生成新的 Python 脚本
        // ═══════════════════════════════════════════════════════════════
        log::info!("Step 4: No matching script found, generating new Python script...");

        let mut gen_messages = messages.clone();
        gen_messages.insert(0, Message::new("system", SYSTEM_PROMPT_PYTHON_GEN));
        gen_messages.push(Message::new("user", &format!(
            "请编写一个 Python 脚本来完成以下任务：{}\n\n只返回 Python 代码，不要有其他解释。",
            user_input
        )));

        let gen_response = client
            .chat_completion(&gen_messages, None, false)
            .await?;

        let script_code = gen_response.content.unwrap_or_default();
        log::debug!("Generated script:\n{}", script_code);

        // Extract code from markdown if present
        let script_code_clean = extract_code_from_markdown(&script_code);

        // Save script
        let script_name = generate_script_name(user_input);
        let script_path = scripts_dir.join(format!("{}.py", script_name));

        match fs::write(&script_path, &script_code_clean) {
            Ok(_) => {
                log::info!("Saved script to {:?}", script_path);
                println!(
                    "{}",
                    format!("═══ Generated Script: {}.py ═══", script_name).green().bold()
                );
                println!("Saved to: {:?}", script_path);
                println!("{}", "══════════════════════════════════════════════".green());

                // ═══════════════════════════════════════════════════════════════
                // Step 5: Execute Generated Script
                // ═══════════════════════════════════════════════════════════════
                log::info!("Step 5: Executing generated script...");

                // Let LLM generate arguments for the script
                let mut exec_messages = messages.clone();
                exec_messages.insert(0, Message::new("system", SYSTEM_PROMPT_TOOL_SELECT));
                exec_messages.push(Message::new("user", &format!(
                    "请使用 execute_python 工具执行刚保存的脚本：{}\n脚本路径：{}",
                    user_input, script_path.display()
                )));

                let mut python_tool_manager = create_tool_manager();
                if let Some(python_tool) = tool_manager.get_tool("execute_python") {
                    python_tool_manager.register_tool(Box::new(PythonToolWrapper::new(
                        python_tool.name(),
                        python_tool.description(),
                        python_tool.parameters(),
                    )));
                }

                let exec_response = client
                    .chat_completion(&exec_messages, Some(&python_tool_manager.get_available_tools()), false)
                    .await?;

                if let Some(tool_calls) = &exec_response.tool_calls {
                    for tool_call in tool_calls {
                        if tool_call.function.name == "execute_python" {
                            match execute_tool_call_and_respond(
                                &tool_manager,
                                tool_call,
                                user_input,
                                &client,
                                &messages,
                                &mut session_manager,
                            ).await {
                                Ok(_) => {}
                                Err(e) => {
                                    print_error("Script Execution Error", &format!("Error executing script: {}", e));
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                print_error("Script Save Error", &format!("Failed to save script: {}", e));
            }
        }
    }

    println!(
        "{}",
        format!("Ended chat session: {}", session.name).bold().green()
    );

    Ok(())
}

/// Helper struct to wrap Python executor tool
struct PythonToolWrapper {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

impl PythonToolWrapper {
    fn new(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
        }
    }
}

impl Tool for PythonToolWrapper {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.parameters.clone()
    }

    fn execute(&self, _args: &serde_json::Value) -> Result<String> {
        // This wrapper doesn't execute, it's just for LLM selection
        Ok("Tool executed via execute_python".to_string())
    }
}

/// Execute a tool call and generate response
async fn execute_tool_call_and_respond(
    tool_manager: &ToolManager,
    tool_call: &ToolCall,
    user_input: &str,
    client: &crate::api::APIClient,
    messages: &[Message],
    session_manager: &mut SessionManager,
) -> Result<()> {
    println!(
        "{}",
        format!("═══ Using tool: {} ═══", tool_call.function.name).cyan().bold()
    );
    log::info!("Executing tool: {} with args: {}",
        tool_call.function.name,
        tool_call.function.arguments);

    match execute_tool_call(tool_manager, tool_call) {
        Ok(result) => {
            println!(
                "\n{}",
                format!("═══ Tool Result: {} ═══", tool_call.function.name)
                    .magenta()
                    .bold()
            );
            println!("{}", result);
            println!("{}", "══════════════════════════════════════════════".magenta());

            // ===== Generate Final Response =====
            log::info!("Generating final response...");

            let mut final_messages: Vec<Message> = messages.to_vec();
            final_messages.insert(0, Message::new("system", SYSTEM_PROMPT_FINAL));

            let result_context = format!(
                "工具执行结果：{}\n请根据这个结果回答用户的问题：{}",
                result, user_input
            );
            final_messages.push(Message::new("user", &result_context));

            let final_response = client
                .chat_completion(&final_messages, None, false)
                .await?;

            if let Some(content) = &final_response.content {
                println!(
                    "\n{}",
                    format!("═══ Assistant ═══").blue().bold()
                );
                println!("{}", content);
                println!("{}", "═══════════════════════════════════════".blue());

                session_manager.add_message(Message::new("assistant", content))?;
                log::info!("Recorded final response: {} chars", content.len());
            }
        }
        Err(e) => {
            let error_msg = format!("Error executing tool: {}", e);
            print_error("Tool Error", &error_msg);
            log::warn!("Tool error: {}", error_msg);
        }
    }

    Ok(())
}

/// Extract code from markdown code blocks
fn extract_code_from_markdown(content: &str) -> String {
    if let Some(start) = content.find("```python") {
        let rest = &content[start + 9..];
        if let Some(end) = rest.find("```") {
            return rest[..end].trim().to_string();
        }
    }
    if let Some(start) = content.find("```") {
        let rest = &content[start + 3..];
        if let Some(end) = rest.find("```") {
            return rest[..end].trim().to_string();
        }
    }
    content.trim().to_string()
}

/// Generate a script name from user input
fn generate_script_name(user_input: &str) -> String {
    // Take first few words and sanitize
    user_input
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join("_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .to_lowercase()
}

/// Set a configuration value
pub fn config_set(key: ConfigKey, value: String) -> Result<()> {
    let mut config_mgr = config_manager()?;

    let config_value = match key {
        ConfigKey::BaseUrl | ConfigKey::ApiKey | ConfigKey::Model => {
            ConfigValue::String(value.clone())
        }
        ConfigKey::Temperature => ConfigValue::Float(value.parse()?),
        ConfigKey::MaxTokens | ConfigKey::Timeout | ConfigKey::HistorySize => {
            ConfigValue::Int(value.parse()?)
        }
    };

    config_mgr.set(key.as_str(), config_value)?;
    println!(
        "{}",
        format!("Set {} = {}", key.as_str(), value).green()
    );

    Ok(())
}

/// Get a configuration value
pub fn config_get(key: ConfigKey) -> Result<()> {
    let mut config_mgr = config_manager()?;
    let value = config_mgr.get(key.as_str())?;

    // Mask API key for security
    let display_value = if key == ConfigKey::ApiKey {
        match value {
            ConfigValue::String(ref s) => utils::mask_sensitive(s, 4),
            _ => "*".repeat(20),
        }
    } else {
        match value {
            ConfigValue::String(ref s) => s.clone(),
            ConfigValue::Int(i) => i.to_string(),
            ConfigValue::Float(f) => f.to_string(),
        }
    };

    println!("{} = {}", key.as_str().blue().bold(), display_value);
    Ok(())
}

/// List all configuration values
pub fn config_list() -> Result<()> {
    let mut config_mgr = config_manager()?;
    let config = config_mgr.load_config()?.clone();

    println!("{}", "Configuration:".bold().blue());
    println!("  {} = {}", "base_url".blue(), config.base_url);

    // Mask API key for security
    let api_key_display = if config.api_key.is_empty() {
        "(not set)".to_string()
    } else {
        utils::mask_sensitive(&config.api_key, 4)
    };
    println!("  {} = {}", "api_key".blue(), api_key_display);

    println!("  {} = {}", "model".blue(), config.model);
    println!("  {} = {}", "temperature".blue(), config.temperature);
    println!("  {} = {}", "max_tokens".blue(), config.max_tokens);
    println!("  {} = {}", "timeout".blue(), config.timeout);
    println!("  {} = {}", "history_size".blue(), config.history_size);

    Ok(())
}

/// Reset configuration to defaults
pub fn config_reset() -> Result<()> {
    let mut config_mgr = config_manager()?;
    config_mgr.reset()?;
    println!("{}", "Configuration reset to defaults".green());
    Ok(())
}

/// List available models from the API
pub async fn run_list_models() -> Result<()> {
    let client = create_client_from_config().await?;

    match client.list_models().await {
        Ok(models) => {
            println!("{}", "Available Models:".bold());
            for model in models {
                println!("  - {}", model);
            }
        }
        Err(e) => {
            print_error("Error", &format!("Error listing models: {}", e));
        }
    }

    Ok(())
}

/// List available tools
pub fn run_list_tools() -> Result<()> {
    let mut tool_manager = create_tool_manager();
    register_builtin_tools(&mut tool_manager);

    println!("{}", "Available Tools:".bold());
    for tool in tool_manager.get_available_tools() {
        println!("  - {}: {}", tool.name().blue(), tool.description());
    }

    Ok(())
}

/// List all available sessions
pub fn run_list_sessions() -> Result<()> {
    let config_mgr = config_manager()?;
    let session_manager = SessionManager::new(config_mgr.config_dir(), None)?;

    let sessions = session_manager.list_sessions()?;

    if sessions.is_empty() {
        println!("{}", "No sessions found".yellow());
        return Ok(());
    }

    println!("{}", "Available Sessions:".bold());
    for session in sessions {
        println!(
            "  - {} (updated: {})",
            session.name.blue(),
            session.updated_at.format("%Y-%m-%d %H:%M:%S")
        );
    }

    Ok(())
}

/// Delete a session by ID
pub fn run_delete_session(session_id: String) -> Result<()> {
    let config_mgr = config_manager()?;
    let mut session_manager = SessionManager::new(config_mgr.config_dir(), None)?;

    if session_manager.delete_session(&session_id)? {
        println!("{}", format!("Deleted session: {}", session_id).green());
    } else {
        print_error("Error", &format!("Session not found: {}", session_id));
    }

    Ok(())
}
