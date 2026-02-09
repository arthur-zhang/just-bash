// src/commands/types.rs
use async_trait::async_trait;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use crate::fs::FileSystem;

/// Callback for executing shell commands (used by xargs, find -exec)
/// Parameters: command_string, stdin, cwd, env, fs
pub type ExecFn = Arc<dyn Fn(String, String, String, HashMap<String, String>, Arc<dyn FileSystem>)
    -> Pin<Box<dyn Future<Output = CommandResult> + Send>> + Send + Sync>;

/// HTTP response for fetch callback
#[derive(Debug, Clone)]
pub struct FetchResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub url: String,
}

/// Callback for HTTP requests (used by curl)
/// Parameters: url, method, headers, body
pub type FetchFn = Arc<dyn Fn(String, String, HashMap<String, String>, Option<String>)
    -> Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>> + Send + Sync>;

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
    pub exec_fn: Option<ExecFn>,
    pub fetch_fn: Option<FetchFn>,
}

/// 命令 trait
#[async_trait]
pub trait Command: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, ctx: CommandContext) -> CommandResult;
}
