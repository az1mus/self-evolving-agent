/// Base classes and interfaces for tools in SEA (Self-Evolved Agent)
use anyhow::{Context, Result};
use serde_json::Value;

pub mod builtins;
pub mod router;

/// Abstract base trait for all tools
pub trait Tool: Send + Sync {
    /// Get the tool name
    fn name(&self) -> &str;

    /// Get the tool description
    fn description(&self) -> &str;

    /// Get the tool parameters as JSON schema
    fn parameters(&self) -> Value;

    /// Execute the tool with given arguments
    fn execute(&self, args: &Value) -> Result<String>;

    /// Validate arguments before execution
    fn validate_arguments(&self, args: &Value) -> Result<()> {
        let params = self.parameters();
        let _properties = params.get("properties").and_then(|p| p.as_object());
        let required = params
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let args_obj = args.as_object().with_context(|| "Arguments must be an object")?;

        for param in &required {
            if !args_obj.contains_key(*param) {
                anyhow::bail!("Missing required parameter: {}", param);
            }
        }

        Ok(())
    }
}

/// Manages available tools
pub struct ToolManager {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolManager {
    /// Create a new ToolManager
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
        }
    }

    /// Register a new tool
    pub fn register_tool(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        log::info!("Registered tool: {}", name);
        self.tools.push(tool);
    }

    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<&Box<dyn Tool>> {
        self.tools.iter().find(|t| t.name() == name)
    }

    /// Get list of all registered tools
    pub fn get_available_tools(&self) -> Vec<&Box<dyn Tool>> {
        self.tools.iter().collect()
    }

    /// Execute a tool call with JSON arguments string
    pub fn execute_tool_call(&self, tool_name: &str, arguments: &str) -> Result<String> {
        let tool = self
            .get_tool(tool_name)
            .with_context(|| format!("Unknown tool: {}", tool_name))?;

        let args: Value = serde_json::from_str(arguments)
            .with_context(|| format!("Invalid JSON arguments for tool {}", tool_name))?;

        tool.validate_arguments(&args)?;
        tool.execute(&args)
    }
}

impl Default for ToolManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Get a new tool manager instance
pub fn create_tool_manager() -> ToolManager {
    ToolManager::new()
}
