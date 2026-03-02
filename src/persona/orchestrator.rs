/// Orchestrator - manages persona execution and model scheduling
use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::manager::PersonaManager;
use super::persona::Persona;
use super::prompt_assembler::{PromptAssembler, PersonaComponent, PromptContext};
use super::dependency_resolver::DependencyResolver;
use crate::api::{Message, ToolCall};
use crate::session::SessionManager;
use crate::tools::ToolManager;

/// Result of persona execution
#[derive(Debug)]
pub struct ExecutionResult {
    /// Content/output from persona
    pub content: String,
    /// Tool calls to execute (if any)
    pub tool_calls: Vec<ToolCall>,
    /// Next persona to invoke (None = end of pipeline)
    pub next_persona: Option<String>,
    /// Whether to end the pipeline
    pub end_pipeline: bool,
}

impl ExecutionResult {
    pub fn with_tool_calls(content: &str, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            content: content.to_string(),
            tool_calls,
            next_persona: None,
            end_pipeline: false,
        }
    }

    pub fn end(content: &str) -> Self {
        Self {
            content: content.to_string(),
            tool_calls: Vec::new(),
            next_persona: None,
            end_pipeline: true,
        }
    }

    pub fn with_next(content: &str, next_persona: &str) -> Self {
        Self {
            content: content.to_string(),
            tool_calls: Vec::new(),
            next_persona: Some(next_persona.to_string()),
            end_pipeline: false,
        }
    }
}

/// Orchestrator manages persona execution and model scheduling
pub struct Orchestrator {
    /// Persona manager
    persona_manager: Arc<Mutex<PersonaManager>>,
    /// Maximum persona chain depth
    max_depth: usize,
    /// Prompt assembler for building final prompts
    prompt_assembler: PromptAssembler,
    /// Dependency resolver for managing persona dependencies
    dependency_resolver: DependencyResolver,
}

impl Orchestrator {
    /// Create a new orchestrator
    pub fn new(persona_manager: Arc<Mutex<PersonaManager>>) -> Self {
        let mut prompt_assembler = PromptAssembler::new();
        let mut dependency_resolver = DependencyResolver::new();
        
        // Load personas into assembler and resolver
        // Note: This is done asynchronously in initialize method
        Self {
            persona_manager,
            max_depth: 10,
            prompt_assembler,
            dependency_resolver,
        }
    }
    
    /// Initialize the orchestrator (load personas into assembler and resolver)
    pub async fn initialize(&mut self) -> Result<()> {
        log::info!("Initializing orchestrator...");

        // Get all personas
        let pm = self.persona_manager.lock().await;
        let personas: Vec<_> = pm.list_personas()
            .iter()
            .filter_map(|name| pm.get_config(name))
            .cloned()
            .collect();
        
        // Build dependency graph
        self.dependency_resolver.build_from_personas(&personas)?;

        // Register components in assembler
        for config in &personas {
            // Create new persona instance from config
            if let Ok(persona) = Persona::new(config.clone()) {
                let persona_arc = Arc::new(Mutex::new(persona));
                let component = Arc::new(PersonaComponent::new(persona_arc, config.clone()));
                self.prompt_assembler.register_component(component);
            }
        }
        drop(pm);

        log::info!("Orchestrator initialized with {} personas", personas.len());
        Ok(())
    }

    /// Execute a persona pipeline
    pub async fn execute_pipeline(
        &self,
        start_persona: &str,
        user_input: &str,
        messages: &[Message],
        client: &crate::api::APIClient,
        tool_manager: &ToolManager,
        tool_router: &crate::tools::router::ToolRouter,
        session_manager: &mut SessionManager,
        scripts_dir: &std::path::PathBuf,
    ) -> Result<()> {
        log::info!("Starting persona pipeline from: {}", start_persona);

        let mut current_persona = start_persona.to_string();
        let mut current_messages: Vec<Message> = messages.to_vec();
        let mut depth = 0;
        // Track persona call history for cycle detection
        let mut call_history: Vec<String> = Vec::new();
        // Track consecutive retries to prevent infinite loops
        let mut consecutive_retries = 0;
        const MAX_CONSECUTIVE_RETRIES: usize = 2;
        // Store summarized context from planner
        let mut summarized_context: Option<String> = None;
        // Store prompt updates for specific personas (from error_handler suggestions)
        let mut prompt_updates: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        loop {
            if depth > self.max_depth {
                log::warn!("Max persona depth reached: {}", self.max_depth);
                break;
            }

            // Cycle detection: check if we're repeating the same persona sequence
            if call_history.len() >= 3 {
                let recent = call_history.iter().rev().take(3).cloned().collect::<Vec<_>>();
                if recent.len() == 3 && recent.iter().all(|p| p == &current_persona) {
                    log::warn!("Cycle detected: persona '{}' repeated {} times", current_persona, recent.len());
                    println!(
                        "\n{}",
                        format!("═══ 检测到循环调用，终止流程 ═══").red().bold()
                    );
                    break;
                }
            }

            // Track retries for cycle prevention
            if current_persona == "tool_selector" && call_history.last() == Some(&"error_handler".to_string()) {
                consecutive_retries += 1;
                if consecutive_retries >= MAX_CONSECUTIVE_RETRIES {
                    log::warn!("Max consecutive retries reached: {}", consecutive_retries);
                    println!(
                        "\n{}",
                        format!("═══ 重试次数过多 ({} 次)，终止流程 ═══", consecutive_retries).red().bold()
                    );
                    break;
                }
            } else {
                consecutive_retries = 0;
            }

            call_history.push(current_persona.clone());
            log::info!("Step {}: Executing persona '{}' (history: {:?})", depth + 1, current_persona, call_history);

            // Execute current persona based on type
            let result = if current_persona == "intent_detector" {
                // Build context with summarized information and prompt updates
                let extra_ctx = if let Some(ref summary) = summarized_context {
                    let mut ctx = summary.clone();
                    // Append prompt updates if any
                    if let Some(update) = prompt_updates.get("intent_detector") {
                        ctx.push_str(&format!("\n\n## 特别提示\n{}", update));
                    }
                    Some(ctx)
                } else {
                    prompt_updates.get("intent_detector").map(|s| s.clone())
                };
                
                let intent_messages = self.build_context_for_persona(
                    "intent_detector",
                    &current_messages,
                    user_input,
                    extra_ctx.as_deref(),
                ).await?;

                // Execute API call
                let response = client.chat_completion(&intent_messages, None, false).await?;

                // Record API request with payload and response
                let request_payload = json!({
                    "messages": intent_messages.iter().map(|m| {
                        json!({
                            "role": m.role,
                            "content": m.content
                        })
                    }).collect::<Vec<_>>()
                });
                let response_payload = json!({
                    "content": response.content,
                    "tool_calls": response.tool_calls,
                    "usage": response.usage
                });
                let _ = session_manager.record_api_request(
                    client.get_model(),
                    "intent_detection",
                    intent_messages.len(),
                    None,
                    response.usage.as_ref().map(|u| u.completion_tokens),
                    None,
                    Some(request_payload),
                    Some(response_payload),
                );

                let exec_result_content = response.content.unwrap_or_default();

                // Check if tools are needed
                if exec_result_content.trim_start().starts_with("[TOOL_NEEDED]") {
                    log::info!("Tool needed detected");
                    ExecutionResult::with_next(&exec_result_content, "tool_selector")
                } else {
                    log::info!("No tool needed, direct response");
                    ExecutionResult::end(&exec_result_content)
                }
            }
            else if current_persona == "tool_selector" {
                // Get available tools
                let selected_tool_names = tool_router.select_tools(user_input);
                let filtered_tools = tool_router.get_filtered_tools(tool_manager, &selected_tool_names);

                if filtered_tools.is_empty() {
                    log::warn!("No tools selected, trying intent extraction");
                    ExecutionResult::with_next("", "intent_extractor")
                } else {
                    // Build context for tool selector with tool list
                    let tool_names: Vec<&str> = filtered_tools.iter().map(|t| t.name()).collect();
                    let tool_context = format!(
                        "用户需要：{}。可用工具：[{}]",
                        user_input,
                        tool_names.join(", ")
                    );

                    let tool_messages = self.build_context_for_persona(
                        "tool_selector",
                        &current_messages,
                        user_input,
                        Some(&tool_context),
                    ).await?;

                    let response = client.chat_completion(&tool_messages, Some(&filtered_tools), false).await?;

                    // Record API request with payload and response
                    let request_payload = json!({
                        "messages": tool_messages.iter().map(|m| {
                            json!({
                                "role": m.role,
                                "content": m.content,
                                "name": m.name
                            })
                        }).collect::<Vec<_>>(),
                        "tools": tool_names
                    });
                    let response_payload = json!({
                        "content": response.content,
                        "tool_calls": response.tool_calls,
                        "usage": response.usage
                    });
                    let _ = session_manager.record_api_request(
                        client.get_model(),
                        "tool_selection",
                        tool_messages.len(),
                        Some(tool_names.iter().map(|s| s.to_string()).collect()),
                        response.usage.as_ref().map(|u| u.completion_tokens),
                        None,
                        Some(request_payload),
                        Some(response_payload),
                    );

                    // Extract tool calls
                    let tool_calls = response.tool_calls.clone().unwrap_or_default();
                    if !tool_calls.is_empty() {
                        ExecutionResult::with_tool_calls(&response.content.unwrap_or_default(), tool_calls)
                    } else {
                        // No tool calls, try intent extraction
                        ExecutionResult::with_next("", "intent_extractor")
                    }
                }
            }
            else if current_persona == "intent_extractor" {
                // Build minimal context for intent extractor
                let extract_messages = self.build_context_for_persona(
                    "intent_extractor",
                    &current_messages,
                    user_input,
                    None,
                ).await?;

                let response = client.chat_completion(&extract_messages, None, false).await?;

                // Record API request with payload and response
                let request_payload = json!({
                    "messages": extract_messages.iter().map(|m| {
                        json!({
                            "role": m.role,
                            "content": m.content
                        })
                    }).collect::<Vec<_>>()
                });
                let response_payload = json!({
                    "content": response.content,
                    "tool_calls": response.tool_calls,
                    "usage": response.usage
                });
                let _ = session_manager.record_api_request(
                    client.get_model(),
                    "intent_extraction",
                    extract_messages.len(),
                    None,
                    response.usage.as_ref().map(|u| u.completion_tokens),
                    None,
                    Some(request_payload),
                    Some(response_payload),
                );

                // After extraction, go to python_generator
                ExecutionResult::with_next(&response.content.unwrap_or_default(), "python_generator")
            }
            else if current_persona == "python_generator" {
                // Build context with summarized information and prompt updates
                let extra_ctx = if let Some(ref summary) = summarized_context {
                    let mut ctx = summary.clone();
                    // Append prompt updates if any
                    if let Some(update) = prompt_updates.get("python_generator") {
                        ctx.push_str(&format!("\n\n## 特别提示\n{}", update));
                    }
                    Some(ctx)
                } else {
                    prompt_updates.get("python_generator").map(|s| s.clone())
                };
                
                let gen_messages = self.build_context_for_persona(
                    "python_generator",
                    &current_messages,
                    user_input,
                    extra_ctx.as_deref(),
                ).await?;

                let gen_response = client
                    .chat_completion(&gen_messages, None, false)
                    .await?;

                // Record API request with payload and response
                let request_payload = json!({
                    "messages": gen_messages.iter().map(|m| {
                        json!({
                            "role": m.role,
                            "content": m.content
                        })
                    }).collect::<Vec<_>>(),
                    "request_type": "python_script_generation"
                });
                let response_payload = json!({
                    "content": gen_response.content,
                    "tool_calls": gen_response.tool_calls,
                    "usage": gen_response.usage
                });
                let _ = session_manager.record_api_request(
                    client.get_model(),
                    "python_script_generation",
                    gen_messages.len(),
                    None,
                    gen_response.usage.as_ref().map(|u| u.completion_tokens),
                    None,
                    Some(request_payload),
                    Some(response_payload),
                );

                let script_response = gen_response.content.unwrap_or_default();

                // Audit python_generator output with error_handler
                log::info!("🔍 Auditing python_generator output...");
                let audit_result = self
                    .execute_error_handler_audit(
                        &script_response,
                        user_input,
                        "python_generator",
                        &current_messages,
                        client,
                    )
                    .await;

                // Record audit to session
                let _ = session_manager.record_tool_call(
                    "error_handler_audit",
                    json!({
                        "persona_type": "python_generator",
                        "user_input": user_input,
                        "model_output_length": script_response.len()
                    }),
                    &audit_result.as_ref().unwrap_or(&"Audit failed to execute".to_string()),
                    audit_result.is_ok(),
                    audit_result.as_ref().err().map(|e| e.to_string()),
                    None,
                );

                // Check audit result and determine action
                let (should_retry, should_abort) = if let Ok(audit_output) = &audit_result {
                    // Log audit details
                    log::info!("📋 Audit output received: {}", audit_output);

                    // Extract JSON from markdown code blocks
                    let json_content = Self::extract_json_from_markdown(audit_output);
                    log::debug!("📄 Extracted JSON: {}", json_content);

                    if let Ok(audit_json) = serde_json::from_str::<serde_json::Value>(&json_content) {
                        // Parse action
                        let action = audit_json
                            .get("action")
                            .and_then(|v| v.as_str())
                            .unwrap_or("pass");

                        // Parse logs
                        let log_level = audit_json
                            .get("logs")
                            .and_then(|l| l.get("level"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("info");
                        let log_summary = audit_json
                            .get("logs")
                            .and_then(|l| l.get("summary"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let log_details = audit_json
                            .get("logs")
                            .and_then(|l| l.get("details"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        // Log based on level
                        match log_level {
                            "error" => log::error!("❌ {}", log_summary),
                            "warn" => log::warn!("⚠️ {}", log_summary),
                            _ => log::info!("✅ {}", log_summary),
                        }
                        if !log_details.is_empty() {
                            log::debug!("   └─ Details: {}", log_details);
                        }

                        // Log suggestions
                        if let Some(suggestions) = audit_json.get("suggestions").and_then(|v| v.as_array()) {
                            for suggestion in suggestions {
                                if let Some(s) = suggestion.as_str() {
                                    log::info!("💡 Suggestion: {}", s);
                                }
                            }
                        }

                        // Handle different actions
                        match action {
                            "abort" => {
                                log::warn!("🛑 Audit suggests ABORT");
                                if let Some(user_msg) = audit_json.get("user_message").and_then(|v| v.as_str()) {
                                    log::warn!("   └─ User message: {}", user_msg);
                                }
                                (false, true)
                            }
                            "ask_user" => {
                                log::info!("❓ Audit suggests ASK_USER");
                                
                                // Extract user inquiry details
                                let question = audit_json
                                    .get("user_inquiry")
                                    .and_then(|i| i.get("question"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("请提供更多信息以帮助完成任务");
                                let expected_info = audit_json
                                    .get("user_inquiry")
                                    .and_then(|i| i.get("expected_info"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("补充信息");
                                let examples: Vec<String> = audit_json
                                    .get("user_inquiry")
                                    .and_then(|i| i.get("examples"))
                                    .and_then(|v| v.as_array())
                                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
                                    .unwrap_or_default();
                                
                                // Log inquiry details
                                log::info!("   └─ Question: {}", question);
                                log::info!("   └─ Expected info: {}", expected_info);
                                if !examples.is_empty() {
                                    log::info!("   └─ Examples: {}", examples.join(", "));
                                }
                                
                                // Store inquiry state in session for resumption
                                let inquiry_state = json!({
                                    "type": "user_inquiry",
                                    "question": question,
                                    "expected_info": expected_info,
                                    "examples": examples,
                                    "persona_at": current_persona,
                                    "messages": current_messages,
                                    "user_input": user_input
                                });
                                
                                // Record inquiry to session
                                let _ = session_manager.record_tool_call(
                                    "user_inquiry",
                                    inquiry_state,
                                    question,
                                    true,
                                    None,
                                    None,
                                );
                                
                                // Output message for user
                                let mut user_msg = format!("❓ {}\n\n期望提供：{}", question, expected_info);
                                if !examples.is_empty() {
                                    user_msg.push_str(&format!("\n示例：{}", examples.join(", ")));
                                }
                                user_msg.push_str("\n\n请补充信息后继续。");
                                
                                println!("\n{}", user_msg);
                                log::info!("⏸️  Waiting for user input...");
                                
                                // End pipeline, waiting for user input
                                // The session is saved with inquiry state for resumption
                                return Ok(());
                            }
                            "retry" => {
                                log::info!("🔄 Audit suggests RETRY");

                                // Handle prompt update - store in prompt_updates for target persona
                                if let Some(prompt_update) = audit_json.get("prompt_update") {
                                    let should_update = prompt_update
                                        .get("should_update")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);

                                    if should_update {
                                        let update_type = prompt_update
                                            .get("update_type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("append");
                                        let content = prompt_update
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");

                                        if !content.is_empty() {
                                            log::info!("📝 Updating prompt (type: {})", update_type);
                                            
                                            // Determine target persona for the update
                                            let target_persona = audit_json
                                                .get("retry_persona")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or(&current_persona);

                                            // Store prompt update for the target persona
                                            // This will be appended when that persona is called
                                            prompt_updates.entry(target_persona.to_string())
                                                .and_modify(|existing| {
                                                    existing.push_str(&format!("\n\n{}", content));
                                                })
                                                .or_insert_with(|| content.to_string());
                                            
                                            log::info!("   └─ Prompt update stored for persona: {}", target_persona);
                                        }
                                    }
                                }
                                (true, false)
                            }
                            "fallback" => {
                                log::info!("⬇️ Audit suggests FALLBACK");
                                // For now, treat fallback as retry
                                (true, false)
                            }
                            "pass" => {
                                log::info!("✅ Audit PASSED");
                                (false, false)
                            }
                            _ => {
                                log::warn!("⚠️ Unknown action: '{}', proceeding as pass", action);
                                (false, false)
                            }
                        }
                    } else {
                        log::warn!("⚠️ Failed to parse audit JSON after extraction");
                        (false, false)
                    }
                } else {
                    log::warn!("⚠️ Audit execution failed: {:?}", audit_result.err());
                    (false, false)
                };

                // If audit suggests abort, end pipeline
                if should_abort {
                    log::info!("🛑 Aborting pipeline based on audit");
                    return Ok(());
                }

                // If audit suggests retry, go back to python_generator
                if should_retry {
                    log::info!("↩️ Retrying python_generator based on audit");
                    depth += 1;
                    continue;
                }

                // Parse response to extract script_name, keywords, script_args and code
                let (script_name, keywords, script_args, script_code_clean) = Self::parse_python_generator_response(&script_response);

                let script_path = scripts_dir.join(format!("{}.py", script_name));
                let meta_path = scripts_dir.join(format!("{}.json", script_name));

                // Ensure scripts directory exists
                if let Some(parent) = script_path.parent() {
                    if !parent.exists() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }

                // Save Python script
                match std::fs::write(&script_path, &script_code_clean) {
                    Ok(_) => {
                        log::info!("Saved Python script to {:?}", script_path);

                        // Save metadata file for SEA executor
                        let metadata = json!({
                            "name": script_name,
                            "keywords": keywords,
                            "parameters": script_args,
                            "created_at": chrono::Local::now().to_rfc3339(),
                            "script_file": format!("{}.py", script_name)
                        });

                        match std::fs::write(&meta_path, metadata.to_string()) {
                            Ok(_) => log::info!("Saved metadata to {:?}", meta_path),
                            Err(e) => log::warn!("Failed to save metadata: {}", e),
                        }

                        // Execute Python script directly (not via tool call)
                        let exec_result = Self::execute_python_script_direct(
                            &script_path,
                            &script_args,
                        );

                        match exec_result {
                            Ok(output) => {
                                log::info!("Script executed successfully: {}", script_path.display());

                                // Record tool execution to session
                                let _ = session_manager.record_tool_call(
                                    "execute_python",
                                    json!({
                                        "script_path": script_path.to_string_lossy(),
                                        "args": script_args
                                    }),
                                    &output,
                                    true,
                                    None,
                                    None,
                                );

                                // Add result to messages for final responder
                                current_messages.push(Message::new(
                                    "user",
                                    &format!("脚本执行结果：{}", output),
                                ));

                                // Go to final responder
                                current_persona = "final_responder".to_string();
                                depth += 1;
                                continue;
                            }
                            Err(e) => {
                                log::warn!("Script execution failed: {}", e);

                                // Record failed execution
                                let _ = session_manager.record_tool_call(
                                    "execute_python",
                                    json!({
                                        "script_path": script_path.to_string_lossy(),
                                        "args": script_args
                                    }),
                                    "",
                                    false,
                                    Some(e.to_string()),
                                    None,
                                );

                                ExecutionResult::end(&format!(
                                    "脚本执行失败：{}\n脚本路径：{:?}",
                                    e, script_path
                                ))
                            }
                        }
                    }
                    Err(e) => {
                        ExecutionResult::end(&format!("Failed to save script: {}", e))
                    }
                }
            }
            else if current_persona == "final_responder" {
                // Build context with summarized information and prompt updates
                let extra_ctx = if let Some(ref summary) = summarized_context {
                    let mut ctx = summary.clone();
                    // Append prompt updates if any
                    if let Some(update) = prompt_updates.get("final_responder") {
                        ctx.push_str(&format!("\n\n## 特别提示\n{}", update));
                    }
                    Some(ctx)
                } else {
                    prompt_updates.get("final_responder").map(|s| s.clone())
                };
                
                let final_messages = self.build_context_for_persona(
                    "final_responder",
                    &current_messages,
                    user_input,
                    extra_ctx.as_deref(),
                ).await?;

                let response = client
                    .chat_completion(&final_messages, None, false)
                    .await?;

                let response_content = response.content.unwrap_or_default();

                // Audit final_responder output with error_handler
                log::info!("🔍 Auditing final_responder output...");
                let audit_result = self
                    .execute_error_handler_audit(
                        &response_content,
                        user_input,
                        "final_responder",
                        &current_messages,
                        client,
                    )
                    .await;

                // Record audit to session
                let _ = session_manager.record_tool_call(
                    "error_handler_audit",
                    json!({
                        "persona_type": "final_responder",
                        "user_input": user_input,
                        "response_length": response_content.len()
                    }),
                    &audit_result.as_ref().unwrap_or(&"Audit failed to execute".to_string()),
                    audit_result.is_ok(),
                    audit_result.as_ref().err().map(|e| e.to_string()),
                    None,
                );

                // Check audit result and determine action
                let (should_retry, should_abort) = if let Ok(audit_output) = &audit_result {
                    // Log audit details
                    log::info!("📋 Audit output received: {}", audit_output);

                    // Extract JSON from markdown code blocks
                    let json_content = Self::extract_json_from_markdown(audit_output);
                    log::debug!("📄 Extracted JSON: {}", json_content);

                    if let Ok(audit_json) = serde_json::from_str::<serde_json::Value>(&json_content) {
                        // Parse action
                        let action = audit_json
                            .get("action")
                            .and_then(|v| v.as_str())
                            .unwrap_or("pass");

                        // Parse logs
                        let log_level = audit_json
                            .get("logs")
                            .and_then(|l| l.get("level"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("info");
                        let log_summary = audit_json
                            .get("logs")
                            .and_then(|l| l.get("summary"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let log_details = audit_json
                            .get("logs")
                            .and_then(|l| l.get("details"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        // Log based on level
                        match log_level {
                            "error" => log::error!("❌ {}", log_summary),
                            "warn" => log::warn!("⚠️ {}", log_summary),
                            _ => log::info!("✅ {}", log_summary),
                        }
                        if !log_details.is_empty() {
                            log::debug!("   └─ Details: {}", log_details);
                        }

                        // Log suggestions
                        if let Some(suggestions) = audit_json.get("suggestions").and_then(|v| v.as_array()) {
                            for suggestion in suggestions {
                                if let Some(s) = suggestion.as_str() {
                                    log::info!("💡 Suggestion: {}", s);
                                }
                            }
                        }

                        // Handle different actions
                        match action {
                            "abort" => {
                                log::warn!("🛑 Audit suggests ABORT");
                                if let Some(user_msg) = audit_json.get("user_message").and_then(|v| v.as_str()) {
                                    log::warn!("   └─ User message: {}", user_msg);
                                }
                                (false, true)
                            }
                            "ask_user" => {
                                log::info!("❓ Audit suggests ASK_USER");
                                
                                // Extract user inquiry details
                                let question = audit_json
                                    .get("user_inquiry")
                                    .and_then(|i| i.get("question"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("请提供更多信息以帮助完成任务");
                                let expected_info = audit_json
                                    .get("user_inquiry")
                                    .and_then(|i| i.get("expected_info"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("补充信息");
                                let examples: Vec<String> = audit_json
                                    .get("user_inquiry")
                                    .and_then(|i| i.get("examples"))
                                    .and_then(|v| v.as_array())
                                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect())
                                    .unwrap_or_default();
                                
                                // Log inquiry details
                                log::info!("   └─ Question: {}", question);
                                log::info!("   └─ Expected info: {}", expected_info);
                                if !examples.is_empty() {
                                    log::info!("   └─ Examples: {}", examples.join(", "));
                                }
                                
                                // Store inquiry state in session for resumption
                                let inquiry_state = json!({
                                    "type": "user_inquiry",
                                    "question": question,
                                    "expected_info": expected_info,
                                    "examples": examples,
                                    "persona_at": current_persona,
                                    "messages": current_messages,
                                    "user_input": user_input
                                });
                                
                                // Record inquiry to session
                                let _ = session_manager.record_tool_call(
                                    "user_inquiry",
                                    inquiry_state,
                                    question,
                                    true,
                                    None,
                                    None,
                                );
                                
                                // Output message for user
                                let mut user_msg = format!("❓ {}\n\n期望提供：{}", question, expected_info);
                                if !examples.is_empty() {
                                    user_msg.push_str(&format!("\n示例：{}", examples.join(", ")));
                                }
                                user_msg.push_str("\n\n请补充信息后继续。");
                                
                                println!("\n{}", user_msg);
                                log::info!("⏸️  Waiting for user input...");
                                
                                // End pipeline, waiting for user input
                                return Ok(());
                            }
                            "retry" => {
                                log::info!("🔄 Audit suggests RETRY");

                                // Handle prompt update - store in prompt_updates for target persona
                                if let Some(prompt_update) = audit_json.get("prompt_update") {
                                    let should_update = prompt_update
                                        .get("should_update")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);

                                    if should_update {
                                        let update_type = prompt_update
                                            .get("update_type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("append");
                                        let content = prompt_update
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");

                                        if !content.is_empty() {
                                            log::info!("📝 Updating prompt (type: {})", update_type);
                                            
                                            // Determine target persona for the update
                                            let target_persona = audit_json
                                                .get("retry_persona")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or(&current_persona);

                                            // Store prompt update for the target persona
                                            prompt_updates.entry(target_persona.to_string())
                                                .and_modify(|existing| {
                                                    existing.push_str(&format!("\n\n{}", content));
                                                })
                                                .or_insert_with(|| content.to_string());
                                            
                                            log::info!("   └─ Prompt update stored for persona: {}", target_persona);
                                        }
                                    }
                                }
                                (true, false)
                            }
                            "fallback" => {
                                log::info!("⬇️ Audit suggests FALLBACK");
                                (true, false)
                            }
                            "pass" => {
                                log::info!("✅ Audit PASSED");
                                (false, false)
                            }
                            _ => {
                                log::warn!("⚠️ Unknown action: '{}', proceeding as pass", action);
                                (false, false)
                            }
                        }
                    } else {
                        log::warn!("⚠️ Failed to parse audit JSON after extraction");
                        (false, false)
                    }
                } else {
                    log::warn!("⚠️ Audit execution failed: {:?}", audit_result.err());
                    (false, false)
                };

                // If audit suggests abort, end pipeline
                if should_abort {
                    log::info!("🛑 Aborting pipeline based on audit");
                    return Ok(());
                }

                // If audit suggests retry, go back to final_responder
                if should_retry {
                    log::info!("↩️ Retrying final_responder based on audit");
                    depth += 1;
                    continue;
                }

                // Record API request with payload and response
                let request_payload = json!({
                    "messages": final_messages.iter().map(|m| {
                        json!({
                            "role": m.role,
                            "content": m.content
                        })
                    }).collect::<Vec<_>>()
                });
                let response_payload = json!({
                    "content": response_content,
                    "tool_calls": response.tool_calls,
                    "usage": response.usage
                });
                let _ = session_manager.record_api_request(
                    client.get_model(),
                    "final_response",
                    final_messages.len(),
                    None,
                    response.usage.as_ref().map(|u| u.completion_tokens),
                    None,
                    Some(request_payload),
                    Some(response_payload),
                );

                ExecutionResult::end(&response_content)
            }
            else {
                log::warn!("Unknown persona: {}", current_persona);
                break;
            };

            // Handle tool calls
            if !result.tool_calls.is_empty() {
                log::info!("Executing {} tool calls", result.tool_calls.len());
                for tool_call in &result.tool_calls {
                    match tool_manager.execute_tool_call(
                        &tool_call.function.name,
                        &tool_call.function.arguments,
                    ) {
                        Ok(tool_result) => {
                            log::info!(
                                "Tool {} executed successfully",
                                tool_call.function.name
                            );

                            // Add tool result to messages
                            current_messages.push(Message::with_tool_result(
                                "tool",
                                &tool_result,
                                &tool_call.id,
                                &tool_call.function.name,
                            ));

                            // Record tool execution to session
                            let _ = session_manager.record_tool_call(
                                &tool_call.function.name,
                                serde_json::Value::String(tool_call.function.arguments.clone()),
                                &tool_result,
                                true,
                                None,
                                None,
                            );
                        }
                        Err(e) => {
                            log::warn!(
                                "Tool {} failed: {}",
                                tool_call.function.name,
                                e
                            );

                            // Record failed tool execution to session
                            let _ = session_manager.record_tool_call(
                                &tool_call.function.name,
                                serde_json::Value::String(tool_call.function.arguments.clone()),
                                "",
                                false,
                                Some(e.to_string()),
                                None,
                            );
                        }
                    }
                }

                // After tool execution, evaluate with error_handler before final response
                log::info!("Invoking error_handler to evaluate tool execution results");
                
                // Collect tool execution results for evaluation
                let tool_results: Vec<String> = current_messages
                    .iter()
                    .filter(|m| m.role == "tool")
                    .map(|m| m.content.clone())
                    .collect();
                
                // Invoke error_handler for evaluation
                let eval_result = self
                    .execute_error_handler_evaluation(
                        &current_messages,
                        user_input,
                        &tool_results,
                        client,
                    )
                    .await;
                
                match eval_result {
                    Ok(eval_output) => {
                        // Parse evaluation result
                        if let Ok(eval_json) = serde_json::from_str::<serde_json::Value>(&eval_output) {
                            let evaluation = eval_json
                                .get("evaluation")
                                .and_then(|v| v.as_str())
                                .unwrap_or("pass");
                            
                            if evaluation == "fail" {
                                log::warn!("Error handler detected failure in tool execution results");
                                
                                // Handle based on suggested action
                                let suggested_action = eval_json
                                    .get("suggested_action")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("terminate");
                                
                                match suggested_action {
                                    "retry" => {
                                        log::info!("Error handler suggests retry, adjusting parameters");
                                        // Add error analysis to context for retry
                                        if let Some(fix_strategy) = eval_json.get("fix_strategy").and_then(|v| v.as_str()) {
                                            current_messages.push(Message::new(
                                                "user",
                                                &format!("根据错误分析，请调整策略：{}", fix_strategy),
                                            ));
                                        }
                                        // Reset consecutive retries counter when error_handler explicitly suggests retry
                                        // The cycle detection will handle infinite loops
                                        current_persona = "tool_selector".to_string();
                                        depth += 1;
                                        continue;
                                    }
                                    "alternative" => {
                                        log::info!("Error handler suggests alternative approach");
                                        if let Some(fix_strategy) = eval_json.get("fix_strategy").and_then(|v| v.as_str()) {
                                            current_messages.push(Message::new(
                                                "user",
                                                &format!("尝试替代方案：{}", fix_strategy),
                                            ));
                                        }
                                        // Alternative approach - reset retry counter
                                        consecutive_retries = 0;
                                        current_persona = "tool_selector".to_string();
                                        depth += 1;
                                        continue;
                                    }
                                    "ask_user" => {
                                        log::info!("Error handler requires user input");
                                        if let Some(reason) = eval_json.get("failure_reason").and_then(|v| v.as_str()) {
                                            println!(
                                                "\n{}",
                                                format!("═══ 需要用户输入 ═══").yellow().bold()
                                            );
                                            println!("{}", reason);
                                            println!("{}", "═══════════════════════════════════════".yellow());
                                        }
                                        // After asking user, proceed to final responder
                                    }
                                    _ => {
                                        log::info!("Error handler suggests termination or no action specified");
                                    }
                                }
                            } else {
                                log::info!("Error handler evaluation passed, proceeding to final response");
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Error handler evaluation failed: {}", e);
                        // Continue to final response anyway
                    }
                }
                
                // After tool execution and evaluation, generate final response
                current_persona = "final_responder".to_string();
                depth += 1;
                continue;
            }

            // Check if pipeline should end
            if result.end_pipeline {
                log::info!("Pipeline ended by persona");
                // Output final content
                if !result.content.is_empty() {
                    println!(
                        "\n{}",
                        format!("═══ Assistant ═══").blue().bold()
                    );
                    println!("{}", result.content);
                    println!("{}", "═══════════════════════════════════════".blue());

                    // Record to session
                    let _ = session_manager.add_message(Message::new("assistant", &result.content));
                }
                break;
            }

            // Determine next persona
            if let Some(next) = result.next_persona {
                // If current persona is planner, summarize context for next personas
                if current_persona == "planner" && summarized_context.is_none() {
                    log::info!("📝 Planner summarizing conversation context...");
                    match self.summarize_context(&current_messages, client).await {
                        Ok(summary) => {
                            log::info!("Context summarized successfully");
                            summarized_context = Some(summary);
                        }
                        Err(e) => {
                            log::warn!("Failed to summarize context: {}", e);
                        }
                    }
                }
                
                current_persona = next;
                depth += 1;
            } else {
                // No next persona specified, determine based on content
                current_persona = self.determine_next_persona(&result.content, &current_persona);
                if current_persona.is_empty() {
                    break;
                }
                depth += 1;
            }
        }

        Ok(())
    }

    /// Build context messages for specific persona
    /// Each persona has different context requirements
    /// Build context messages for persona (async version)
    async fn build_context_for_persona(
        &self,
        persona_name: &str,
        current_messages: &[Message],
        user_input: &str,
        extra_context: Option<&str>,
    ) -> Result<Vec<Message>> {
        let mut messages = Vec::new();

        // Add system prompt (async)
        let system_prompt = self.get_persona_prompt(persona_name).await;
        if let Ok(prompt) = system_prompt {
            messages.push(Message::new("system", &prompt));
        }

        match persona_name {
            // Intent Detector: Only needs current user input + summarized context
            "intent_detector" => {
                // Extract summarized context from extra_context
                let context_msg = if let Some(ctx) = extra_context {
                    format!("## 上下文重要信息\n{}\n\n## 用户当前请求\n{}", ctx, user_input)
                } else {
                    user_input.to_string()
                };
                messages.push(Message::new("user", &context_msg));
            }

            // Tool Selector: Needs user input + available tools context
            "tool_selector" => {
                if let Some(ctx) = extra_context {
                    messages.push(Message::new("user", ctx));
                } else {
                    messages.push(Message::new("user", user_input));
                }
            }

            // Intent Extractor: Only needs current user input + summarized context
            "intent_extractor" => {
                let context_msg = if let Some(ctx) = extra_context {
                    format!("## 上下文重要信息\n{}\n\n## 请从以下用户输入中提取意图和参数\n{}", ctx, user_input)
                } else {
                    format!("请从以下用户输入中提取意图和参数：{}", user_input)
                };
                messages.push(Message::new("user", &context_msg));
            }

            // Python Generator: Needs user input + extracted intent (if available)
            "python_generator" => {
                if let Some(ctx) = extra_context {
                    messages.push(Message::new("user", ctx));
                } else {
                    messages.push(Message::new(
                        "user",
                        &format!("请编写一个 Python 脚本来完成以下任务：{}", user_input),
                    ));
                }
            }

            // Final Responder: Only receives summarized important information
            "final_responder" => {
                // Use summarized context from extra_context instead of full conversation
                let context_msg = if let Some(ctx) = extra_context {
                    format!("## 上下文重要信息\n{}\n\n## 用户原始请求\n{}\n\n请根据以上信息和工具执行结果，给用户一个清晰、完整的回答。", ctx, user_input)
                } else {
                    format!("用户请求：{}\n\n请给用户一个清晰、完整的回答。", user_input)
                };
                messages.push(Message::new("user", &context_msg));
            }

            // Planner: Sees full conversation flow
            "planner" => {
                // Include full conversation history
                messages.extend(current_messages.iter().cloned());
            }

            // Error Handler: Gets context for audit/evaluation
            "error_handler" => {
                // Include relevant context for error analysis
                messages.extend(current_messages.iter().cloned());
            }

            // Default: Use limited context (last 5 messages)
            _ => {
                let recent_messages = current_messages
                    .iter()
                    .rev()
                    .take(5)
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>();
                messages.extend(recent_messages);
            }
        }

        Ok(messages)
    }

    /// Get persona system prompt (async)
    async fn get_persona_prompt(&self, persona_name: &str) -> Result<String> {
        let pm = self.persona_manager.lock().await;
        let persona = pm.get_persona(persona_name)
            .with_context(|| format!("Persona not found: {}", persona_name))?;
        Ok(persona.system_prompt().to_string())
    }

    /// Summarize important information from conversation context
    /// Used by planner to pass to other personas
    async fn summarize_context(&self, messages: &[Message], client: &crate::api::APIClient) -> Result<String> {
        // Get planner system prompt
        let planner_prompt = self.get_persona_prompt("planner").await?;

        // Build conversation summary input
        let conversation = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let summary_input = format!(
            r#"请总结以下对话历史中的重要信息。

## 对话历史
{}

## 输出要求

请提取并总结以下信息（JSON 格式）：
```json
{{
  "user_goal": "用户的核心目标",
  "key_parameters": {{"参数名": "参数值"}},
  "completed_steps": ["已完成的任务步骤"],
  "pending_steps": ["待完成的任务步骤"],
  "tool_results": ["重要工具执行结果摘要"],
  "constraints": ["限制条件或注意事项"]
}}
```

只返回 JSON 内容，不要有其他解释。"#,
            conversation
        );

        let summary_messages = vec![
            Message::new("system", &planner_prompt),
            Message::new("user", &summary_input),
        ];

        let response = client
            .chat_completion(&summary_messages, None, false)
            .await?;

        let content = response.content.unwrap_or_default();
        let json_content = Self::extract_json_from_markdown(&content);

        // Parse and format summary for display
        if let Ok(summary_json) = serde_json::from_str::<serde_json::Value>(&json_content) {
            let mut summary_parts = Vec::new();

            if let Some(goal) = summary_json.get("user_goal").and_then(|v| v.as_str()) {
                summary_parts.push(format!("**用户目标**: {}", goal));
            }
            if let Some(params) = summary_json.get("key_parameters") {
                summary_parts.push(format!("**关键参数**: {}", params));
            }
            if let Some(completed) = summary_json.get("completed_steps").and_then(|v| v.as_array()) {
                if !completed.is_empty() {
                    let steps: Vec<String> = completed.iter().filter_map(|v| v.as_str()).map(String::from).collect();
                    summary_parts.push(format!("**已完成**: {}", steps.join(" → ")));
                }
            }
            if let Some(pending) = summary_json.get("pending_steps").and_then(|v| v.as_array()) {
                if !pending.is_empty() {
                    let steps: Vec<String> = pending.iter().filter_map(|v| v.as_str()).map(String::from).collect();
                    summary_parts.push(format!("**待完成**: {}", steps.join(" → ")));
                }
            }
            if let Some(results) = summary_json.get("tool_results").and_then(|v| v.as_array()) {
                if !results.is_empty() {
                    let results_str: Vec<String> = results.iter().filter_map(|v| v.as_str()).map(String::from).collect();
                    summary_parts.push(format!("**工具结果**: {}", results_str.join("; ")));
                }
            }
            if let Some(constraints) = summary_json.get("constraints").and_then(|v| v.as_array()) {
                if !constraints.is_empty() {
                    let constraints_str: Vec<String> = constraints.iter().filter_map(|v| v.as_str()).map(String::from).collect();
                    summary_parts.push(format!("**注意事项**: {}", constraints_str.join("; ")));
                }
            }

            if summary_parts.is_empty() {
                Ok("无特殊重要信息".to_string())
            } else {
                Ok(summary_parts.join("\n"))
            }
        } else {
            // Fallback: return condensed conversation
            Ok(format!("对话摘要：{}", conversation.chars().take(500).collect::<String>()))
        }
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

    /// Parse response from python_generator persona
    /// Expected format:
    /// [NAME] filename.py
    /// [KEYWORDS] keyword1, keyword2, ...
    /// [PARAMETERS] {"key": "value"}
    /// [CODE]
    /// ```python
    /// # code here
    /// ```
    /// Returns (script_name, keywords, script_args, code) tuple
    fn parse_python_generator_response(content: &str) -> (String, Vec<String>, serde_json::Value, String) {
        let mut script_name = String::new();
        let mut keywords = Vec::new();
        let mut script_args = json!({});

        // Parse metadata lines
        for line in content.lines() {
            let trimmed = line.trim();
            
            if trimmed.starts_with("[NAME]") {
                script_name = trimmed[6..]
                    .trim()
                    .strip_suffix(".py")
                    .unwrap_or(trimmed[6..].trim())
                    .to_string();
            } else if trimmed.starts_with("[KEYWORDS]") {
                keywords = trimmed[10..]
                    .trim()
                    .split(',')
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if trimmed.starts_with("[PARAMETERS]") {
                let params_str = trimmed[12..].trim();
                if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(params_str) {
                    script_args = json_value;
                }
            } else if trimmed.starts_with("[CODE]") {
                break;
            }
        }

        // Extract code from markdown block
        let code = Self::extract_code_from_markdown(content);

        // Fallback for script_name if not provided
        if script_name.is_empty() {
            script_name = Self::generate_script_name_fallback(&code);
        }

        (script_name, keywords, script_args, code)
    }

    /// Generate a script name as fallback when JSON parsing fails
    fn generate_script_name_fallback(content: &str) -> String {
        // Common Chinese to English mappings
        let chinese_to_english = [
            ("天气", "weather"), ("气候", "weather"), ("温度", "temperature"),
            ("文件", "file"), ("文档", "document"), ("转换", "convert"),
            ("计算", "calculate"), ("查询", "query"), ("搜索", "search"),
            ("下载", "download"), ("上传", "upload"), ("读取", "read"),
            ("写入", "write"), ("创建", "create"), ("删除", "delete"),
        ];

        // Skip shebang line and docstring, focus on actual code
        let code_only = Self::extract_code_logic(content);

        let mut processed = code_only;
        for (cn, en) in chinese_to_english.iter() {
            processed = processed.replace(cn, en);
        }

        // Sanitize: convert to lowercase and replace invalid chars with underscore
        let sanitized: String = processed
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();

        // Collapse consecutive underscores and trim
        let cleaned: String = sanitized
            .split('_')
            .filter(|s| !s.is_empty())
            .take(4)  // Limit to 4 words for shorter names
            .collect::<Vec<_>>()
            .join("_");

        if cleaned.is_empty() {
            format!("script_{}", chrono::Local::now().format("%Y%m%d_%H%M%S"))
        } else {
            cleaned
        }
    }

    /// Extract actual code logic, skipping shebang and docstring
    fn extract_code_logic(content: &str) -> String {
        let mut result = String::new();
        let mut in_docstring = false;
        let mut docstring_char = '\"';

        for line in content.lines() {
            let trimmed = line.trim();

            // Skip shebang line
            if trimmed.starts_with("#!") {
                continue;
            }

            // Skip encoding declarations
            if trimmed.starts_with("# -*- coding:") || trimmed.starts_with("# coding:") {
                continue;
            }

            // Handle docstrings (triple quotes)
            if !in_docstring {
                if trimmed.starts_with("\"\"\"") {
                    in_docstring = true;
                    docstring_char = '\"';
                    // Check if docstring ends on same line
                    if trimmed.ends_with("\"\"\"") && trimmed.len() > 6 {
                        in_docstring = false;
                    }
                    continue;
                } else if trimmed.starts_with("'''") {
                    in_docstring = true;
                    docstring_char = '\'';
                    // Check if docstring ends on same line
                    if trimmed.ends_with("'''") && trimmed.len() > 6 {
                        in_docstring = false;
                    }
                    continue;
                }
            } else {
                // Inside docstring, check for end
                let end_marker = if docstring_char == '\"' { "\"\"\"" } else { "'''" };
                if trimmed.ends_with(end_marker) {
                    in_docstring = false;
                }
                continue;
            }

            // Skip single-line comments
            if trimmed.starts_with('#') {
                continue;
            }

            // Keep actual code lines (imports, function definitions, logic)
            result.push_str(line);
            result.push('\n');
        }

        result
    }

    /// Determine next persona based on content
    fn determine_next_persona(&self, content: &str, _current: &str) -> String {
        // Simple routing logic
        if content.trim_start().starts_with("[TOOL_NEEDED]") {
            return "tool_selector".to_string();
        }

        // Default: end pipeline
        String::new()
    }

    /// Execute Python script directly (not via tool call)
    fn execute_python_script_direct(
        script_path: &std::path::Path,
        script_args: &serde_json::Value,
    ) -> Result<String> {
        use std::process::Command;

        // Validate script exists
        if !script_path.exists() {
            return Ok(format!("Error: Script not found: {}", script_path.display()));
        }

        if !script_path.is_file() {
            return Ok(format!("Error: Not a file: {}", script_path.display()));
        }

        // Serialize args to JSON string
        let args_json = serde_json::to_string(script_args)
            .with_context(|| "Failed to serialize arguments")?;

        // Execute Python script
        let output = Command::new("python")
            .arg(script_path)
            .arg(&args_json)
            .output()
            .with_context(|| format!("Failed to execute script: {}", script_path.display()))?;

        let mut result = String::from_utf8_lossy(&output.stdout).to_string();

        if !output.stderr.is_empty() {
            result.push_str("\nSTDERR: ");
            result.push_str(&String::from_utf8_lossy(&output.stderr));
        }

        if !output.status.success() {
            result = format!(
                "Script failed with code {}\n{}",
                output.status.code().unwrap_or(-1),
                result
            );
        }

        Ok(result)
    }

    /// Extract JSON from markdown code blocks
    fn extract_json_from_markdown(content: &str) -> String {
        // Try to find ```json block first
        if let Some(start) = content.find("```json") {
            let rest = &content[start + 7..];
            if let Some(end) = rest.find("```") {
                return rest[..end].trim().to_string();
            }
        }
        // Fallback to generic ``` block
        if let Some(start) = content.find("```") {
            let rest = &content[start + 3..];
            if let Some(end) = rest.find("```") {
                return rest[..end].trim().to_string();
            }
        }
        // No markdown found, return original
        content.trim().to_string()
    }

    /// Execute error_handler to evaluate model output (for tool execution results)
    /// Returns the evaluation result as JSON string
    async fn execute_error_handler_evaluation(
        &self,
        messages: &[Message],
        user_input: &str,
        tool_results: &[String],
        client: &crate::api::APIClient,
    ) -> Result<String> {
        // Get error_handler system prompt
        let system_prompt = self.get_persona_prompt("error_handler").await?;

        // Build evaluation context
        let model_output = messages
            .iter()
            .filter(|m| m.role == "assistant" || m.role == "tool")
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n");

        // Build error_handler input
        let eval_input = format!(
            r#"请评估以下模型生成内容是否达到用户预期。

## 用户原始请求
{}

## 模型输出/工具执行结果
{}

## 可用工具结果
{}

请输出 JSON 格式的评估结果，包含：
- evaluation: "pass" 或 "fail"
- failure_type: 如果失败，说明类型 (refusal/error/format/incomplete)
- failure_reason: 具体原因分析
- suggested_action: "retry" / "alternative" / "ask_user" / "terminate"
- fix_strategy: 具体修复步骤
- needs_orchestrator_collaboration: true 或 false

特别注意：如果输出包含"我无法"、"我不能"、"无法"、"错误"、"失败"等关键词，应标记为失败。"#,
            user_input,
            model_output,
            tool_results.join("\n")
        );

        // Build messages for error_handler
        let eval_messages = vec![
            Message::new("system", &system_prompt),
            Message::new("user", &eval_input),
        ];

        // Call API
        let response = client
            .chat_completion(&eval_messages, None, false)
            .await?;

        Ok(response.content.unwrap_or_default())
    }

    /// Execute error_handler to audit specific persona output
    /// Returns the audit result as JSON string
    async fn execute_error_handler_audit(
        &self,
        model_output: &str,
        user_input: &str,
        persona_type: &str,
        messages: &[Message],
        client: &crate::api::APIClient,
    ) -> Result<String> {
        // Get error_handler system prompt
        let system_prompt = self.get_persona_prompt("error_handler").await?;

        // Get recent conversation context
        let context = messages
            .iter()
            .rev()
            .take(5)
            .rev()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        // Build audit-specific input
        let audit_input = format!(
            r#"请审计以下 persona 的输出质量。

## 用户原始请求
{}

## Persona 类型
{}

## 上下文对话
{}

## Persona 输出
{}

请根据 persona 类型的审计要点进行检查，返回严格的 JSON 格式（不要用 markdown 包裹）：

{{
  "action": "abort" | "retry" | "fallback" | "pass" | "ask_user",
  "error_type": "recoverable" | "needs_user_input" | "fatal" | "none",
  "error_message": "原始错误信息（如果没有错误则为 null）",
  "diagnosis": "错误诊断分析",
  "prompt_update": {{
    "should_update": true | false,
    "update_type": "append" | "replace" | "clarify",
    "content": "具体的提示词更新内容，用于指导下一次生成"
  }},
  "user_inquiry": {{
    "needed": true | false,
    "question": "向用户提出的问题",
    "expected_info": "期望用户提供的信息类型",
    "examples": ["示例 1", "示例 2"]
  }},
  "logs": {{
    "level": "info" | "warn" | "error",
    "summary": "简要日志摘要",
    "details": "详细的分析过程"
  }},
  "suggestions": [
    "建议 1: 具体的改进建议",
    "建议 2: 另一个建议"
  ],
  "retry_persona": "应该重试的 persona 名称（如果需要重试）",
  "retry_count": 0,
  "max_retries": 3,
  "fallback_persona": "替代 persona 名称（如果需要）",
  "user_message": "给用户的消息（如果需要用户输入）"
}}

审计要点：
- python_generator: 检查 [NAME], [KEYWORDS], [PARAMETERS], [CODE] 格式，参数提取是否正确，代码是否完整可运行
- final_responder: 检查是否完整回答用户问题，是否总结工具执行结果
- planner: 检查任务分解是否合理，子任务是否可执行

如果输出不符合要求，action 设为 "retry"，prompt_update.should_update 设为 true，并在 content 中说明如何修改提示词。
如果需要用户询问信息，action 设为 "ask_user"，并填写 user_inquiry 字段。
如果建议重试，请设置 retry_persona 为目标 persona 名称。"#,
            user_input,
            persona_type,
            context,
            model_output
        );

        // Build messages for error_handler
        let eval_messages = vec![
            Message::new("system", &system_prompt),
            Message::new("user", &audit_input),
        ];

        // Call API
        let response = client
            .chat_completion(&eval_messages, None, false)
            .await?;

        Ok(response.content.unwrap_or_default())
    }
    
    // ═══════════════════════════════════════════════════════════════════════
    // New Method: Execute using Prompt Assembler (v0.2 feature)
    // ═══════════════════════════════════════════════════════════════════════
    
    /// Execute using prompt assembler with dependency management
    /// 
    /// This is the new execution method that uses the prompt assembler to build
    /// the final prompt from multiple persona components with position control
    /// and dependency management.
    pub async fn execute_with_prompt_assembler(
        &self,
        user_input: &str,
        messages: &[Message],
        client: &crate::api::APIClient,
        session_manager: &mut SessionManager,
    ) -> Result<String> {
        log::info!("Executing with prompt assembler for: {}", user_input);
        
        // Build prompt context
        let mut ctx = PromptContext::new(user_input);
        ctx = ctx.with_history(messages.to_vec());
        
        // Set enabled personas (all by default)
        let pm = self.persona_manager.lock().await;
        ctx.enabled_personas = pm.list_personas().iter().map(|s| s.to_string()).collect();
        drop(pm);
        
        // Assemble final prompt
        let assembled_messages = self.prompt_assembler.assemble(&ctx).await?;
        
        log::info!("Assembled {} messages for API call", assembled_messages.len());
        
        // Call API with assembled messages
        let response = client
            .chat_completion(&assembled_messages, None, false)
            .await?;
        
        let content = response.content.unwrap_or_default();
        
        // Record to session
        let _ = session_manager.add_message(Message::new("assistant", &content));
        
        Ok(content)
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self {
            persona_manager: Arc::new(Mutex::new(PersonaManager::new().unwrap())),
            max_depth: 10,
            prompt_assembler: PromptAssembler::new(),
            dependency_resolver: DependencyResolver::new(),
        }
    }
}
