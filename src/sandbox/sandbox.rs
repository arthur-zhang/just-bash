use std::collections::HashMap;

use crate::bash::{Bash, BashOptions};
use crate::fs::MkdirOptions;
use crate::interpreter::types::ExecutionLimits;

use super::types::*;

pub struct Sandbox {
    bash: Bash,
}

impl Sandbox {
    /// Create a new Sandbox with the given options.
    pub async fn create(opts: Option<SandboxOptions>) -> Self {
        let opts = opts.unwrap_or_default();
        let limits = ExecutionLimits {
            max_recursion_depth: opts.max_call_depth.unwrap_or(1000),
            max_command_count: opts.max_command_count.unwrap_or(100_000),
            max_iterations: opts.max_loop_iterations.unwrap_or(1_000_000),
        };
        let bash = Bash::new(BashOptions {
            env: opts.env,
            cwd: opts.cwd,
            fs: None,
            limits: Some(limits),
        })
        .await;
        Self { bash }
    }

    /// Execute a command in the sandbox.
    ///
    /// If `RunCommandOptions` specifies `cwd` or `env`, they are applied
    /// by wrapping the command in a shell script that sets the environment
    /// and changes directory before executing the user command.
    pub async fn run_command(
        &mut self,
        cmd: &str,
        opts: Option<RunCommandOptions>,
    ) -> SandboxCommand {
        let opts = opts.unwrap_or_default();

        // Build a wrapper script that applies cwd/env before the command
        let mut preamble_parts: Vec<String> = Vec::new();

        // Apply per-command environment variables using for-loop trick
        // (plain assignments are not yet processed by the execution engine)
        if let Some(env) = &opts.env {
            for (key, value) in env {
                let escaped = value.replace('\\', "\\\\").replace(' ', "\\ ");
                preamble_parts.push(
                    format!("for {} in {}; do true; done", key, escaped),
                );
            }
        }

        // Apply per-command working directory
        if let Some(cwd) = &opts.cwd {
            preamble_parts.push(format!("cd '{}'", cwd));
        }

        let script = if preamble_parts.is_empty() {
            cmd.to_string()
        } else {
            // Join preamble with && and append the user command
            format!("{} && {}", preamble_parts.join(" && "), cmd)
        };

        let result = self.bash.exec(&script, None).await;
        SandboxCommand::from_exec_result(&result)
    }

    /// Write multiple files to the sandbox filesystem.
    /// Parent directories are created automatically.
    pub async fn write_files(
        &mut self,
        files: HashMap<String, FileContent>,
    ) -> Result<(), String> {
        for (path, content) in &files {
            let data = match content {
                FileContent::Text(s) => s.clone(),
                FileContent::Encoded {
                    content: c,
                    encoding,
                } => match encoding {
                    FileEncoding::Base64 => {
                        use base64::Engine;
                        let bytes = base64::engine::general_purpose::STANDARD
                            .decode(c)
                            .map_err(|e| format!("base64 decode error: {}", e))?;
                        String::from_utf8(bytes)
                            .map_err(|e| format!("utf-8 decode error: {}", e))?
                    }
                    FileEncoding::Utf8 => c.clone(),
                },
            };
            // Ensure parent directory exists
            if let Some(last_slash) = path.rfind('/') {
                let parent = if last_slash == 0 {
                    "/"
                } else {
                    &path[..last_slash]
                };
                if parent != "/" {
                    let _ = self
                        .bash
                        .fs
                        .mkdir(parent, &MkdirOptions { recursive: true })
                        .await;
                }
            }
            self.bash
                .write_file(path, &data)
                .await
                .map_err(|e| format!("write error for {}: {}", path, e))?;
        }
        Ok(())
    }

    /// Read a file from the sandbox filesystem.
    pub async fn read_file(
        &self,
        path: &str,
        encoding: Option<FileEncoding>,
    ) -> Result<String, String> {
        let content = self
            .bash
            .read_file(path)
            .await
            .map_err(|e| format!("read error: {}", e))?;
        match encoding {
            Some(FileEncoding::Base64) => {
                use base64::Engine;
                Ok(base64::engine::general_purpose::STANDARD
                    .encode(content.as_bytes()))
            }
            _ => Ok(content),
        }
    }

    /// Create a directory in the sandbox.
    pub async fn mkdir(&self, path: &str, recursive: bool) -> Result<(), String> {
        self.bash
            .fs
            .mkdir(path, &MkdirOptions { recursive })
            .await
            .map_err(|e| format!("mkdir error: {}", e))
    }

    /// Get current working directory.
    pub fn get_cwd(&self) -> &str {
        self.bash.get_cwd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_create_default() {
        let sandbox = Sandbox::create(None).await;
        assert_eq!(sandbox.get_cwd(), "/home/user");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_create_with_options() {
        let sandbox = Sandbox::create(Some(SandboxOptions {
            cwd: Some("/tmp".to_string()),
            ..Default::default()
        }))
        .await;
        assert_eq!(sandbox.get_cwd(), "/tmp");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_run_command_echo() {
        let mut sandbox = Sandbox::create(None).await;
        let result = sandbox.run_command("echo hello", None).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "hello\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_run_command_with_cwd() {
        let mut sandbox = Sandbox::create(None).await;
        let result = sandbox
            .run_command(
                "pwd",
                Some(RunCommandOptions {
                    cwd: Some("/tmp".to_string()),
                    ..Default::default()
                }),
            )
            .await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/tmp\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_run_command_with_env() {
        let mut sandbox = Sandbox::create(None).await;
        let mut env = HashMap::new();
        env.insert("MY_VAR".to_string(), "test_value".to_string());
        let result = sandbox
            .run_command(
                "echo $MY_VAR",
                Some(RunCommandOptions {
                    env: Some(env),
                    ..Default::default()
                }),
            )
            .await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "test_value\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_run_command_exit_code() {
        let mut sandbox = Sandbox::create(None).await;
        let result = sandbox.run_command("false", None).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_write_and_read_files() {
        let mut sandbox = Sandbox::create(None).await;
        let mut files = HashMap::new();
        files.insert(
            "/home/user/test.txt".to_string(),
            FileContent::Text("hello world".to_string()),
        );
        sandbox.write_files(files).await.unwrap();
        let content = sandbox
            .read_file("/home/user/test.txt", None)
            .await
            .unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_write_files_base64() {
        let mut sandbox = Sandbox::create(None).await;
        let mut files = HashMap::new();
        files.insert(
            "/home/user/b64.txt".to_string(),
            FileContent::Encoded {
                content: "aGVsbG8=".to_string(),
                encoding: FileEncoding::Base64,
            },
        );
        sandbox.write_files(files).await.unwrap();
        let content = sandbox
            .read_file("/home/user/b64.txt", None)
            .await
            .unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_write_files_creates_parent_dirs() {
        let mut sandbox = Sandbox::create(None).await;
        let mut files = HashMap::new();
        files.insert(
            "/deep/nested/dir/file.txt".to_string(),
            FileContent::Text("nested content".to_string()),
        );
        sandbox.write_files(files).await.unwrap();
        let content = sandbox
            .read_file("/deep/nested/dir/file.txt", None)
            .await
            .unwrap();
        assert_eq!(content, "nested content");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_read_file_base64_encoding() {
        let mut sandbox = Sandbox::create(None).await;
        let mut files = HashMap::new();
        files.insert(
            "/home/user/plain.txt".to_string(),
            FileContent::Text("hello".to_string()),
        );
        sandbox.write_files(files).await.unwrap();
        let content = sandbox
            .read_file("/home/user/plain.txt", Some(FileEncoding::Base64))
            .await
            .unwrap();
        assert_eq!(content, "aGVsbG8=");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_read_file_not_found() {
        let sandbox = Sandbox::create(None).await;
        let result = sandbox.read_file("/nonexistent/file.txt", None).await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_mkdir() {
        let sandbox = Sandbox::create(None).await;
        sandbox.mkdir("/test_dir", false).await.unwrap();
        // Verify directory exists via the filesystem
        assert!(sandbox.bash.fs.exists("/test_dir").await);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_mkdir_recursive() {
        let sandbox = Sandbox::create(None).await;
        sandbox.mkdir("/a/b/c", true).await.unwrap();
        // Verify all directories exist via the filesystem
        assert!(sandbox.bash.fs.exists("/a/b/c").await);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_multiple_commands_share_state() {
        let mut sandbox = Sandbox::create(None).await;
        // Use cd to change state, then verify it persists across calls
        sandbox.run_command("cd /tmp", None).await;
        let result = sandbox.run_command("pwd", None).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "/tmp\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sandbox_get_cwd() {
        let sandbox = Sandbox::create(None).await;
        assert_eq!(sandbox.get_cwd(), "/home/user");
    }
}
