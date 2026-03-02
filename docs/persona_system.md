# SEA Persona 管理系统

## 概述

Persona 管理系统将原来硬编码的系统提示词转换为可配置、可扩展的 Persona 定义，支持：

- **JSON 配置**：使用 JSON 文件定义 Persona
- **动态加载**：运行时从磁盘加载用户 Persona
- **类型系统**：Core/Utility/Special 三种类型
- **路由规则**：定义 Persona 之间的调用关系
- **错误处理**：每个 Persona 可配置错误恢复策略

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│                  Persona Management System                   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│  │  Persona    │    │  Pipeline   │    │   Orchestr- │     │
│  │  Manager    │───►│  Context    │───►│   ator      │     │
│  └─────────────┘    └─────────────┘    └─────────────┘     │
│        │                                      │             │
│        ▼                                      ▼             │
│  ┌─────────────┐                        ┌─────────────┐     │
│  │  Persona    │                        │  Persona    │     │
│  │  Config     │                        │  Executor   │     │
│  └─────────────┘                        └─────────────┘     │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## 目录结构

```
~/.sea/personas/          # 用户 Persona 目录
├── intent_detector.json
├── intent_extractor.json
├── planner.json
└── error_handler.json

personas/                 # 项目默认 Persona 模板
├── intent_detector.json
├── intent_extractor.json
├── tool_selector.json
├── python_generator.json
├── final_responder.json
├── planner.json
└── error_handler.json
```

## Persona 配置格式

```json
{
  "name": "intent_detector",
  "display_name": "意图检测器",
  "description": "判断用户请求是否需要使用工具来完成",
  "type": "core",
  "system_prompt": "你是一个智能助手...",
  "temperature": 0.7,
  "max_tokens": 512,
  "model": "qwen-turbo",
  "input_schema": {
    "required_fields": ["user_input"],
    "optional_fields": ["conversation_history"]
  },
  "output_schema": {
    "format": "text",
    "parse_pattern": "^\\[TOOL_NEEDED\\]"
  },
  "on_error": {
    "retry": 3,
    "fallback": "error_handler"
  },
  "next_persona": {
    "if_match": "\\[TOOL_NEEDED\\]",
    "then": "tool_selector",
    "else": null
  },
  "tags": ["core", "intent"]
}
```

### 字段说明

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | ✓ | 唯一标识符 |
| `display_name` | string | ✓ | 显示名称 |
| `description` | string | ✓ | 功能描述 |
| `type` | string | ✓ | `core`/`utility`/`special` |
| `system_prompt` | string | ✓ | 系统提示词 |
| `temperature` | number | ✗ | 采样温度 (默认 0.7) |
| `max_tokens` | integer | ✗ | 最大 token 数 (默认 2048) |
| `model` | string | ✗ | 覆盖默认模型 |
| `input_schema` | object | ✗ | 输入参数定义 |
| `output_schema` | object | ✗ | 输出格式定义 |
| `on_error` | object | ✗ | 错误处理策略 |
| `next_persona` | object | ✗ | 下一个 Persona 路由规则 |
| `tags` | array | ✗ | 标签列表 |

## CLI 命令

### 列出所有 Persona

```bash
sea persona list
```

### 查看 Persona 详情

```bash
sea persona show intent_detector
```

### 从模板创建 Persona

```bash
sea persona create my_assistant --template intent_detector
```

可用模板：
- `intent_detector`
- `intent_extractor`
- `tool_selector`
- `python_generator`
- `final_responder`
- `planner`
- `error_handler`

### 重新加载 Persona

```bash
sea persona reload
```

### 查看统计信息

```bash
sea persona stats
```

## Persona 类型

### Core (核心)

始终可用的基础 Persona，对应原硬编码的系统提示词：

- `intent_detector` - 意图检测
- `intent_extractor` - 意图/参数提取
- `final_responder` - 最终回复生成

### Utility (工具)

按需使用的工具型 Persona：

- `tool_selector` - 工具选择
- `python_generator` - Python 脚本生成
- `formatter` - 格式化输出（待实现）

### Special (特殊)

在特定条件下激活的 Persona：

- `planner` - 任务规划（复杂任务时激活）
- `error_handler` - 错误处理（出错时激活）
- `debugger` - 调试模式（待实现）

## 执行流程

```
用户输入
   │
   ▼
┌─────────────────────────┐
│ intent_detector         │ 检测是否需要工具
└──────────┬──────────────┘
           │
     ┌─────┴─────┐
     │ 需要工具   │ 不需要工具
     ▼           ▼
┌─────────┐   直接回复
│ intent_ │
│ extractor│
└────┬────┘
     │
     ▼
┌─────────────────────────┐
│ planner (可选)          │ 复杂任务时激活
└──────────┬──────────────┘
           │
           ▼
┌─────────────────────────┐
│ tool_selector           │ 选择工具
└──────────┬──────────────┘
           │
           ▼
┌─────────────────────────┐
│ python_generator (可选) │ 需要新脚本时
└──────────┬──────────────┘
           │
           ▼
     执行工具
           │
           ▼
┌─────────────────────────┐
│ final_responder         │ 生成最终回复
└─────────────────────────┘
```

## 扩展 Persona

### 1. 创建 JSON 配置文件

在 `~/.sea/personas/` 目录下创建 JSON 文件：

```json
{
  "name": "translator",
  "display_name": "翻译助手",
  "description": "将文本从一种语言翻译成另一种语言",
  "type": "utility",
  "system_prompt": "你是一个专业的翻译助手...",
  "temperature": 0.3,
  "tags": ["translation", "language"]
}
```

### 2. 重新加载

```bash
sea persona reload
```

### 3. 验证

```bash
sea persona show translator
```

## API 使用

```rust
use crate::persona::{PersonaManager, Orchestrator};
use std::sync::Arc;
use tokio::sync::Mutex;

// 创建 Persona 管理器
let manager = PersonaManager::new()?;

// 创建编排器
let orchestrator = Orchestrator::new(Arc::new(Mutex::new(manager)));

// 执行 Persona
let output = orchestrator
    .execute_simple("intent_detector", user_input, &messages, &client)
    .await?;
```

## 最佳实践

1. **命名规范**：使用 `snake_case` 命名 Persona
2. **职责单一**：每个 Persona 只负责一个明确的任务
3. **路由清晰**：明确定义 `next_persona` 规则
4. **错误处理**：为每个 Persona 配置 `on_error` 策略
5. **标签分类**：使用 `tags` 便于搜索和过滤

## 故障排除

### Persona 未加载

检查 JSON 格式是否正确：

```bash
# 验证 JSON 格式
python -m json.tool ~/.sea/personas/my_persona.json
```

### 循环调用

系统会自动检测并阻止循环调用。检查 `next_persona` 规则。

### 性能问题

大量 Persona 文件会影响启动速度。建议：
- 保持 Persona 文件数量合理（<50）
- 使用 `reload` 命令而非重启程序

## 未来计划

- [ ] 可视化 Persona 编辑器
- [ ] Persona 市场/分享
- [ ] 运行时 Persona 热更新
- [ ] Persona 链/工作流定义
- [ ] 性能监控和日志
