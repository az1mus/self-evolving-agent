/// Command Line Interface for SEA (Self-Evolved Agent)
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use dialoguer::Input;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

use crate::api::{create_client_from_config, Message, ToolCall};
use crate::config::{config_manager, ConfigValue};
use crate::persona::Orchestrator;
use crate::session::SessionManager;
use crate::tools::{create_tool_manager, router::ToolRouter, ToolManager};
use crate::tools::builtins::register_builtin_tools;
use crate::utils;

// Note: System prompts have been migrated to Persona configuration files (JSON)
// See ~/.sea/personas/ directory for persona definitions

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

    /// Manage personas
    Persona {
        #[command(subcommand)]
        command: PersonaCommands,
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

#[derive(Subcommand)]
pub enum PersonaCommands {
    /// List all available personas
    List,

    /// Show details of a persona
    Show {
        /// Persona name
        name: String,
    },

    /// Create a new persona from template
    Create {
        /// New persona name
        name: String,
        /// Template to use
        #[arg(short, long)]
        template: String,
    },

    /// Reload personas from disk
    Reload,

    /// Show persona statistics
    Stats,
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

/// Get the Python venv directory
fn get_pyvenv_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".sea").join("pyvenv"))
        .unwrap_or_else(|| PathBuf::from("./.sea/pyvenv"))
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

/// Initialize Python virtual environment
fn ensure_python_venv() -> Result<PathBuf> {
    let pyvenv_dir = get_pyvenv_dir();
    
    if !pyvenv_dir.exists() {
        println!(
            "{}",
            format!("═══ Setting up Python Virtual Environment ═══").cyan().bold()
        );
        println!("Creating virtual environment at: {:?}", pyvenv_dir);
        println!("{}", "══════════════════════════════════════════════".cyan());
        
        // Ensure parent directory exists
        if let Some(parent) = pyvenv_dir.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create parent directory: {:?}", parent))?;
            }
        }
        
        // Find python executable
        let python_cmd = find_python();
        
        // Create virtual environment
        let output = Command::new(&python_cmd)
            .args(["-m", "venv", pyvenv_dir.to_str().unwrap()])
            .output()
            .with_context(|| format!("Failed to create virtual environment using {}", python_cmd))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create venv: {}", stderr);
        }
        
        log::info!("Created Python venv at: {:?}", pyvenv_dir);
        println!(
            "{}",
            format!("✓ Python virtual environment created successfully").green()
        );
    } else {
        log::info!("Python venv already exists at: {:?}", pyvenv_dir);
    }
    
    Ok(pyvenv_dir)
}

/// Find Python executable
fn find_python() -> String {
    // Try common Python executable names
    let python_names = ["python3", "python", "py"];
    
    for name in &python_names {
        // On Windows, also try with .exe extension
        #[cfg(windows)]
        {
            let output = Command::new(name)
                .arg("--version")
                .output();
            if let Ok(out) = output {
                if out.status.success() {
                    log::info!("Found Python: {}", name);
                    return name.to_string();
                }
            }
        }
        
        #[cfg(unix)]
        {
            let output = Command::new(name)
                .arg("--version")
                .output();
            if let Ok(out) = output {
                if out.status.success() {
                    log::info!("Found Python: {}", name);
                    return name.to_string();
                }
            }
        }
    }
    
    // Default to python
    "python".to_string()
}

/// Get the Python executable path from venv
fn get_venv_python(venv_dir: &PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        venv_dir.join("Scripts").join("python.exe")
    }
    
    #[cfg(unix)]
    {
        venv_dir.join("bin").join("python")
    }
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

    // Initialize Python environment
    let _pyvenv_dir = ensure_python_venv()?;

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

/// Start an interactive chat session using Persona system
pub async fn run_chat() -> Result<()> {
    use std::sync::Arc;
    use tokio::sync::Mutex;
    
    let mut config_mgr = config_manager()?;
    let config = config_mgr.load_config()?.clone();

    let client = crate::api::APIClient::new(config)?;
    let mut tool_manager = create_tool_manager();
    register_builtin_tools(&mut tool_manager);
    let tool_router = ToolRouter::new();

    // Initialize Python environment
    let pyvenv_dir = ensure_python_venv()?;
    let _venv_python = get_venv_python(&pyvenv_dir);

    // Ensure scripts directory exists
    let scripts_dir = ensure_scripts_dir()?;

    let mut session_manager = SessionManager::new(
        config_mgr.config_dir(),
        None,
    )?;

    // Initialize Persona Manager
    let persona_manager = Arc::new(Mutex::new(
        crate::persona::manager::PersonaManager::new()?
    ));

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

        // Execute using Orchestrator
        let orchestrator = Orchestrator::new(persona_manager.clone());
        orchestrator.execute_pipeline(
            "intent_detector",
            user_input,
            &messages,
            &client,
            &tool_manager,
            &tool_router,
            &mut session_manager,
            &scripts_dir,
        ).await?;
    }

    println!(
        "{}",
        format!("Ended chat session: {}", session.name).bold().green()
    );

    Ok(())
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

// ═══════════════════════════════════════════════════════════════════════
// Persona Management Commands
// ═══════════════════════════════════════════════════════════════════════

/// List all available personas
pub fn run_list_personas() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let manager = crate::persona::manager::PersonaManager::new()?;
        let personas = manager.list_personas();
        
        if personas.is_empty() {
            println!("{}", "No personas found".yellow());
            return Ok(());
        }
        
        println!("{}", "Available Personas:".bold());
        for name in personas {
            if let Some(config) = manager.get_config(name) {
                println!(
                    "  - {} ({}) - {}",
                    name.blue(),
                    config.display_name,
                    config.description
                );
            }
        }
        
        Ok(())
    })
}

/// Show details of a persona
pub fn run_show_persona(name: &str) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let manager = crate::persona::manager::PersonaManager::new()?;
        
        if let Some(config) = manager.get_config(name) {
            println!("{}", format!("═══ Persona: {} ═══", config.display_name).bold().blue());
            println!("Name: {}", config.name);
            println!("Type: {:?}", config.persona_type);
            println!("Description: {}", config.description);
            println!("Temperature: {}", config.temperature);
            println!("Max Tokens: {}", config.max_tokens);
            if let Some(model) = &config.model {
                println!("Model: {}", model);
            }
            println!("\nSystem Prompt:");
            println!("{}", "─────────────────────────────────────────".dimmed());
            println!("{}", config.system_prompt);
            println!("{}", "─────────────────────────────────────────".dimmed());
            
            if !config.tags.is_empty() {
                println!("\nTags: {}", config.tags.join(", "));
            }
        } else {
            print_error("Error", &format!("Persona not found: {}", name));
        }
        
        Ok(())
    })
}

/// Create a new persona from template
pub fn run_create_persona(name: &str, template: &str) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut manager = crate::persona::manager::PersonaManager::new()?;
        
        match manager.create_from_template(name, template) {
            Ok(_) => {
                println!("{}", format!("Created persona '{}' from template '{}'", name, template).green());
            }
            Err(e) => {
                print_error("Error", &format!("Failed to create persona: {}", e));
            }
        }
        
        Ok(())
    })
}

/// Reload personas from disk
pub fn run_reload_personas() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut manager = crate::persona::manager::PersonaManager::new()?;
        
        match manager.reload() {
            Ok(_) => {
                println!("{}", "Personas reloaded successfully".green());
            }
            Err(e) => {
                print_error("Error", &format!("Failed to reload personas: {}", e));
            }
        }
        
        Ok(())
    })
}

/// Show persona statistics
pub fn run_persona_stats() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let manager = crate::persona::manager::PersonaManager::new()?;
        let stats = manager.stats();

        println!("{}", "Persona Statistics:".bold());
        println!("  Total: {}", stats.total);
        println!("  Core: {}", stats.core);
        println!("  Utility: {}", stats.utility);
        println!("  Special: {}", stats.special);

        Ok(())
    })
}
