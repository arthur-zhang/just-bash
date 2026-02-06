//! Bash Environment
//!
//! Main entry point for the bash shell environment.
//! Ties together the parser, interpreter, and filesystem.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::fs::{FileSystem, FsError, InMemoryFs, MkdirOptions};
use crate::interpreter::types::{ExecResult, ExecutionLimits, InterpreterState};
use crate::interpreter::helpers::shellopts::{build_shellopts, build_bashopts};

/// Options for creating a Bash environment.
#[derive(Default)]
pub struct BashOptions {
    /// Environment variables
    pub env: Option<HashMap<String, String>>,
    /// Working directory
    pub cwd: Option<String>,
    /// File system instance (defaults to InMemoryFs)
    pub fs: Option<Arc<dyn FileSystem>>,
    /// Execution limits
    pub limits: Option<ExecutionLimits>,
}

/// Per-execution options.
pub struct ExecOptions {
    /// Temporary environment variables
    pub env: Option<HashMap<String, String>>,
    /// Temporary working directory
    pub cwd: Option<String>,
    /// Skip script normalization
    pub raw_script: bool,
}

/// The main Bash shell environment.
pub struct Bash {
    pub fs: Arc<dyn FileSystem>,
    limits: ExecutionLimits,
    state: InterpreterState,
}

impl Bash {
    /// Create a new Bash environment.
    pub async fn new(options: BashOptions) -> Self {
        let use_default_layout = options.cwd.is_none();
        let cwd = options.cwd.unwrap_or_else(|| "/home/user".to_string());

        let fs: Arc<dyn FileSystem> = options.fs.unwrap_or_else(|| {
            Arc::new(InMemoryFs::new())
        });

        let limits = options.limits.unwrap_or_default();

        // Build default environment
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), if use_default_layout { "/home/user" } else { "/" }.to_string());
        env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        env.insert("IFS".to_string(), " \t\n".to_string());
        env.insert("OSTYPE".to_string(), "linux-gnu".to_string());
        env.insert("MACHTYPE".to_string(), "x86_64-pc-linux-gnu".to_string());
        env.insert("HOSTTYPE".to_string(), "x86_64".to_string());
        env.insert("HOSTNAME".to_string(), "localhost".to_string());
        env.insert("PWD".to_string(), cwd.clone());
        env.insert("OLDPWD".to_string(), cwd.clone());
        env.insert("OPTIND".to_string(), "1".to_string());

        // Merge user-provided env
        let user_env_keys: Vec<String>;
        if let Some(user_env) = options.env {
            user_env_keys = user_env.keys().cloned().collect();
            env.extend(user_env);
        } else {
            user_env_keys = Vec::new();
        }

        // Build exported vars set
        let mut exported = HashSet::new();
        exported.insert("HOME".to_string());
        exported.insert("PATH".to_string());
        exported.insert("PWD".to_string());
        exported.insert("OLDPWD".to_string());
        for key in &user_env_keys {
            exported.insert(key.clone());
        }

        let mut state = InterpreterState::default();
        state.env = env;
        state.cwd = cwd.clone();
        state.previous_dir = if use_default_layout { "/home/user".to_string() } else { "/".to_string() };
        state.exported_vars = Some(exported);
        state.readonly_vars = Some(["SHELLOPTS".to_string(), "BASHOPTS".to_string()].into_iter().collect());

        // Set SHELLOPTS and BASHOPTS
        let shellopts = build_shellopts(&state.options);
        let bashopts = build_bashopts(&state.shopt_options);
        state.env.insert("SHELLOPTS".to_string(), shellopts);
        state.env.insert("BASHOPTS".to_string(), bashopts);

        // Initialize filesystem
        init_filesystem(&*fs, use_default_layout).await;

        // Ensure cwd exists
        let _ = fs.mkdir(&cwd, &MkdirOptions { recursive: true }).await;

        Self { fs, limits, state }
    }

    /// Execute a bash script.
    pub async fn exec(&mut self, script: &str, options: Option<ExecOptions>) -> ExecResult {
        if self.state.call_depth == 0 {
            self.state.command_count = 0;
        }

        self.state.command_count += 1;
        if self.state.command_count > self.limits.max_command_count {
            return ExecResult::new(
                String::new(),
                format!(
                    "bash: maximum command count ({}) exceeded (possible infinite loop)\n",
                    self.limits.max_command_count
                ),
                1,
            );
        }

        let trimmed = script.trim();
        if trimmed.is_empty() {
            return ExecResult::ok();
        }

        // Normalize script (strip leading whitespace, preserve heredocs)
        let normalized = if options.as_ref().map_or(false, |o| o.raw_script) {
            script.to_string()
        } else {
            normalize_script(script)
        };

        // Parse the script
        match crate::parser::parse(&normalized) {
            Ok(ast) => {
                // Execute AST via interpreter
                let fs = self.fs.clone();
                let limits = self.limits.clone();
                let state = &mut self.state;

                // Use block_in_place to bridge async context with sync execution engine
                tokio::task::block_in_place(|| {
                    let handle = tokio::runtime::Handle::current();
                    let sync_fs = crate::interpreter::SyncFsAdapter::new(fs, handle);
                    let engine = crate::interpreter::ExecutionEngine::new(&limits, &sync_fs);

                    match engine.execute_script(state, &ast) {
                        Ok(result) => result,
                        Err(crate::interpreter::InterpreterError::Exit(e)) => {
                            ExecResult::new(e.stdout, e.stderr, e.exit_code)
                        }
                        Err(crate::interpreter::InterpreterError::ExecutionLimit(e)) => {
                            ExecResult::new(e.stdout, e.stderr, 126)
                        }
                        Err(e) => {
                            ExecResult::new(String::new(), format!("{}\n", e), 1)
                        }
                    }
                })
            }
            Err(e) => {
                let msg = e.to_string();
                ExecResult::new(String::new(), format!("bash: syntax error: {}\n", msg), 2)
            }
        }
    }

    /// Read a file relative to cwd.
    pub async fn read_file(&self, path: &str) -> Result<String, FsError> {
        let resolved = self.fs.resolve_path(&self.state.cwd, path);
        self.fs.read_file(&resolved).await
    }

    /// Write a file relative to cwd.
    pub async fn write_file(&self, path: &str, content: &str) -> Result<(), FsError> {
        let resolved = self.fs.resolve_path(&self.state.cwd, path);
        self.fs.write_file(&resolved, content.as_bytes()).await
    }

    /// Get current working directory.
    pub fn get_cwd(&self) -> &str {
        &self.state.cwd
    }

    /// Get environment variables.
    pub fn get_env(&self) -> &HashMap<String, String> {
        &self.state.env
    }
}

/// Initialize the filesystem with standard directories and device files.
async fn init_filesystem(fs: &dyn FileSystem, use_default_layout: bool) {
    let _ = fs.mkdir("/bin", &MkdirOptions { recursive: true }).await;
    let _ = fs.mkdir("/usr/bin", &MkdirOptions { recursive: true }).await;

    if use_default_layout {
        let _ = fs.mkdir("/home/user", &MkdirOptions { recursive: true }).await;
        let _ = fs.mkdir("/tmp", &MkdirOptions { recursive: true }).await;
    }

    // /dev files
    let _ = fs.mkdir("/dev", &MkdirOptions { recursive: true }).await;
    let _ = fs.write_file("/dev/null", b"").await;
    let _ = fs.write_file("/dev/zero", b"").await;
    let _ = fs.write_file("/dev/stdin", b"").await;
    let _ = fs.write_file("/dev/stdout", b"").await;
    let _ = fs.write_file("/dev/stderr", b"").await;

    // /proc files
    let _ = fs.mkdir("/proc/self/fd", &MkdirOptions { recursive: true }).await;
    let _ = fs.write_file("/proc/version", b"Linux version 6.1.0-just-bash\n").await;
    let _ = fs.write_file("/proc/self/exe", b"/bin/bash").await;
    let _ = fs.write_file("/proc/self/cmdline", b"bash\0").await;
    let _ = fs.write_file("/proc/self/comm", b"bash\n").await;
    let _ = fs.write_file("/proc/self/fd/0", b"/dev/stdin").await;
    let _ = fs.write_file("/proc/self/fd/1", b"/dev/stdout").await;
    let _ = fs.write_file("/proc/self/fd/2", b"/dev/stderr").await;
}

/// Normalize a script by stripping leading whitespace while preserving heredoc content.
fn normalize_script(script: &str) -> String {
    let lines: Vec<&str> = script.split('\n').collect();
    let mut result = Vec::new();
    let mut pending_delimiters: Vec<(String, bool)> = Vec::new(); // (delimiter, strip_tabs)

    for line in lines {
        if !pending_delimiters.is_empty() {
            let (delim, strip_tabs) = pending_delimiters.last().unwrap();
            let line_to_check = if *strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line
            };
            if line_to_check == delim {
                result.push(line.trim_start());
                pending_delimiters.pop();
                continue;
            }
            // Inside heredoc - preserve exactly
            result.push(line);
            continue;
        }

        // Not inside heredoc - normalize and check for heredoc starts
        let normalized_line = line.trim_start();
        result.push(normalized_line);

        // Check for heredoc operators: <<DELIM, <<-DELIM, <<'DELIM', <<"DELIM"
        let mut chars = normalized_line.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '<' {
                if chars.peek() == Some(&'<') {
                    chars.next();
                    let strip_tabs = chars.peek() == Some(&'-');
                    if strip_tabs {
                        chars.next();
                    }
                    // Skip whitespace
                    while chars.peek() == Some(&' ') || chars.peek() == Some(&'\t') {
                        chars.next();
                    }
                    // Read delimiter (possibly quoted)
                    let quote = match chars.peek() {
                        Some(&'\'') | Some(&'"') => {
                            let q = chars.next().unwrap();
                            Some(q)
                        }
                        _ => None,
                    };
                    let mut delim = String::new();
                    for ch in chars.by_ref() {
                        if let Some(q) = quote {
                            if ch == q {
                                break;
                            }
                        } else if !ch.is_alphanumeric() && ch != '_' && ch != '-' {
                            break;
                        }
                        delim.push(ch);
                    }
                    if !delim.is_empty() {
                        pending_delimiters.push((delim, strip_tabs));
                    }
                }
            }
        }
    }

    result.join("\n")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bash_new_default() {
        let bash = Bash::new(BashOptions::default()).await;
        assert_eq!(bash.get_cwd(), "/home/user");
        assert_eq!(bash.get_env().get("HOME"), Some(&"/home/user".to_string()));
        assert_eq!(bash.get_env().get("PATH"), Some(&"/usr/bin:/bin".to_string()));
    }

    #[tokio::test]
    async fn test_bash_custom_cwd() {
        let bash = Bash::new(BashOptions {
            cwd: Some("/tmp".to_string()),
            ..Default::default()
        }).await;
        assert_eq!(bash.get_cwd(), "/tmp");
    }

    #[tokio::test]
    async fn test_exec_empty() {
        let mut bash = Bash::new(BashOptions::default()).await;
        let result = bash.exec("", None).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_exec_syntax_error() {
        let mut bash = Bash::new(BashOptions::default()).await;
        let result = bash.exec("if then", None).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("syntax error"));
    }

    #[tokio::test]
    async fn test_filesystem_initialized() {
        let bash = Bash::new(BashOptions::default()).await;
        assert!(bash.fs.exists("/bin").await);
        assert!(bash.fs.exists("/usr/bin").await);
        assert!(bash.fs.exists("/dev/null").await);
        assert!(bash.fs.exists("/home/user").await);
        assert!(bash.fs.exists("/tmp").await);
    }

    #[tokio::test]
    async fn test_read_write_file() {
        let bash = Bash::new(BashOptions::default()).await;
        bash.write_file("test.txt", "hello world").await.unwrap();
        let content = bash.read_file("test.txt").await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_normalize_script_basic() {
        let script = "  echo hello\n  echo world";
        let result = normalize_script(script);
        assert_eq!(result, "echo hello\necho world");
    }

    #[test]
    fn test_normalize_script_heredoc() {
        // With <<EOF (no dash), delimiter must match exactly
        let script = "  cat <<EOF\n  preserved\nEOF";
        let result = normalize_script(script);
        assert_eq!(result, "cat <<EOF\n  preserved\nEOF");
    }

    // ========================================================================
    // Integration tests for execution engine
    // ========================================================================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_exec_echo() {
        let mut bash = Bash::new(BashOptions::default()).await;
        let result = bash.exec("echo hello world", None).await;
        assert_eq!(result.stdout, "hello world\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_exec_variable_expansion() {
        let mut bash = Bash::new(BashOptions::default()).await;
        let result = bash.exec("echo $HOME", None).await;
        assert_eq!(result.stdout, "/home/user\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_exec_if_statement() {
        let mut bash = Bash::new(BashOptions::default()).await;
        let result = bash.exec("if true; then echo yes; fi", None).await;
        assert_eq!(result.stdout, "yes\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_exec_for_loop() {
        let mut bash = Bash::new(BashOptions::default()).await;
        let result = bash.exec("for i in a b c; do echo $i; done", None).await;
        assert_eq!(result.stdout, "a\nb\nc\n");
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_exec_and_or() {
        let mut bash = Bash::new(BashOptions::default()).await;

        let result = bash.exec("true && echo yes", None).await;
        assert_eq!(result.stdout, "yes\n");

        let result = bash.exec("false || echo fallback", None).await;
        assert_eq!(result.stdout, "fallback\n");

        let result = bash.exec("false && echo no", None).await;
        assert_eq!(result.stdout, "");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_exec_pwd_cd() {
        let mut bash = Bash::new(BashOptions::default()).await;

        let result = bash.exec("pwd", None).await;
        assert_eq!(result.stdout, "/home/user\n");

        let result = bash.exec("cd /tmp && pwd", None).await;
        assert_eq!(result.stdout, "/tmp\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_exec_exit() {
        let mut bash = Bash::new(BashOptions::default()).await;
        let result = bash.exec("exit 42", None).await;
        assert_eq!(result.exit_code, 42);
    }
}
