# Rust 编译环境配置

## 系统信息
- **操作系统**: Windows (x86_64)
- **目标平台**: x86_64-pc-windows-msvc

## Rust 工具链
- **版本**: rustc 1.93.1 (01f6ddf75 2026-02-11)
- **Cargo**: 1.93.1
- **默认工具链**: stable-x86_64-pc-windows-msvc
- **LLVM**: 21.1.8
- **安装路径**: `C:\Users\wuyue\.rustup\toolchains\stable-x86_64-pc-windows-msvc`

## MSVC 编译器
- **版本**: Microsoft C/C++ 优化编译器 19.50.35724 (Visual Studio 2022)
- **架构**: x64

## 已安装的工具链
1. `stable-x86_64-pc-windows-msvc` (active, default)
2. `stable-x86_64-pc-windows-gnu`

## 常用编译命令

### Debug 模式（开发）
```bash
cargo build          # 编译
cargo run            # 编译并运行
```

### Release 模式（发布，优化）
```bash
cargo build --release
cargo run --release
```

### 指定目标平台
```bash
cargo build --target x86_64-pc-windows-msvc
```

### 清理构建缓存
```bash
cargo clean
```

## Cargo.toml 配置示例
```toml
[package]
name = "your_project"
version = "0.1.0"
edition = "2021"

[dependencies]

[profile.release]
opt-level = 3      # 优化级别
lto = true         # 链接时优化
```

## 环境变量（可选）
```bash
# 设置默认构建模式为 release
set CARGO_PROFILE=release

# 设置 RUSTFLAGS
set RUSTFLAGS=-C target-cpu=native
```
