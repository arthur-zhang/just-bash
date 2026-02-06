// src/commands/types.rs
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use crate::fs::FileSystem;

/// 命令执行结果
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandResult {
    pub fn success(stdout: String) -> Self {
        Self { stdout, stderr: String::new(), exit_code: 0 }
    }

    pub fn error(stderr: String) -> Self {
        Self { stdout: String::new(), stderr, exit_code: 1 }
    }

    pub fn with_exit_code(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self { stdout, stderr, exit_code }
    }
}

/// 命令执行上下文
pub struct CommandContext {
    pub args: Vec<String>,
    pub stdin: String,
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub fs: Arc<dyn FileSystem>,
}

/// 命令 trait
#[async_trait]
pub trait Command: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, ctx: CommandContext) -> CommandResult;
}
