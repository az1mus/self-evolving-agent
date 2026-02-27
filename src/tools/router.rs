/// Tool router for intelligent tool selection
/// Uses keyword matching to filter relevant tools based on user input

use crate::tools::Tool;

/// Tool category for classification
#[derive(Debug, Clone, PartialEq)]
pub enum ToolCategory {
    DateTime,
    FileSystem,
    System,
}

/// Metadata for each tool
pub struct ToolMeta {
    pub name: &'static str,
    #[allow(dead_code)]
    pub category: ToolCategory,
    pub keywords: &'static [&'static str],
}

/// Predefined tool metadata
const TOOL_METAS: &[ToolMeta] = &[
    ToolMeta {
        name: "get_current_datetime",
        category: ToolCategory::DateTime,
        keywords: &["时间", "日期", "当前", "几点", "today", "time", "date", "now"],
    },
    ToolMeta {
        name: "read_file",
        category: ToolCategory::FileSystem,
        keywords: &["读取", "打开", "查看", "文件", "内容", "read", "file", "open", "view"],
    },
    ToolMeta {
        name: "write_file",
        category: ToolCategory::FileSystem,
        keywords: &["写入", "保存", "创建", "文件", "write", "save", "create", "file"],
    },
    ToolMeta {
        name: "list_files",
        category: ToolCategory::FileSystem,
        keywords: &["列出", "目录", "文件", "哪些", "list", "files", "directory", "folder"],
    },
    ToolMeta {
        name: "execute_shell",
        category: ToolCategory::System,
        keywords: &["执行", "命令", "shell", "cmd", "run", "command", "terminal"],
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

    /// Check if user message indicates tool usage intent
    #[allow(dead_code)]
    pub fn needs_tools(&self, user_message: &str) -> bool {
        // If any keywords match, tools might be needed
        !self.select_tools(user_message).is_empty()
    }
}

impl Default for ToolRouter {
    fn default() -> Self {
        Self::new()
    }
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
