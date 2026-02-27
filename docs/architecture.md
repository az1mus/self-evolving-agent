# Self-Evolved Agent (SEA) 项目结构

## 项目概述

Self-Evolved Agent (SEA) 是一个命令行工具，用于与大型语言模型(LLM)进行交互。它支持任何兼容OpenAI格式的API端点，具有灵活的配置管理、工具集成和会话管理功能。

## 目录结构

```
self-evolved-agent/
├── src/                    # 源代码目录
│   ├── __init__.py         # 包初始化文件
│   ├── cli.py              # 主命令行接口
│   ├── config.py           # 配置管理模块
│   ├── api.py              # API客户端模块
│   ├── session.py          # 会话管理模块
│   ├── utils.py            # 工具函数
│   └── tools/              # 工具模块
│       ├── __init__.py
│       ├── base.py         # 工具基类
│       └── builtins.py     # 内置工具
├── tests/                  # 测试目录
│   └── test_sea.py         # 测试套件
├── docs/                   # 文档目录
│   └── usage_example.md    # 使用示例
├── venv/                   # 虚拟环境目录
├── pyproject.toml          # 项目配置文件
├── README.md               # 项目说明
├── LICENSE                 # 许可证
└── sea.py                  # 启动脚本
```

## 模块说明

### src/cli.py
- 主命令行界面，使用Click框架
- 提供chat、query、config、models、tools、session等命令
- 处理用户输入和输出显示

### src/config.py
- 配置管理，使用Pydantic数据类
- 支持持久化配置到YAML文件
- 提供全局配置管理器实例

### src/api.py
- API客户端，使用httpx进行HTTP请求
- 支持聊天完成、流式响应等功能
- 处理API错误和认证

### src/session.py
- 会话管理，保存对话历史
- 支持创建、加载、保存、删除会话
- 限制历史大小以节省内存

### src/tools/base.py
- 工具系统基类定义
- 工具管理器，注册和执行工具
- 参数验证和错误处理

### src/tools/builtins.py
- 内置工具实现
- 包括文件操作、系统命令、日期时间等工具

### src/utils.py
- 通用工具函数
- 日志配置、路径处理、文本格式化等

## 设计特点

1. **模块化设计**：各组件职责分离，易于维护和扩展
2. **异步支持**：使用async/await提高性能
3. **类型安全**：使用类型注解增强代码可读性和安全性
4. **配置灵活**：支持多种配置选项和持久化
5. **工具集成**：支持自定义工具和内置工具
6. **错误处理**：完善的异常处理机制
7. **测试覆盖**：包含单元测试确保代码质量

## 扩展性

SEA设计为高度可扩展：
- 可以轻松添加新的命令行功能
- 可以通过继承Tool类创建新工具
- 可以扩展API客户端以支持更多功能
- 可以定制会话管理逻辑