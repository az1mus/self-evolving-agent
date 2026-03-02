/// Persona configuration and execution
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Type of persona
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PersonaType {
    /// Core personas (always available)
    Core,
    /// Utility personas (on-demand)
    Utility,
    /// Special personas (activated by conditions)
    Special,
}

/// Input schema for persona
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSchema {
    /// Required fields
    #[serde(default)]
    pub required_fields: Vec<String>,
    /// Optional fields
    #[serde(default)]
    pub optional_fields: Vec<String>,
}

/// Output schema for persona
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSchema {
    /// Output format (text, json, markdown)
    #[serde(default = "default_format")]
    pub format: String,
    /// Regex pattern to parse output
    pub parse_pattern: Option<String>,
}

fn default_format() -> String {
    "text".to_string()
}

/// Error handling strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorStrategy {
    /// Number of retries
    #[serde(default = "default_retry")]
    pub retry: u32,
    /// Fallback persona
    pub fallback: Option<String>,
}

fn default_retry() -> u32 {
    3
}

/// Next persona routing rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextPersonaRule {
    /// Condition to match (regex or keyword)
    #[serde(rename = "if_match")]
    pub condition: String,
    /// Next persona if condition matches
    pub then: Option<String>,
    /// Next persona if condition doesn't match
    #[serde(rename = "else")]
    pub otherwise: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════
// Prompt Position Control
// ═══════════════════════════════════════════════════════════════════════

/// Position mode for prompt component
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PositionMode {
    /// Relative position (sorted by order value)
    Relative,
    /// Inside chat history at specified depth
    InChat,
    /// Fixed at header (before all system messages)
    Header,
    /// Fixed at footer (before user input)
    Footer,
}

/// Prompt position configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptPositionConfig {
    /// Position mode
    #[serde(default = "default_position_mode")]
    pub mode: PositionMode,
    /// Relative order (smaller = earlier)
    pub order: Option<u32>,
    /// Depth in chat history (only for InChat mode)
    pub depth: Option<u32>,
    /// Order within same depth/role
    pub inner_order: Option<u32>,
    /// Group name for sorting
    pub group: Option<String>,
}

fn default_position_mode() -> PositionMode {
    PositionMode::Relative
}

// ═══════════════════════════════════════════════════════════════════════
// Dependency Management
// ═══════════════════════════════════════════════════════════════════════

/// Dependency configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyConfig {
    /// Names of other personas/components this one depends on
    #[serde(default)]
    pub requires: Vec<String>,
    /// Names of data this component provides to others
    #[serde(default)]
    pub provides: Vec<String>,
    /// Whether this component is optional (skip if dependencies not met)
    #[serde(default = "default_true")]
    pub optional: bool,
}

fn default_true() -> bool {
    true
}

// ═══════════════════════════════════════════════════════════════════════
// Context Management
// ═══════════════════════════════════════════════════════════════════════

/// Truncate strategy for context management
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruncateStrategy {
    /// Truncate from head (keep recent messages)
    Head,
    /// Truncate from tail (keep early messages)
    Tail,
    /// Use summary instead of full content
    Summary,
}

/// Context management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextManagementConfig {
    /// Maximum number of messages to include
    pub max_messages: Option<u32>,
    /// Truncate strategy when exceeding max_messages
    pub truncate_strategy: Option<TruncateStrategy>,
    /// Token budget for this component
    pub token_budget: Option<u32>,
}

// ═══════════════════════════════════════════════════════════════════════
// Trigger Conditions
// ═══════════════════════════════════════════════════════════════════════

/// Trigger configuration for conditional activation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    /// Condition expression (e.g., "intent_contains('查询', '搜索')")
    pub condition: Option<String>,
    /// Data source name (e.g., "local_database", "conversation_history")
    pub data_source: Option<String>,
    /// Fallback component name if trigger fails
    pub fallback: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════
// Macro Definitions
// ═══════════════════════════════════════════════════════════════════════

/// Macro variable configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroConfig {
    /// Macro name (e.g., "{{user_input}}")
    pub name: String,
    /// Macro value or template
    pub value: String,
    /// Whether to evaluate dynamically
    #[serde(default)]
    pub dynamic: bool,
}

/// Persona configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaConfig {
    /// Unique identifier
    pub name: String,
    /// Display name
    pub display_name: String,
    /// Description
    pub description: String,
    /// Type of persona
    #[serde(rename = "type")]
    pub persona_type: PersonaType,
    /// System prompt
    pub system_prompt: String,
    /// Temperature (0.0-2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    /// Model override (optional)
    pub model: Option<String>,
    /// Max tokens
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Input schema
    pub input_schema: Option<InputSchema>,
    /// Output schema
    pub output_schema: Option<OutputSchema>,
    /// Error handling strategy
    pub on_error: Option<ErrorStrategy>,
    /// Next persona routing
    pub next_persona: Option<NextPersonaRule>,
    /// Tags for search and filtering
    #[serde(default)]
    pub tags: Vec<String>,
    /// Custom metadata
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
    
    // ═══════════════════════════════════════════════════════════════════
    // New fields for prompt position control and dependency management
    // ═══════════════════════════════════════════════════════════════════
    
    /// Prompt position configuration
    pub prompt_position: Option<PromptPositionConfig>,
    /// Dependency configuration
    pub dependencies: Option<DependencyConfig>,
    /// Context management configuration
    pub context_management: Option<ContextManagementConfig>,
    /// Trigger configuration
    pub triggers: Option<TriggerConfig>,
    /// Macro definitions
    #[serde(default)]
    pub macros: Vec<MacroConfig>,
}

fn default_temperature() -> f64 {
    0.7
}

fn default_max_tokens() -> u32 {
    2048
}

impl PersonaConfig {
    /// Load persona config from JSON file
    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read persona config: {}", path))?;

        let config: PersonaConfig = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse persona config: {}", path))?;

        Ok(config)
    }

    /// Save persona config to JSON file
    pub fn to_file(&self, path: &str) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            anyhow::bail!("Persona name cannot be empty");
        }
        if self.system_prompt.is_empty() {
            anyhow::bail!("System prompt cannot be empty for persona: {}", self.name);
        }
        Ok(())
    }
    
    /// Get prompt position config with defaults
    pub fn get_position(&self) -> PromptPositionConfig {
        self.prompt_position.clone().unwrap_or(PromptPositionConfig {
            mode: PositionMode::Relative,
            order: Some(100), // Default to middle
            depth: None,
            inner_order: None,
            group: None,
        })
    }
    
    /// Check if this persona should be enabled based on triggers
    pub fn should_enable(&self, context: &PromptEnableContext) -> bool {
        if let Some(triggers) = &self.triggers {
            if let Some(condition) = &triggers.condition {
                return Self::evaluate_condition(condition, context);
            }
        }
        true
    }
    
    /// Simple condition evaluator
    fn evaluate_condition(condition: &str, context: &PromptEnableContext) -> bool {
        // Support simple conditions:
        // - always() -> true
        // - intent_contains('keyword1', 'keyword2') -> check if intent contains any keyword
        // - persona_enabled('name') -> check if persona is enabled
        // - tool_available('name') -> check if tool is available
        
        if condition == "always()" {
            return true;
        }
        
        // Parse intent_contains('k1', 'k2', ...)
        if let Some(keywords) = Self::parse_function_args(condition, "intent_contains") {
            for kw in keywords {
                if context.user_input.contains(&kw) {
                    return true;
                }
            }
            return false;
        }
        
        // Parse persona_enabled('name')
        if let Some(names) = Self::parse_function_args(condition, "persona_enabled") {
            for name in names {
                if context.enabled_personas.contains(&name) {
                    return true;
                }
            }
            return false;
        }
        
        // Default: condition not recognized, enable by default
        true
    }
    
    /// Parse function arguments from condition string
    fn parse_function_args(condition: &str, func_name: &str) -> Option<Vec<String>> {
        let prefix = format!("{}(", func_name);
        if !condition.starts_with(&prefix) || !condition.ends_with(')') {
            return None;
        }
        
        let args_str = &condition[prefix.len()..condition.len() - 1];
        let args: Vec<String> = args_str
            .split(',')
            .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        Some(args)
    }
}

/// Context for evaluating persona enable conditions
pub struct PromptEnableContext {
    pub user_input: String,
    pub enabled_personas: Vec<String>,
    pub available_tools: Vec<String>,
}

/// Runtime persona instance
pub struct Persona {
    pub config: PersonaConfig,
}

impl Persona {
    /// Create a new persona from config
    pub fn new(config: PersonaConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Get system prompt
    pub fn system_prompt(&self) -> &str {
        &self.config.system_prompt
    }
}

// Forward declarations - will be implemented in context module
/// Pipeline context (forward declared to avoid circular dependency)
#[allow(dead_code)]
pub struct PipelineContext {
    pub user_input: String,
    pub variables: HashMap<String, Value>,
    pub call_stack: Vec<String>,
    pub tool_results: Vec<ToolResult>,
    pub metadata: PipelineMetadata,
}

#[allow(dead_code)]
pub struct PipelineMetadata {
    pub started_at: chrono::DateTime<chrono::Local>,
    pub total_steps: Option<u32>,
    pub current_goal: String,
    pub completed_steps: Vec<String>,
}

#[allow(dead_code)]
pub struct ToolResult {
    pub tool_name: String,
    pub result: String,
    pub success: bool,
}
