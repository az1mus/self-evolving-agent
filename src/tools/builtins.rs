/// Built-in tools for SEA (Self-Evolved Agent)
use anyhow::{Context, Result};
use chrono::Local;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::tools::Tool;

/// Tool to get the current date and time
pub struct DateTimeTool;

impl Tool for DateTimeTool {
    fn name(&self) -> &str {
        "get_current_datetime"
    }

    fn description(&self) -> &str {
        "Get the current date and time"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(&self, _args: &Value) -> Result<String> {
        Ok(Local::now().to_rfc3339())
    }
}

/// Tool to read a file
pub struct FileReadTool;

impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read"
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: &Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .with_context(|| "Path is required")?;

        let file_path = PathBuf::from(path);

        if !file_path.exists() {
            return Ok(format!("Error: File does not exist: {}", path));
        }

        if !file_path.is_file() {
            return Ok(format!("Error: Path is not a file: {}", path));
        }

        // Check file size to prevent reading very large files
        let metadata = fs::metadata(&file_path)?;
        if metadata.len() > 1024 * 1024 {
            // 1MB limit
            return Ok(format!("Error: File too large to read: {}", path));
        }

        let content = fs::read_to_string(&file_path)?;
        Ok(content)
    }
}

/// Tool to write content to a file
pub struct FileWriteTool;

impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn execute(&self, args: &Value) -> Result<String> {
        let path = args["path"]
            .as_str()
            .with_context(|| "Path is required")?;
        let content = args["content"]
            .as_str()
            .with_context(|| "Content is required")?;

        let file_path = PathBuf::from(path);

        // Create parent directories if they don't exist
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&file_path, content)?;
        Ok(format!(
            "Successfully wrote {} characters to {}",
            content.len(),
            path
        ))
    }
}

/// Tool to list files in a directory
pub struct ListFilesTool;

impl Tool for ListFilesTool {
    fn name(&self) -> &str {
        "list_files"
    }

    fn description(&self) -> &str {
        "List files in a directory"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the directory to list (default: current directory)"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Whether to list files recursively (default: false)"
                }
            },
            "required": []
        })
    }

    fn execute(&self, args: &Value) -> Result<String> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let recursive = args
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let dir_path = PathBuf::from(path);

        if !dir_path.exists() {
            return Ok(format!("Error: Directory does not exist: {}", path));
        }

        if !dir_path.is_dir() {
            return Ok(format!("Error: Path is not a directory: {}", path));
        }

        let mut files = Vec::new();
        if recursive {
            for entry in fs::read_dir(&dir_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    if let Ok(rel_path) = path.strip_prefix(&dir_path) {
                        files.push(rel_path.to_string_lossy().to_string());
                    }
                } else if path.is_dir() {
                    // Recursively list files in subdirectory
                    if let Ok(sub_files) = Self::list_recursive(&path, &dir_path) {
                        files.extend(sub_files);
                    }
                }
            }
        } else {
            for entry in fs::read_dir(&dir_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_name() {
                        files.push(name.to_string_lossy().to_string());
                    }
                }
            }
        }

        if files.is_empty() {
            return Ok(format!("No files found in {}", path));
        }

        Ok(files.join("\n"))
    }
}

impl ListFilesTool {
    fn list_recursive(dir: &PathBuf, base: &PathBuf) -> Result<Vec<String>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Ok(rel_path) = path.strip_prefix(base) {
                    files.push(rel_path.to_string_lossy().to_string());
                }
            } else if path.is_dir() {
                files.extend(Self::list_recursive(&path, base)?);
            }
        }
        Ok(files)
    }
}

/// Tool to execute shell commands
pub struct ShellTool;

impl Tool for ShellTool {
    fn name(&self) -> &str {
        "execute_shell"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return the output"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                }
            },
            "required": ["command"]
        })
    }

    fn execute(&self, args: &Value) -> Result<String> {
        let command = args["command"]
            .as_str()
            .with_context(|| "Command is required")?;

        // Security check: only allow safe commands
        let dangerous_patterns = ["rm ", "mv ", "dd ", "kill ", "reboot", "shutdown"];
        let command_lower = command.to_lowercase();
        for pattern in &dangerous_patterns {
            if command_lower.contains(pattern) {
                return Ok(format!(
                    "Error: Command contains potentially dangerous pattern: {}",
                    command
                ));
            }
        }

        // Execute command based on platform
        let output = if cfg!(windows) {
            Command::new("cmd").args(["/C", command]).output()
        } else {
            Command::new("sh").args(["-c", command]).output()
        }?;

        let mut result = String::from_utf8_lossy(&output.stdout).to_string();

        if !output.stderr.is_empty() {
            result.push_str("\nSTDERR: ");
            result.push_str(&String::from_utf8_lossy(&output.stderr));
        }

        if !output.status.success() {
            result = format!(
                "Command failed with return code {}\n{}",
                output.status.code().unwrap_or(-1),
                result
            );
        }

        Ok(result)
    }
}

/// Register all built-in tools with the tool manager
pub fn register_builtin_tools(tool_manager: &mut crate::tools::ToolManager) {
    let builtin_tools: Vec<Box<dyn Tool>> = vec![
        Box::new(DateTimeTool),
        Box::new(FileReadTool),
        Box::new(FileWriteTool),
        Box::new(ListFilesTool),
        Box::new(ShellTool),
    ];

    for tool in builtin_tools {
        tool_manager.register_tool(tool);
    }
}
