/// Persona management system for SEA
///
/// This module provides a flexible persona system that replaces hard-coded system prompts
/// with configurable persona definitions loaded from JSON files.
/// 
/// ## New Features (v0.2)
/// 
/// - **Prompt Position Control**: Configure where each persona's prompt appears in the final request
/// - **Dependency Management**: Define dependencies between personas for correct execution order
/// - **Context Management**: Control token budget and truncation strategies
/// - **Trigger Conditions**: Enable/disable personas based on dynamic conditions
/// - **Macro System**: Define custom variables for prompt substitution

pub mod builtins;
pub mod manager;
pub mod orchestrator;
pub mod persona;
pub mod prompt_assembler;
pub mod dependency_resolver;

pub use orchestrator::Orchestrator;
pub use prompt_assembler::{PromptAssembler, PromptContext, ToolResult as PromptToolResult};
pub use dependency_resolver::DependencyResolver;
pub use persona::{
    PersonaConfig, Persona, PersonaType, PositionMode, PromptPositionConfig,
    DependencyConfig, ContextManagementConfig, TriggerConfig, MacroConfig,
    PromptEnableContext,
};
