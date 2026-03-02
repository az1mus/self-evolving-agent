/// Prompt Assembler - builds final prompt from multiple components
/// 
/// This module provides the trait and assembler for building prompts
/// from multiple persona components with position control and dependency management.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::persona::{Persona, PersonaConfig, PositionMode};
use crate::api::Message;

/// Tool result for prompt context
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_name: String,
    pub result: String,
    pub success: bool,
}

/// Prompt context - contains all data needed for building prompts
pub struct PromptContext {
    /// User's current input
    pub user_input: String,
    /// Conversation history
    pub conversation_history: Vec<Message>,
    /// Tool execution results
    pub tool_results: Vec<ToolResult>,
    /// Local retrieval results (from local database)
    pub local_retrieval: Option<String>,
    /// Online retrieval results (from web search)
    pub online_retrieval: Option<String>,
    /// Conversation summary (from Planner persona)
    pub conversation_summary: Option<String>,
    /// Context/scenario information
    pub context_info: Option<String>,
    /// Variable substitutions
    pub variables: HashMap<String, String>,
    /// Enabled persona names
    pub enabled_personas: Vec<String>,
    /// Available tool names
    pub available_tools: Vec<String>,
}

impl PromptContext {
    pub fn new(user_input: &str) -> Self {
        Self {
            user_input: user_input.to_string(),
            conversation_history: Vec::new(),
            tool_results: Vec::new(),
            local_retrieval: None,
            online_retrieval: None,
            conversation_summary: None,
            context_info: None,
            variables: HashMap::new(),
            enabled_personas: Vec::new(),
            available_tools: Vec::new(),
        }
    }
    
    /// Set conversation history
    pub fn with_history(mut self, history: Vec<Message>) -> Self {
        self.conversation_history = history;
        self
    }
    
    /// Add tool result
    pub fn add_tool_result(&mut self, tool_name: &str, result: &str, success: bool) {
        self.tool_results.push(ToolResult {
            tool_name: tool_name.to_string(),
            result: result.to_string(),
            success,
        });
    }
    
    /// Set variable
    pub fn set_variable(&mut self, key: &str, value: &str) {
        self.variables.insert(key.to_string(), value.to_string());
    }
}

/// Prompt position in the final prompt
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PromptPosition {
    /// Fixed at header (earliest)
    Header,
    /// Relative position with order value
    Relative(u32, u32), // (order, inner_order)
    /// In chat history at specified depth
    InChat { depth: u32, order: u32 },
    /// Fixed at footer (latest)
    Footer,
}

/// Prompt component trait - defines interface for prompt building blocks
pub trait PromptComponent: Send + Sync {
    /// Component name
    fn name(&self) -> &str;

    /// Whether this component should be included
    fn should_include(&self, ctx: &PromptContext) -> bool;

    /// Build component content
    fn build(&self, ctx: &PromptContext) -> Result<String>;

    /// Get component position
    fn position(&self) -> PromptPosition;

    /// Get component dependencies
    fn dependencies(&self) -> Vec<&str>;
    
    /// Get as any for downcasting
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Persona-based prompt component
pub struct PersonaComponent {
    pub persona: Arc<Mutex<Persona>>,
    pub config: PersonaConfig,
}

impl PersonaComponent {
    pub fn new(persona: Arc<Mutex<Persona>>, config: PersonaConfig) -> Self {
        Self { persona, config }
    }
}

impl PromptComponent for PersonaComponent {
    fn name(&self) -> &str {
        &self.config.name
    }
    
    fn should_include(&self, ctx: &PromptContext) -> bool {
        // Check if persona is explicitly disabled
        if !ctx.enabled_personas.is_empty() 
            && !ctx.enabled_personas.contains(&self.config.name) {
            return false;
        }
        
        // Check trigger conditions
        let enable_ctx = super::persona::PromptEnableContext {
            user_input: ctx.user_input.clone(),
            enabled_personas: ctx.enabled_personas.clone(),
            available_tools: ctx.available_tools.clone(),
        };
        
        self.config.should_enable(&enable_ctx)
    }
    
    fn build(&self, _ctx: &PromptContext) -> Result<String> {
        // Return the system prompt for this persona
        // Variables will be substituted by the assembler
        Ok(self.config.system_prompt.clone())
    }
    
    fn position(&self) -> PromptPosition {
        let pos = self.config.get_position();
        
        match pos.mode {
            PositionMode::Header => PromptPosition::Header,
            PositionMode::Footer => PromptPosition::Footer,
            PositionMode::Relative => {
                let order = pos.order.unwrap_or(100);
                let inner_order = pos.inner_order.unwrap_or(0);
                PromptPosition::Relative(order, inner_order)
            }
            PositionMode::InChat => {
                let depth = pos.depth.unwrap_or(0);
                let order = pos.inner_order.unwrap_or(0);
                PromptPosition::InChat { depth, order }
            }
        }
    }
    
    fn dependencies(&self) -> Vec<&str> {
        self.config.dependencies
            .as_ref()
            .map(|d| d.requires.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Prompt assembler - builds final prompt from components
pub struct PromptAssembler {
    components: Vec<Arc<dyn PromptComponent>>,
}

impl PromptAssembler {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }
    
    /// Register a prompt component
    pub fn register_component(&mut self, component: Arc<dyn PromptComponent>) {
        log::info!("Registered prompt component: {}", component.name());
        self.components.push(component);
    }
    
    /// Build final prompt from components
    pub async fn assemble(&self, ctx: &PromptContext) -> Result<Vec<Message>> {
        log::info!("Assembling prompt from {} components", self.components.len());
        
        // 1. Filter enabled components
        let enabled_components: Vec<_> = self.components
            .iter()
            .filter(|c| c.should_include(ctx))
            .collect();
        
        log::debug!("Enabled components: {:?}", 
            enabled_components.iter().map(|c| c.name()).collect::<Vec<_>>());
        
        // 2. Topological sort (handle dependencies)
        let sorted = self.topological_sort(&enabled_components)?;
        
        // 3. Separate InChat components from others
        let mut in_chat_components: Vec<_> = Vec::new();
        let mut regular_components: Vec<_> = Vec::new();
        
        for component in sorted {
            match component.position() {
                PromptPosition::InChat { .. } => in_chat_components.push(component),
                _ => regular_components.push(component),
            }
        }
        
        // 4. Sort regular components by position
        regular_components.sort_by(|a, b| a.position().cmp(&b.position()));
        
        // 5. Build messages from regular components
        let mut messages = Vec::new();
        
        for component in regular_components {
            let content = component.build(ctx)?;
            let content = self.substitute_variables(&content, ctx);
            messages.push(Message::new("system", &content));
        }
        
        // 6. Add conversation history with InChat components inserted
        messages = self.insert_in_chat_components(
            messages,
            &in_chat_components,
            ctx,
        )?;
        
        // 7. Add user input at the end
        messages.push(Message::new("user", &ctx.user_input));
        
        log::info!("Assembled prompt with {} messages", messages.len());
        Ok(messages)
    }
    
    /// Topological sort to handle dependencies
    fn topological_sort<'a>(
        &self,
        components: &[&'a Arc<dyn PromptComponent>],
    ) -> Result<Vec<&'a Arc<dyn PromptComponent>>> {
        // Build adjacency list and in-degree map
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut adj_list: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut name_to_component: HashMap<&str, &'a Arc<dyn PromptComponent>> = HashMap::new();
        
        // Initialize
        for component in components {
            let name = component.name();
            in_degree.insert(name, 0);
            adj_list.insert(name, Vec::new());
            name_to_component.insert(name, component);
        }
        
        // Build graph
        for component in components {
            let name = component.name();
            for dep in component.dependencies() {
                // Only consider dependencies that exist in our component set
                if name_to_component.contains_key(dep) {
                    adj_list.entry(dep).or_default().push(name);
                    *in_degree.entry(name).or_insert(0) += 1;
                }
            }
        }
        
        // Kahn's algorithm
        let mut queue: std::collections::VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(name, _)| *name)
            .collect();
        
        let mut result = Vec::new();
        
        while let Some(node) = queue.pop_front() {
            result.push(name_to_component[node]);
            
            if let Some(neighbors) = adj_list.get(node) {
                for neighbor in neighbors {
                    let deg = in_degree.get_mut(neighbor).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
        
        // Check for cycles
        if result.len() != components.len() {
            // Find components involved in cycle
            let all_names: std::collections::HashSet<_> = components.iter().map(|c| c.name()).collect();
            let sorted_names: std::collections::HashSet<_> = result.iter().map(|c| c.name()).collect();
            let remaining: Vec<_> = all_names
                .difference(&sorted_names)
                .copied()
                .collect();
            
            anyhow::bail!(
                "Detected circular dependency in prompt components: {:?}",
                remaining
            );
        }
        
        Ok(result)
    }
    
    /// Insert InChat components into conversation history
    fn insert_in_chat_components(
        &self,
        mut messages: Vec<Message>,
        in_chat_components: &[&Arc<dyn PromptComponent>],
        ctx: &PromptContext,
    ) -> Result<Vec<Message>> {
        if in_chat_components.is_empty() {
            return Ok(messages);
        }
        
        // Sort InChat components by depth (deeper = inserted earlier)
        let mut sorted_components: Vec<_> = in_chat_components.iter().collect();
        sorted_components.sort_by(|a, b| {
            let pos_a = a.position();
            let pos_b = b.position();
            
            match (pos_a, pos_b) {
                (PromptPosition::InChat { depth: da, order: oa },
                 PromptPosition::InChat { depth: db, order: ob }) => {
                    // Higher depth = inserted earlier in history
                    db.cmp(&da).then_with(|| oa.cmp(&ob))
                }
                _ => std::cmp::Ordering::Equal,
            }
        });
        
        // Insert components at appropriate positions
        for component in sorted_components {
            let content = component.build(ctx)?;
            let content = self.substitute_variables(&content, ctx);
            
            // Find insertion point based on depth
            let pos = component.position();
            if let PromptPosition::InChat { depth, .. } = pos {
                // Insert at depth position (0 = after all history, 1 = before last message, etc.)
                let insert_pos = messages.len().saturating_sub(depth as usize);
                messages.insert(insert_pos, Message::new("system", &content));
            }
        }
        
        Ok(messages)
    }
    
    /// Substitute variables in content
    fn substitute_variables(&self, content: &str, ctx: &PromptContext) -> String {
        let mut result = content.to_string();
        
        // Built-in macros
        result = result.replace("{{user_input}}", &ctx.user_input);
        
        // Tool results
        for tool_result in &ctx.tool_results {
            let key = format!("{{{{tool_result.{}}}}}", tool_result.tool_name);
            result = result.replace(&key, &tool_result.result);
        }
        
        // Context variables
        for (key, value) in &ctx.variables {
            let macro_key = format!("{{{{{}}}}}", key);
            result = result.replace(&macro_key, value);
        }
        
        // Persona-defined macros
        for component in &self.components {
            if let Some(persona_comp) = component.as_any().downcast_ref::<PersonaComponent>() {
                for macro_def in &persona_comp.config.macros {
                    let macro_key = format!("{{{{{}}}}}", macro_def.name);
                    result = result.replace(&macro_key, &macro_def.value);
                }
            }
        }

        result
    }
}

impl Default for PromptAssembler {
    fn default() -> Self {
        Self::new()
    }
}
