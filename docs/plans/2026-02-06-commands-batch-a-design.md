# 第二阶段：Commands 模块迁移设计 - 批次 A

## 概述

将 TypeScript just-bash 项目的 `commands/` 模块迁移到 Rust，优先实现批次 A 的 14 个基础命令。

## 设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 目录结构 | 与 TS 保持一致，独立 `src/commands/` | 用户要求 |
| 参数解析 | clap 库 | Rust 生态标准，功能强大 |
| 共享工具 | Rust 生态库 + 自实现 | 符合 Rust 惯例 |
| 测试策略 | 同步迁移测试 | 确保行为一致 |
| 异步支持 | async_trait | 与现有 FileSystem trait 一致 |

## 目录结构

```
src/commands/
├── mod.rs              # 模块导出
├── types.rs            # Command trait 和共享类型
├── registry.rs         # 命令注册表
├── utils/
│   ├── mod.rs
│   ├── help.rs         # 帮助系统
│   └── file_reader.rs  # 文件读取工具
├── cat/
│   └── mod.rs
├── ls/
│   └── mod.rs
├── grep/
│   └── mod.rs
├── head/
│   └── mod.rs
├── tail/
│   └── mod.rs
├── wc/
│   └── mod.rs
├── mkdir/
│   └── mod.rs
├── rm/
│   └── mod.rs
├── cp/
│   └── mod.rs
├── mv/
│   └── mod.rs
├── touch/
│   └── mod.rs
├── basename/
│   └── mod.rs
├── dirname/
│   └── mod.rs
└── test/
    └── mod.rs
```

## 核心类型

```rust
// src/commands/types.rs
use async_trait::async_trait;
use crate::fs::FileSystem;
use std::collections::HashMap;

pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub struct CommandContext<'a> {
    pub args: Vec<String>,
    pub stdin: String,
    pub cwd: &'a str,
    pub env: &'a HashMap<String, String>,
    pub fs: &'a dyn FileSystem,
}

#[async_trait]
pub trait Command: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, ctx: CommandContext<'_>) -> CommandResult;
}
```

## 命令注册表

```rust
// src/commands/registry.rs
pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self;
    pub fn register(&mut self, cmd: Box<dyn Command>);
    pub fn get(&self, name: &str) -> Option<&dyn Command>;
    pub fn names(&self) -> Vec<&str>;
}

pub fn register_batch_a(registry: &mut CommandRegistry);
```

## 实现顺序

1. **基础设施**: types.rs, registry.rs, utils/
2. **简单命令**: basename, dirname, touch, mkdir
3. **文件读取**: cat, head, tail, wc
4. **文件操作**: rm, cp, mv
5. **复杂命令**: ls, grep, test

## 依赖

- `clap` - 参数解析
- `async_trait` - 已有依赖
- `regex` - grep 命令需要

## 参考

- TS 源码: `/Users/arthur/PycharmProjects/just-bash/src/commands/`
- Rust 项目: `/Users/arthur/conductor/workspaces/just-bash-v1/san-jose/`
