use std::collections::HashMap;
use crate::interpreter::types::ExecResult;

/// Output message type (stdout or stderr)
#[derive(Debug, Clone, PartialEq)]
pub enum OutputType {
    Stdout,
    Stderr,
}

/// A single output message from command execution.
#[derive(Debug, Clone)]
pub struct OutputMessage {
    pub output_type: OutputType,
    pub data: String,
}

/// Options for creating a Sandbox.
#[derive(Debug, Default)]
pub struct SandboxOptions {
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub max_call_depth: Option<u32>,
    pub max_command_count: Option<u64>,
    pub max_loop_iterations: Option<u64>,
}

/// Options for running a command.
#[derive(Debug, Default)]
pub struct RunCommandOptions {
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
}

/// Input for writing files.
#[derive(Debug, Clone)]
pub enum FileContent {
    Text(String),
    Encoded { content: String, encoding: FileEncoding },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileEncoding {
    Utf8,
    Base64,
}

/// Result of a completed command execution.
#[derive(Debug, Clone)]
pub struct SandboxCommand {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl SandboxCommand {
    pub fn from_exec_result(result: &ExecResult) -> Self {
        Self {
            exit_code: result.exit_code,
            stdout: result.stdout.clone(),
            stderr: result.stderr.clone(),
        }
    }

    /// Get combined stdout + stderr output.
    pub fn output(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    /// Get output messages as a Vec.
    pub fn logs(&self) -> Vec<OutputMessage> {
        let mut messages = Vec::new();
        if !self.stdout.is_empty() {
            messages.push(OutputMessage {
                output_type: OutputType::Stdout,
                data: self.stdout.clone(),
            });
        }
        if !self.stderr.is_empty() {
            messages.push(OutputMessage {
                output_type: OutputType::Stderr,
                data: self.stderr.clone(),
            });
        }
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::types::ExecResult;

    #[test]
    fn test_sandbox_command_from_exec_result() {
        let result = ExecResult::new(
            "hello\n".to_string(),
            "warn\n".to_string(),
            42,
        );
        let cmd = SandboxCommand::from_exec_result(&result);
        assert_eq!(cmd.exit_code, 42);
        assert_eq!(cmd.stdout, "hello\n");
        assert_eq!(cmd.stderr, "warn\n");
    }

    #[test]
    fn test_sandbox_command_output_combined() {
        let cmd = SandboxCommand {
            exit_code: 0,
            stdout: "out".to_string(),
            stderr: "err".to_string(),
        };
        assert_eq!(cmd.output(), "outerr");
    }

    #[test]
    fn test_sandbox_command_logs_both() {
        let cmd = SandboxCommand {
            exit_code: 0,
            stdout: "out".to_string(),
            stderr: "err".to_string(),
        };
        let logs = cmd.logs();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].output_type, OutputType::Stdout);
        assert_eq!(logs[0].data, "out");
        assert_eq!(logs[1].output_type, OutputType::Stderr);
        assert_eq!(logs[1].data, "err");
    }

    #[test]
    fn test_sandbox_command_logs_empty() {
        let cmd = SandboxCommand {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        };
        let logs = cmd.logs();
        assert_eq!(logs.len(), 0);
    }

    #[test]
    fn test_sandbox_options_default() {
        let opts = SandboxOptions::default();
        assert!(opts.cwd.is_none());
        assert!(opts.env.is_none());
        assert!(opts.timeout_ms.is_none());
        assert!(opts.max_call_depth.is_none());
        assert!(opts.max_command_count.is_none());
        assert!(opts.max_loop_iterations.is_none());
    }

    #[test]
    fn test_file_content_variants() {
        let text = FileContent::Text("hello".to_string());
        match &text {
            FileContent::Text(s) => assert_eq!(s, "hello"),
            _ => panic!("expected Text variant"),
        }

        let encoded = FileContent::Encoded {
            content: "aGVsbG8=".to_string(),
            encoding: FileEncoding::Base64,
        };
        match &encoded {
            FileContent::Encoded { content, encoding } => {
                assert_eq!(content, "aGVsbG8=");
                assert_eq!(*encoding, FileEncoding::Base64);
            }
            _ => panic!("expected Encoded variant"),
        }
    }

    #[test]
    fn test_file_encoding_equality() {
        assert_eq!(FileEncoding::Utf8, FileEncoding::Utf8);
        assert_ne!(FileEncoding::Base64, FileEncoding::Utf8);
    }

    #[test]
    fn test_output_type_equality() {
        assert_eq!(OutputType::Stdout, OutputType::Stdout);
        assert_ne!(OutputType::Stderr, OutputType::Stdout);
    }
}
