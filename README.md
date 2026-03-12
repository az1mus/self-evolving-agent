# Self-Evolving Agent

一个能够在运行中实现工具进化与提示词进化的智能代理系统。

## 功能特性

- **LLM API 通信**: 支持与多种LLM API进行通信
- **CLI 界面**: 交互式命令行界面
- **配置管理**: 支持在CLI中配置API入口、API Key等
- **Session 管理**: 完整的会话管理系统
- **日志系统**: 支持info、debug、warning、error等级，debug包含完整API包记录

## 项目结构

```
self-evolving-agent/
├── src/                # 源代码
│   ├── main.rs         # 程序入口
│   ├── lib.rs          # 库入口
│   ├── config/         # 配置管理
│   ├── llm/            # LLM API通信
│   ├── session/        # 会话管理
│   ├── logger/         # 日志系统
│   └── cli/            # CLI界面
├── bin/                # 编译输出目录
├── Cargo.toml
├── README.md
└── LICENSE
```

## 快速开始

```bash
# 构建
cargo build --release

# 运行
cargo run --release
```

可执行文件位于 `bin/release/self-evolving-agent.exe`

## 许可证

MIT License
