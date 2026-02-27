# SEA 工具调用流程设计

## 架构概述

### 角色定义

| 角色 | 职责 | 模型建议 |
|------|------|---------|
| **主模型 (Main Model)** | 用户对话、意图理解、任务规划 | 大模型 (如 GPT-4/Qwen-Max) |
| **工具模型 (Tool Model)** | 工具匹配、工具调用、上下文管理 | 小模型 (如 GPT-3.5/Qwen-Turbo) |
| **SEA (执行器)** | 协议解析、工具路由、Python 脚本执行 | - |

### 核心设计理念

1. **双模型架构** - 主模型负责对话，工具模型负责系统任务
2. **按需工具调用** - 默认不传递 tools，仅在需要时触发
3. **动态工具发现** - 基于关键词匹配工具内禀属性
4. **Python 扩展** - 用户使用 Python 编写自定义工具

---

## 完整流程图

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         单次会话流程                                     │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ 用户输入："查询北京天气"                                         │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                            │                                            │
│                            ▼                                            │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ 【阶段 1】主模型意图识别 (无 tools)                                │   │
│  │ System Prompt: "如需工具，在回复末尾输出 [NeedTools]<关键词>"    │   │
│  │                                                                 │   │
│  │ 主模型 → "[NeedTools]<天气，城市>"                               │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                            │                                            │
│                            ▼                                            │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ 【阶段 2】SEA 解析 [NeedTools] 标签                                │   │
│  │ - 提取关键词：["天气", "城市"]                                    │   │
│  │ - 根据关键词匹配工具内禀属性 (标签/分类/描述)                      │   │
│  │ - 初步筛选候选工具列表                                           │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                            │                                            │
│           ┌─────────────────┴─────────────────┐                        │
│           ▼                                   ▼                        │
│  ┌─────────────────┐                 ┌─────────────────┐              │
│  │ 有候选工具       │                 │ 无候选工具       │              │
│  └────────┬────────┘                 └────────┬────────┘              │
│           │                                   │                        │
│           ▼                                   ▼                        │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ 【阶段 3a】工具模型确认 (有候选)                                  │   │
│  │ System Prompt: "你是工具专家，从候选工具中选择最合适的"           │   │
│  │ Input: "用户需求：查询北京天气。候选工具：[get_weather, ...]"   │   │
│  │                                                                 │   │
│  │ 工具模型 → {"selected_tool": "get_weather", "args": {...}}     │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│           │                                   │                        │
│           ▼                                   ▼                        │
│  ┌─────────────────┐                 ┌─────────────────┐              │
│  │ 工具模型确认成功 │                 │ 工具模型确认失败 │              │
│  │ (选择具体工具)   │                 │ (无合适工具)     │              │
│  └────────┬────────┘                 └────────┬────────┘              │
│           │                                   │                        │
│           ▼                                   ▼                        │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ 【阶段 4a】执行工具                                               │   │
│  │ - 调用 Python 脚本：python ~/.sea/tools/weather.py --args {...} │   │
│  │ - 捕获输出/错误                                                   │   │
│  │ - 记录执行日志                                                    │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│           │                                   │                        │
│           ├─────────────────┬─────────────────┘                        │
│           ▼                 ▼                                          │
│  ┌─────────────────┐ ┌─────────────────┐                              │
│  │ 执行成功 ✓      │ │ 执行失败 ✗      │                              │
│  └────────┬────────┘ └────────┬────────┘                              │
│           │                   │                                        │
│           ▼                   ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ 【阶段 5a】结果回传主模型                                         │   │
│  │ Input: "工具执行结果：北京晴，25°C。请根据结果回答用户"           │   │
│  │                                                                 │   │
│  │ 主模型 → "北京当前天气晴朗，温度 25°C，适合外出活动。"           │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ 【阶段 3b】工具模型新建工具请求 (无候选)                          │   │
│  │ 工具模型 → "[NewTool]<天气 API, HTTP 请求，JSON 解析>"             │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│           │                                                           │
│           ▼                                                           │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ 【阶段 4b】新建工具流程 (待实现)                                  │   │
│  │ - 工具模型生成 Python 代码                                        │   │
│  │ - 用户确认/修改                                                   │   │
│  │ - 保存到 ~/.sea/tools/                                           │   │
│  │ - 注册到工具索引                                                  │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 各阶段详细设计

### 阶段 1：主模型意图识别

**System Prompt 设计:**

```rust
const SYSTEM_PROMPT_MAIN: &str = r#"你是一个智能助手。请分析用户请求并回答。

## 工具使用规则

如果你需要外部数据或执行操作才能回答，请在回复的**最后一行**输出：
[NeedTools]<关键词 1, 关键词 2, ...>

关键词应该是：
- 工具类型 (如：天气、搜索、计算)
- 所需能力 (如：HTTP 请求、文件读取、数据分析)
- 相关领域 (如：网络、本地文件、数学)

## 示例

用户："查询北京天气"
你："我需要获取天气信息。
[NeedTools]<天气，HTTP API, 城市>"

用户："你好"
你："你好！有什么可以帮助你的吗？"

用户："计算 1+1"
你："1+1=2"
"#;
```

**输出解析代码:**

```rust
// 解析主模型响应
fn parse_intent(response: &str) -> Intent {
    let lines: Vec<&str> = response.lines().collect();
    let last_line = lines.last().unwrap_or(&"").trim();
    
    if let Some(keywords_str) = last_line.strip_prefix("[NeedTools]") {
        let keywords: Vec<String> = keywords_str
            .trim()
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
        Intent::NeedTools { keywords }
    } else {
        Intent::DirectAnswer { content: response.to_string() }
    }
}

#[derive(Debug, Clone)]
pub enum Intent {
    DirectAnswer { content: String },
    NeedTools { keywords: Vec<String> },
}
```

---

### 阶段 2：SEA 工具筛选

**工具内禀属性定义:**

```rust
// src/tools/registry.rs

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub script_path: PathBuf,
    pub description: String,
    pub tags: Vec<String>,           // ["天气", "HTTP", "城市"]
    pub category: ToolCategory,      // Weather, FileSystem, System...
    pub required_params: Vec<ParamInfo>,
    pub optional_params: Vec<ParamInfo>,
    pub capabilities: Vec<String>,   // ["http_request", "json_parse"]
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolCategory {
    Weather,
    Search,
    Calculator,
    FileSystem,
    System,
    AI,
    Custom,
}

#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: String,
    pub param_type: String,  // "string", "number", "boolean"
    pub required: bool,
    pub description: String,
}
```

**筛选逻辑:**

```rust
impl ToolRegistry {
    /// 根据关键词筛选候选工具
    pub fn filter_tools(&self, keywords: &[String]) -> Vec<ToolInfo> {
        self.tools.iter()
            .filter(|tool| {
                // 匹配标签
                let tag_match = keywords.iter()
                    .any(|kw| tool.tags.iter().any(|t| t.contains(kw)));
                
                // 匹配描述
                let desc_match = keywords.iter()
                    .any(|kw| tool.description.contains(kw));
                
                // 匹配能力
                let cap_match = keywords.iter()
                    .any(|kw| tool.capabilities.iter().any(|c| c.contains(kw)));
                
                tag_match || desc_match || cap_match
            })
            .cloned()
            .collect()
    }
}
```

---

### 阶段 3a：工具模型确认

**System Prompt 设计:**

```rust
const SYSTEM_PROMPT_TOOL_MODEL: &str = r#"你是工具选择专家。你的任务是从候选工具中选择最合适的工具。

## 输入
- 用户需求描述
- 候选工具列表 (包含名称、描述、参数)

## 输出格式 (JSON)
{
    "selected_tool": "工具名称",
    "confidence": 0.0-1.0,
    "arguments": {
        "param1": "value1",
        "param2": "value2"
    },
    "reason": "选择理由"
}

如果没有合适的工具，输出：
{
    "selected_tool": null,
    "new_tool_keywords": ["关键词 1", "关键词 2"]
}

## 候选工具
{{tools_json}}
"#;
```

**工具模型调用:**

```rust
async fn select_tool_with_model(
    client: &APIClient,
    tool_model: &str,
    user_need: &str,
    candidate_tools: &[ToolInfo],
) -> Result<ToolSelection> {
    let tools_json = serde_json::to_string_pretty(&candidate_tools)?;
    
    let prompt = SYSTEM_PROMPT_TOOL_MODEL
        .replace("{{tools_json}}", &tools_json);
    
    let messages = vec![
        Message::new("system", &prompt),
        Message::new("user", &format!("用户需求：{}", user_need)),
    ];
    
    let response = client.chat_completion(&messages, None, false).await?;
    
    // 解析 JSON 响应
    let selection: ToolSelection = serde_json::from_str(&response.content.unwrap())?;
    Ok(selection)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolSelection {
    pub selected_tool: Option<String>,
    pub confidence: Option<f64>,
    pub arguments: Option<serde_json::Value>,
    pub reason: Option<String>,
    pub new_tool_keywords: Option<Vec<String>>,
}
```

---

### 阶段 3b：新建工具请求

**工具模型输出示例:**

```json
{
    "selected_tool": null,
    "new_tool_keywords": ["天气 API", "HTTP 请求", "JSON 解析", "城市查询"],
    "required_capabilities": [
        "http_get",
        "json_parse",
        "string_format"
    ],
    "suggested_params": [
        {"name": "city", "type": "string", "required": true},
        {"name": "api_key", "type": "string", "required": false}
    ]
}
```

**SEA 处理逻辑:**

```rust
if selection.selected_tool.is_none() {
    log::info!("No suitable tool found, initiating new tool creation");
    
    // 输出标签，进入新建工具流程
    let keywords = selection.new_tool_keywords.join(",");
    println!("[NewTool]<{}>", keywords);
    
    // TODO: 进入新建工具流程
    // 1. 工具模型生成 Python 代码
    // 2. 用户确认/修改
    // 3. 保存到 ~/.sea/tools/
    // 4. 更新工具索引
}
```

---

### 阶段 4a：执行工具

**Python 工具执行器:**

```rust
// src/tools/python_executor.rs

use std::process::{Command, Stdio};
use serde_json::Value;

pub struct PythonExecutor {
    python_path: PathBuf,
    timeout_secs: u64,
}

impl PythonExecutor {
    pub fn new() -> Self {
        Self {
            python_path: PathBuf::from("python"),
            timeout_secs: 30,
        }
    }
    
    pub fn execute(&self, script_path: &Path, args: &Value) -> Result<String> {
        let args_str = serde_json::to_string(args)?;
        
        let output = Command::new(&self.python_path)
            .arg(script_path)
            .arg("--args")
            .arg(&args_str)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("Tool execution failed: {}", stderr))
        }
    }
}
```

**Python 工具模板:**

```python
# ~/.sea/tools/weather.py
"""
@tool(name="get_weather", description="Get current weather for a city")
@tag("天气")
@tag("HTTP")
@tag("城市")
@category("Weather")
@param city (string, required): The city name
@returns string: Weather description
"""

import sys
import json
import requests

def execute(city: str) -> str:
    """获取城市天气"""
    api_key = "your-api-key"  # 从环境变量或配置读取
    response = requests.get(
        f"http://api.openweathermap.org/data/2.5/weather",
        params={"q": city, "appid": api_key}
    )
    response.raise_for_status()
    data = response.json()
    
    temp = data['main']['temp'] - 273.15  # 转摄氏度
    desc = data['weather'][0]['description']
    return f"{city} 当前温度：{temp:.1f}°C, {desc}"

if __name__ == "__main__":
    # 解析命令行参数
    args = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
    result = execute(**args)
    print(result)
```

---

### 阶段 5a：结果回传主模型

**System Prompt 设计:**

```rust
const SYSTEM_PROMPT_FINAL: &str = r#"你是一个友好的智能助手。请根据工具执行的结果，给用户一个清晰、完整的回答。

## 规则
- 如果工具执行成功，总结结果并用自然语言呈现
- 如果工具执行失败，礼貌地解释原因并提供替代建议
- 不要提及"工具"、"API"等技术细节，用用户能理解的语言

## 示例

工具结果："北京 当前温度：25.0°C, 晴"
你："北京当前天气晴朗，温度 25°C，非常适合外出活动。"

工具结果："Error: 城市不存在"
你："抱歉，未能找到该城市的天气信息。请检查城市名称是否正确。"
"#;
```

**调用逻辑:**

```rust
let final_messages = vec![
    Message::new("system", SYSTEM_PROMPT_FINAL),
    Message::new("user", &format!(
        "工具执行结果：{}\n请根据这个结果回答用户的问题：{}",
        tool_result, user_input
    )),
];

let final_response = client
    .chat_completion(&final_messages, None, false)
    .await?;

// 显示并记录最终响应
println!("{}", final_response.content.unwrap());
```

---

## 状态机设计

```rust
// src/chat/state_machine.rs

#[derive(Debug, Clone, PartialEq)]
pub enum ChatState {
    WaitingForUser,
    IntentDetection,
    ToolFiltering,
    ToolSelection,
    ToolExecution,
    FinalResponse,
    NewToolCreation,
}

pub struct ChatContext {
    pub messages: Vec<Message>,
    pub current_intent: Option<Intent>,
    pub candidate_tools: Vec<ToolInfo>,
    pub tool_result: Option<String>,
}

pub struct ChatStateMachine {
    state: ChatState,
    context: ChatContext,
}

impl ChatStateMachine {
    pub fn new() -> Self {
        Self {
            state: ChatState::WaitingForUser,
            context: ChatContext {
                messages: Vec::new(),
                current_intent: None,
                candidate_tools: Vec::new(),
                tool_result: None,
            },
        }
    }
    
    pub fn process_user_input(&mut self, input: &str) -> Result<String> {
        match self.state {
            ChatState::WaitingForUser => {
                self.context.add_user_message(input);
                self.state = ChatState::IntentDetection;
                self.detect_intent()
            }
            ChatState::IntentDetection => {
                // 处理主模型响应
                let intent = parse_intent(input);
                match intent {
                    Intent::DirectAnswer { content } => {
                        self.state = ChatState::WaitingForUser;
                        Ok(content)
                    }
                    Intent::NeedTools { keywords } => {
                        self.context.current_intent = Some(Intent::NeedTools { keywords: keywords.clone() });
                        self.state = ChatState::ToolFiltering;
                        self.filter_tools(&keywords)
                    }
                }
            }
            ChatState::ToolFiltering => {
                // 处理工具筛选结果
                if self.context.candidate_tools.is_empty() {
                    self.state = ChatState::NewToolCreation;
                    self.request_new_tool()
                } else {
                    self.state = ChatState::ToolSelection;
                    self.select_tool_with_model()
                }
            }
            ChatState::ToolSelection => {
                // 处理工具模型选择
                let selection = parse_tool_selection(input);
                if selection.selected_tool.is_some() {
                    self.state = ChatState::ToolExecution;
                    self.execute_tool(&selection)
                } else {
                    self.state = ChatState::NewToolCreation;
                    self.request_new_tool()
                }
            }
            ChatState::ToolExecution => {
                // 处理工具执行结果
                self.state = ChatState::FinalResponse;
                self.generate_final_response()
            }
            ChatState::FinalResponse => {
                // 完成一轮对话
                self.state = ChatState::WaitingForUser;
                Ok(input.to_string())
            }
            ChatState::NewToolCreation => {
                // TODO: 新建工具流程
                self.state = ChatState::WaitingForUser;
                Ok("新建工具功能开发中...".to_string())
            }
            _ => Err(anyhow::anyhow!("Invalid state")),
        }
    }
    
    fn detect_intent(&self) -> Result<String> {
        // 调用主模型进行意图识别
        // ...
    }
    
    fn filter_tools(&mut self, keywords: &[String]) -> Result<String> {
        // 筛选候选工具
        // ...
    }
    
    fn select_tool_with_model(&self) -> Result<String> {
        // 调用工具模型选择工具
        // ...
    }
    
    fn execute_tool(&self, selection: &ToolSelection) -> Result<String> {
        // 执行工具
        // ...
    }
    
    fn generate_final_response(&self) -> Result<String> {
        // 生成最终回复
        // ...
    }
}
```

---

## 配置文件

```yaml
# ~/.sea/config.yaml

# API 配置
api:
  base_url: "https://api.example.com"
  api_key: "your-api-key"

# 模型配置
models:
  main:
    name: "qwen-max"
    temperature: 0.7
    max_tokens: 2048
  
  tool:
    name: "qwen-turbo"
    temperature: 0.3
    max_tokens: 1024

# 工具配置
tools:
  directory: "~/.sea/tools"
  auto_discover: true
  timeout_secs: 30
  python_path: "python"
  
# 日志配置
logging:
  level: "info"
  file: "~/.sea/sea.log"
  console: true
```

---

## 目录结构

```
~/.sea/
├── config.yaml              # 配置文件
├── sea.log                  # 日志文件
├── sessions/                # 会话记录
│   └── sess_*.json
└── tools/                   # 用户工具目录
    ├── weather.py           # 天气查询工具
    ├── calculator.py        # 计算器工具
    ├── search.py            # 搜索工具
    └── llm_search.py        # AI 搜索工具
```

---

## 工具元数据解析

**Python 工具注释规范:**

```python
"""
@tool(name="tool_name", description="Tool description")
@tag("tag1")
@tag("tag2")
@category("CategoryName")
@param param_name (type, required|optional): Description
@returns return_type: Description
"""
```

**Rust 解析器:**

```rust
// src/tools/metadata_parser.rs

use regex::Regex;

pub struct MetadataParser;

impl MetadataParser {
    pub fn parse_python_tool(script_path: &Path) -> Result<ToolInfo> {
        let content = std::fs::read_to_string(script_path)?;
        
        // 提取文档字符串
        let docstring = Self::extract_docstring(&content)?;
        
        // 解析元数据
        let name = Self::extract_annotation(&docstring, "name")?;
        let description = Self::extract_annotation(&docstring, "description")?;
        let tags = Self::extract_all_annotations(&docstring, "tag");
        let category = Self::extract_annotation(&docstring, "category")
            .unwrap_or_else(|_| "Custom".to_string());
        let params = Self::parse_params(&docstring)?;
        
        Ok(ToolInfo {
            name,
            script_path: script_path.to_path_buf(),
            description,
            tags,
            category: ToolCategory::from_str(&category),
            required_params: params.iter().filter(|p| p.required).cloned().collect(),
            optional_params: params.iter().filter(|p| !p.required).cloned().collect(),
            capabilities: Self::infer_capabilities(&content),
        })
    }
    
    fn extract_docstring(content: &str) -> Result<String> {
        // 提取 Python 文档字符串
        let re = Regex::new(r#"^"""([\s\S]*?)""""#)?;
        re.captures(content)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| anyhow::anyhow!("No docstring found"))
    }
    
    fn extract_annotation(docstring: &str, key: &str) -> Result<String> {
        let pattern = format!(r#"@{}\("([^"]+)"\)"#, key);
        let re = Regex::new(&pattern)?;
        re.captures(docstring)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| anyhow::anyhow!("Annotation @{} not found", key))
    }
    
    fn extract_all_annotations(docstring: &str, key: &str) -> Vec<String> {
        let pattern = format!(r#"@{}\("([^"]+)"\)"#, key);
        let re = Regex::new(&pattern).unwrap();
        re.captures_iter(docstring)
            .filter_map(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }
    
    fn parse_params(docstring: &str) -> Result<Vec<ParamInfo>> {
        let re = Regex::new(r#"@param (\w+) \((\w+), (required|optional)\): (.+)"#)?;
        let params = re.captures_iter(docstring)
            .map(|c| ParamInfo {
                name: c[1].to_string(),
                param_type: c[2].to_string(),
                required: c[3] == "required",
                description: c[4].to_string(),
            })
            .collect();
        Ok(params)
    }
    
    fn infer_capabilities(content: &str) -> Vec<String> {
        let mut caps = Vec::new();
        
        if content.contains("requests.") || content.contains("http") {
            caps.push("http_request".to_string());
        }
        if content.contains("json.") {
            caps.push("json_parse".to_string());
        }
        if content.contains("open(") || content.contains("read(") {
            caps.push("file_io".to_string());
        }
        if content.contains("subprocess.") || content.contains("os.system") {
            caps.push("shell_exec".to_string());
        }
        
        caps
    }
}
```

---

## 实现路线图

### 阶段 1：核心流程 (MVP)
- [ ] 实现主模型意图识别
- [ ] 实现 `[NeedTools]` 标签解析
- [ ] 实现工具筛选逻辑
- [ ] 实现 Python 工具执行器

### 阶段 2：工具模型集成
- [ ] 实现工具模型 System Prompt
- [ ] 实现工具选择 JSON 解析
- [ ] 实现工具模型调用流程

### 阶段 3：工具注册与发现
- [ ] 实现工具目录扫描
- [ ] 实现元数据解析器
- [ ] 实现工具索引缓存

### 阶段 4：新建工具流程
- [ ] 实现 `[NewTool]` 标签处理
- [ ] 实现工具模型代码生成
- [ ] 实现用户确认交互
- [ ] 实现工具保存与注册

### 阶段 5：优化与扩展
- [ ] 添加工具沙箱执行
- [ ] 添加工具调用日志
- [ ] 实现工具调用统计
- [ ] 支持工具链 (多工具组合)

---

## 安全考虑

1. **Python 脚本权限控制**
   - 限制网络访问 (可选)
   - 限制文件系统访问范围
   - 限制系统命令执行

2. **用户确认机制**
   - 首次使用新工具需用户确认
   - 敏感操作 (文件写入、命令执行) 需用户确认

3. **超时与资源限制**
   - 工具执行超时 (默认 30 秒)
   - 输出大小限制
   - 内存使用限制

4. **审计日志**
   - 记录所有工具调用
   - 记录工具执行结果
   - 记录用户确认操作

---

## 参考资源

- [Model Context Protocol Specification](https://modelcontextprotocol.io/specification/)
- [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat)
- [Python subprocess 文档](https://docs.python.org/3/library/subprocess.html)
