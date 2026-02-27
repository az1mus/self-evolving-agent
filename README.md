# Self-Evolved Agent (SEA)

Self-Evolved Agent (SEA) 是一个命令行工具，用于通过 OpenAI 兼容的 API 与大型语言模型 (LLM) 进行交互。SEA 采用 Rust 语言实现，提供高性能、类型安全和灵活的配置能力。

❗本项目100%由AI生成，未经过测试，请勿用于生产环境。

## 特性

- **多模型支持**：兼容任何 OpenAI 兼容的 API 端点
- **意图识别**：自动判断用户请求是否需要调用工具
- **工具路由**：基于关键词智能筛选和选择合适工具
- **内置工具**：时间查询、文件读写、目录列出、命令执行、**Python 脚本执行**
- **Python 工具扩展**：支持外挂 Python 脚本，无需重新编译
- **自动代码生成**：无匹配脚本时，LLM 自动生成 Python 脚本并保存
- **会话管理**：维护和恢复对话历史
- **交互式 CLI**：支持聊天模式和单次查询模式
- **丰富输出**：使用 Colored 库提供美观的终端输出

## 安装

### 系统要求

- **操作系统**: Windows (x86_64) / Linux / macOS
- **Rust 工具链**: 1.70+ (推荐最新稳定版，如 rustc 1.93.1)
- **编译器**: MSVC (Windows) 或 GCC/Clang (Linux/macOS)

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/az1mus/sea.git
cd sea

# Debug 模式构建（开发）
cargo build

# Release 模式构建（发布，优化）
cargo build --release
```

构建产物位于 `target/debug/sea` 或 `target/release/sea`。

### 常用命令

```bash
# 运行程序
cargo run --chat
cargo run --query "你好"

# 运行测试
cargo test

# 清理构建缓存
cargo clean
```

### 环境变量（可选）

```bash
# 设置默认构建模式为 release
set CARGO_PROFILE=release          # Windows
export CARGO_PROFILE=release       # Linux/macOS

# 设置 RUSTFLAGS 优化
set RUSTFLAGS=-C target-cpu=native # Windows
export RUSTFLAGS=-C target-cpu=native # Linux/macOS
```

## 快速开始

### 1. 配置 API 设置

```bash
# 设置 API 端点
sea config set base_url https://api.openai.com/v1

# 设置 API 密钥
sea config set api_key sk-your-api-key

# 设置默认模型
sea config set model gpt-4-turbo
```

### 2. 开始聊天

```bash
sea chat
```

### 3. 或运行单次查询

```bash
sea query "法国的首都是哪里？"
```

## 配置管理

SEA 将配置存储在 `~/.sea/config.yaml`。您可以通过 CLI 命令或直接编辑文件进行管理。

### 配置命令

- `sea config set <key> <value>` - 设置配置项
- `sea config get <key>` - 获取配置项
- `sea config list` - 列出所有配置
- `sea config reset` - 重置为默认值

### 配置项

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `base_url` | String | `https://api.openai.com/v1` | API 端点 URL |
| `api_key` | String | - | API 密钥 |
| `model` | String | `gpt-4-turbo` | 默认模型名称 |
| `temperature` | Float | `0.7` | 采样温度 |
| `max_tokens` | Int | `2048` | 最大生成 token 数 |
| `timeout` | Int | `30` | 请求超时 (秒) |
| `history_size` | Int | `100` | 会话历史大小 |

### 配置文件示例

```yaml
base_url: "https://api.example.com/v1"
api_key: "your-api-key"
model: "qwen-max"
temperature: 0.7
max_tokens: 2048
timeout: 30
history_size: 100
```

## 可用命令

### 主要命令

- `sea chat` - 启动交互式聊天会话
- `sea query <message>` - 发送单次查询
- `sea config` - 管理配置
- `sea models` - 列出可用模型
- `sea tools` - 列出可用工具
- `sea session` - 管理会话

### 会话管理

```bash
# 列出所有会话
sea session list

# 删除指定会话
sea session delete <session_id>
```

## 工具系统

### 内置工具

SEA 提供以下内置工具：

| 工具名称 | 功能 | 参数 |
|----------|------|------|
| `get_current_datetime` | 获取当前日期时间 | 无 |
| `read_file` | 读取文件内容 | `path`: 文件路径 |
| `write_file` | 写入文件内容 | `path`: 文件路径，`content`: 内容 |
| `list_files` | 列出目录文件 | `path`: 目录路径 (可选)，`recursive`: 是否递归 (可选) |
| `execute_shell` | 执行 Shell 命令 | `command`: 命令字符串 |
| `execute_python` | 执行 Python 脚本 | `script_path`: 脚本路径，`args`: JSON 参数 |

### Python 脚本扩展

SEA 支持通过 Python 脚本扩展功能，无需重新编译。

#### 脚本目录

Python 脚本存储在 `~/.sea/scripts/` 目录。

#### 脚本格式

```python
#!/usr/bin/env python3
"""
脚本功能描述
"""

import sys
import json

def main():
    if len(sys.argv) < 2:
        print("Error: No arguments provided")
        sys.exit(1)
    
    args = json.loads(sys.argv[1])
    # 处理参数并输出结果
    print("执行结果")

if __name__ == "__main__":
    main()
```

#### 使用示例

**用户**: "用 weather.py 查询北京天气"

**SEA**: 
1. 扫描 `~/.sea/scripts/` 目录
2. 匹配到 `weather.py`
3. 执行 `execute_python` 工具
4. 返回结果

### 工具调用流程

SEA 采用**多阶段**工具调用架构：

```
用户输入 → 意图识别 → 是否需要工具？
                    │
        ┌───────────┴───────────┐
        │                       │
        ▼                       ▼
    需要工具                  不需要工具
        │                       │
        ▼                       │
Step 2: 内置工具选择             │
        │                       │
    ┌───┴───┐                   │
    │       │                   │
    ▼       ▼                   │
有匹配    无匹配                 │
    │       │                   │
    ▼       ▼                   │
执行工具  Step 3: 扫描 Python 脚本库
                │
            ┌───┴───┐
            │       │
            ▼       ▼
        有匹配    无匹配
            │       │
            ▼       ▼
        执行脚本  Step 4: LLM 生成 Python 代码
                    │
                    ▼
                Step 5: 执行生成的脚本
                    │
                    ▼
                Step 6: 生成最终回复
```

### 意图识别

系统使用特定的 System Prompt 让主模型判断是否需要工具：

```rust
const SYSTEM_PROMPT_INTENT: &str = r#"你是一个智能助手。请分析用户请求，判断是否需要使用工具来完成。

如果你认为需要使用工具，请在回复的第一行输出：[TOOL_NEEDED] 并简要说明需要什么工具。
如果你不需要工具就能回答，请直接回答。
"#;
```

### 工具路由

工具路由器 (`ToolRouter`) 基于关键词匹配筛选工具：

```rust
// 示例：用户输入 "列出当前目录的文件"
// 路由器匹配到关键词 "列出", "目录", "文件"
// 自动筛选出 list_files 工具
```

## 架构设计

### 核心模块

```
src/
├── main.rs          # 程序入口
├── api.rs           # API 客户端 (OpenAI 兼容)
├── cli.rs           # 命令行接口
├── config.rs        # 配置管理
├── session.rs       # 会话管理
├── tools/           # 工具系统
│   ├── mod.rs       # 工具基类
│   ├── router.rs    # 工具路由器
│   └── builtins.rs  # 内置工具
└── utils.rs         # 工具函数
```

### 关键组件

#### 1. API 客户端 (`api::APIClient`)

- 处理与 LLM API 的通信
- 支持工具调用格式
- 解析响应和工具调用

#### 2. 工具管理器 (`tools::ToolManager`)

- 注册和管理工具
- 执行工具调用
- 参数验证

#### 3. 工具路由器 (`tools::router::ToolRouter`)

- 基于关键词筛选工具
- 智能匹配用户意图

#### 4. 会话管理器 (`session::SessionManager`)

- 创建和恢复会话
- 管理对话历史
- 持久化存储

## 开发指南

### 添加新工具

有两种方式扩展工具功能：

#### 方式 1：Python 脚本（推荐，无需编译）

在 `~/.sea/scripts/` 目录下创建 Python 脚本：

```python
#!/usr/bin/env python3
"""
计算两个数的乘积
"""

import sys
import json

def main():
    if len(sys.argv) < 2:
        print("Error: No arguments provided")
        sys.exit(1)
    
    args = json.loads(sys.argv[1])
    a = args.get("a", 0)
    b = args.get("b", 0)
    result = a * b
    print(f"{a} × {b} = {result}")

if __name__ == "__main__":
    main()
```

SEA 会自动扫描并匹配脚本名称。用户可以说"用 multiply.py 计算 5 和 3 的乘积"。

#### 方式 2：Rust 内置工具（需重新编译）

1. 在 `src/tools/builtins.rs` 中实现 `Tool` trait：

```rust
use anyhow::Result;
use serde_json::{json, Value};
use crate::tools::Tool;

pub struct MyCustomTool;

impl Tool for MyCustomTool {
    fn name(&self) -> &str {
        "my_custom_tool"
    }

    fn description(&self) -> &str {
        "这是一个自定义工具"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "param1": {
                    "type": "string",
                    "description": "参数 1 说明"
                }
            },
            "required": ["param1"]
        })
    }

    fn execute(&self, args: &Value) -> Result<String> {
        let param1 = args["param1"].as_str().unwrap();
        Ok(format!("执行结果：{}", param1))
    }
}
```

2. 在 `register_builtin_tools` 函数中注册：

```rust
pub fn register_builtin_tools(tool_manager: &mut crate::tools::ToolManager) {
    let builtin_tools: Vec<Box<dyn Tool>> = vec![
        // ... 现有工具
        Box::new(MyCustomTool),
    ];

    for tool in builtin_tools {
        tool_manager.register_tool(tool);
    }
}
```

3. 在 `src/tools/router.rs` 中添加关键词：

```rust
ToolMeta {
    name: "my_custom_tool",
    keywords: &["关键词 1", "关键词 2"],
},
```

### 构建和测试

```bash
# Debug 模式（开发）
cargo build          # 编译
cargo run            # 编译并运行

# Release 模式（发布，优化）
cargo build --release
cargo run --release

# 指定目标平台
cargo build --target x86_64-pc-windows-msvc

# 运行测试
cargo test

# 清理构建缓存
cargo clean
```

## 依赖项

主要 Rust 依赖：

| 依赖 | 版本 | 用途 |
|------|------|------|
| `clap` | 4.4 | CLI 框架 |
| `tokio` | 1.35 | 异步运行时 |
| `reqwest` | 0.11 | HTTP 客户端 |
| `serde` / `serde_json` | 1.0 | 序列化 |
| `serde_yaml` | 0.9 | YAML 配置 |
| `chrono` | 0.4 | 日期时间 |
| `uuid` | 1.6 | 会话 ID 生成 |
| `dirs` | 5.0 | 主目录路径 |
| `anyhow` / `thiserror` | 1.0 | 错误处理 |
| `colored` | 2.1 | 彩色输出 |
| `dialoguer` | 0.11 | 交互提示 |
| `log` / `env_logger` | 0.4 / 0.10 | 日志记录 |

### Cargo.toml 配置示例

```toml
[package]
name = "sea"
version = "0.1.0"
edition = "2021"

[dependencies]
# CLI 框架
clap = { version = "4.4", features = ["derive"] }
# 异步运行时
tokio = { version = "1.35", features = ["full"] }
# HTTP 客户端
reqwest = { version = "0.11", features = ["json"] }
# 序列化
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
# 日志
log = "0.4"
env_logger = "0.10"
# 日期时间
chrono = { version = "0.4", features = ["serde"] }
# UUID
uuid = { version = "1.6", features = ["v4"] }
# 主目录
dirs = "5.0"
# 错误处理
anyhow = "1.0"
thiserror = "1.0"
# 彩色输出
colored = "2.1"
# 交互提示
dialoguer = "0.11"

[profile.release]
opt-level = 3      # 优化级别
lto = true         # 链接时优化
```

## 与 Python 版本的差异

| 特性 | Python 版本 | Rust 版本 |
|------|-------------|-----------|
| 性能 | 解释执行 | 编译优化，高性能 |
| 类型安全 | 动态类型 | 静态类型，编译时检查 |
| 并发 | GIL 限制 | 原生异步支持 |
| 部署 | 需要 Python 环境 | 单一二进制文件 |
| 工具扩展 | Python 脚本 | **支持 Python 脚本 + 内置工具** |

## 安全考虑

1. **命令执行限制**：`execute_shell` 工具会检查危险命令模式
2. **文件大小限制**：文件读取限制为 1MB
3. **API 密钥保护**：配置显示时自动脱敏
4. **超时控制**：默认 30 秒请求超时

## 日志

SEA 使用 `env_logger` 和 `log` crate 进行日志记录：

```bash
# 设置日志级别
RUST_LOG=debug sea chat
RUST_LOG=info sea chat
RUST_LOG=error sea query "test"
```

## 贡献

欢迎贡献！请遵循以下步骤：

1. Fork 仓库
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 提交 Pull Request

## 许可证

本项目采用 MIT 许可证 - 详见 [LICENSE](LICENSE) 文件。

## References

- [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat)
- [Rust Programming Language](https://www.rust-lang.org/)
- [Tokio Async Runtime](https://tokio.rs/)
- [Clap CLI Framework](https://docs.rs/clap/latest/clap/)
