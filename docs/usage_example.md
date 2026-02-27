# 使用示例：Self-Evolved Agent (SEA)

## 安装

```bash
# 克隆或下载项目后
cd self-evolved-agent
python -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate
pip install -e .
```

## 快速开始

### 1. 配置API设置

```bash
# 设置API基础URL
sea config set base_url https://api.openai.com/v1

# 设置API密钥
sea config set api_key your-api-key-here

# 设置默认模型
sea config set model gpt-4-turbo
```

### 2. 发送单个查询

```bash
sea query "Hello, how are you?"
```

### 3. 开始交互式聊天

```bash
sea chat
```

在聊天模式下，您可以输入消息与模型对话，输入 'exit' 或 'quit' 结束会话。

### 4. 查看可用模型

```bash
sea models
```

### 5. 查看可用工具

```bash
sea tools
```

### 6. 管理会话

```bash
# 列出会话
sea session list

# 删除会话
sea session delete session-id-here
```

## 配置选项

- `base_url`: API的基础URL
- `api_key`: API密钥
- `model`: 默认使用的模型
- `temperature`: 温度参数（0.0-2.0）
- `max_tokens`: 最大生成token数
- `timeout`: 请求超时时间（秒）
- `history_size`: 历史消息保留数量

## 内置工具

SEA包含以下内置工具：

- `get_current_datetime`: 获取当前日期和时间
- `read_file`: 读取文件内容
- `write_file`: 写入内容到文件
- `list_files`: 列出目录中的文件
- `execute_shell`: 执行shell命令

## 自定义工具

您可以通过在 `~/.sea/tools/` 目录中添加Python文件来定义自定义工具。每个工具应该继承自 `Tool` 类并实现 `execute` 方法。

## 故障排除

如果遇到问题，请检查：

1. API密钥是否正确设置
2. 网络连接是否正常
3. API提供商是否支持所选模型
4. 配置文件路径是否正确 (`~/.sea/config.yaml`)

## 贡献

欢迎提交PR和报告问题！