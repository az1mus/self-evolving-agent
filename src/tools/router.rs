/// Tool router for intelligent tool selection
/// Uses keyword matching to filter relevant tools based on user input

use crate::tools::Tool;
use std::fs;
use std::path::{Path, PathBuf};

/// Metadata for each tool
pub struct ToolMeta {
    pub name: &'static str,
    pub keywords: &'static [&'static str],
}

/// Predefined tool metadata
const TOOL_METAS: &[ToolMeta] = &[
    ToolMeta {
        name: "get_current_datetime",
        keywords: &["时间", "日期", "当前", "几点", "today", "time", "date", "now"],
    },
    ToolMeta {
        name: "read_file",
        keywords: &["读取", "打开", "查看", "文件", "内容", "read", "file", "open", "view"],
    },
    ToolMeta {
        name: "write_file",
        keywords: &["写入", "保存", "创建", "文件", "write", "save", "create", "file"],
    },
    ToolMeta {
        name: "list_files",
        keywords: &["列出", "目录", "文件", "哪些", "list", "files", "directory", "folder"],
    },
    ToolMeta {
        name: "execute_shell",
        keywords: &["执行", "命令", "shell", "cmd", "run", "command", "terminal"],
    },
    ToolMeta {
        name: "execute_python",
        keywords: &["python", "脚本", "py", "execute python", "run python", "执行 python"],
    },
];

/// Router for selecting relevant tools based on user input
pub struct ToolRouter;

impl ToolRouter {
    /// Create a new ToolRouter
    pub fn new() -> Self {
        Self
    }

    /// Select relevant tool names based on user message
    pub fn select_tools(&self, user_message: &str) -> Vec<&'static str> {
        let message_lower = user_message.to_lowercase();
        
        TOOL_METAS
            .iter()
            .filter(|meta| {
                meta.keywords.iter().any(|kw| {
                    let kw_lower = kw.to_lowercase();
                    message_lower.contains(&kw_lower)
                })
            })
            .map(|meta| meta.name)
            .collect()
    }

    /// Get filtered tools from tool manager based on selected names
    pub fn get_filtered_tools<'a>(
        &self,
        tool_manager: &'a crate::tools::ToolManager,
        selected_names: &[&str],
    ) -> Vec<&'a Box<dyn Tool>> {
        tool_manager
            .get_available_tools()
            .into_iter()
            .filter(|tool| selected_names.contains(&tool.name()))
            .collect()
    }
}

impl Default for ToolRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a Python script in the library
#[derive(Debug, Clone)]
pub struct PythonScriptInfo {
    pub name: String,
    pub path: PathBuf,
    pub description: String,
}

/// Scan Python scripts from the library directory
pub fn scan_python_scripts(scripts_dir: &Path) -> Vec<PythonScriptInfo> {
    let mut scripts = Vec::new();

    if !scripts_dir.exists() {
        log::info!("Python scripts directory does not exist: {:?}", scripts_dir);
        return scripts;
    }

    for entry in fs::read_dir(scripts_dir).ok().into_iter().flatten() {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("py") {
                // Extract script name (without .py extension)
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Try to extract description from docstring
                let description = extract_script_description(&path)
                    .unwrap_or_else(|| "Python script".to_string());

                log::debug!("Found Python script: {} at {:?}", name, path);
                scripts.push(PythonScriptInfo { name, path, description });
            }
        }
    }

    scripts
}

/// Extract description from Python script docstring
fn extract_script_description(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    
    // Find docstring (triple quotes)
    let start = content.find("\"\"\"")?;
    let end = content[start + 3..].find("\"\"\"")?;
    let docstring = &content[start + 3..start + 3 + end];
    
    // Get first non-empty line as description
    docstring
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|s| s.trim().to_string())
}

/// Match user query against available Python scripts with scoring
pub fn match_python_scripts<'a>(query: &'a str, scripts: &'a [PythonScriptInfo]) -> Vec<&'a PythonScriptInfo> {
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
        .filter(|s| !s.is_empty())
        .collect();
    
    let mut scored_scripts: Vec<(&PythonScriptInfo, i32)> = scripts
        .iter()
        .filter_map(|script| {
            let score = calculate_match_score(&query_lower, &query_words, script);
            if score > 0 {
                Some((script, score))
            } else {
                None
            }
        })
        .collect();
    
    // Sort by score (highest first)
    scored_scripts.sort_by(|a, b| b.1.cmp(&a.1));
    
    // Return top matches (limit to 5 to avoid overwhelming LLM)
    scored_scripts
        .into_iter()
        .take(5)
        .map(|(script, _)| script)
        .collect()
}

/// Calculate match score for a script against query
fn calculate_match_score(query_lower: &str, query_words: &[&str], script: &PythonScriptInfo) -> i32 {
    let name_lower = script.name.to_lowercase();
    let desc_lower = script.description.to_lowercase();
    let mut score = 0i32;
    
    // Exact name match (highest priority)
    if name_lower == query_lower {
        score += 100;
    }
    
    // Name contains query (high priority)
    if name_lower.contains(query_lower) {
        score += 50;
    }
    
    // Check each query word
    for word in query_words {
        if word.len() < 2 {
            continue; // Skip very short words
        }
        
        // Word in name (high priority)
        if name_lower.contains(word) {
            score += 30;
        }
        
        // Word in description (medium priority)
        if desc_lower.contains(word) {
            score += 10;
        }
    }
    
    // Check for common variations (weather/天气，calculate/计算)
    score += check_semantic_matches(query_lower, &name_lower, &desc_lower);
    
    score
}

/// Check for common semantic matches (Chinese-English variations)
fn check_semantic_matches(query: &str, name: &str, desc: &str) -> i32 {
    let mut score = 0i32;
    
    // Common semantic pairs
    let semantic_pairs = [
        ("天气", "weather"),
        ("时间", "time"),
        ("日期", "date"),
        ("文件", "file"),
        ("计算", "calc"),
        ("转换", "convert"),
        ("下载", "download"),
        ("搜索", "search"),
        ("邮件", "email"),
        ("图片", "image"),
    ];
    
    for (cn, en) in semantic_pairs.iter() {
        // If query contains Chinese and name/desc contains English (or vice versa)
        if (query.contains(cn) && (name.contains(en) || desc.contains(en))) ||
           (query.contains(en) && (name.contains(cn) || desc.contains(cn))) {
            score += 20;
        }
    }
    
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_tools_file_query() {
        let router = ToolRouter;
        let tools = router.select_tools("列出当前目录的文件");
        assert!(tools.contains(&"list_files"));
    }

    #[test]
    fn test_select_tools_time_query() {
        let router = ToolRouter;
        let tools = router.select_tools("现在几点了");
        assert!(tools.contains(&"get_current_datetime"));
    }

    #[test]
    fn test_select_tools_no_match() {
        let router = ToolRouter;
        let tools = router.select_tools("你好，打招呼");
        assert!(tools.is_empty());
    }
}
