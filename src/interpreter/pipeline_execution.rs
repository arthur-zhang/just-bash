//! Pipeline Execution
//!
//! Handles execution of command pipelines (cmd1 | cmd2 | cmd3).

use std::collections::HashMap;
use crate::interpreter::types::ExecResult;

/// Result of executing a pipeline.
#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    /// Exit codes of all commands in the pipeline (for PIPESTATUS)
    pub pipestatus: Vec<i32>,
}

impl PipelineResult {
    /// Create a new pipeline result.
    pub fn new(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self {
            stdout,
            stderr,
            exit_code,
            pipestatus: vec![exit_code],
        }
    }

    /// Create from an ExecResult.
    pub fn from_exec_result(result: &ExecResult) -> Self {
        Self {
            stdout: result.stdout.clone(),
            stderr: result.stderr.clone(),
            exit_code: result.exit_code,
            pipestatus: vec![result.exit_code],
        }
    }

    /// Convert to an ExecResult.
    pub fn to_exec_result(&self) -> ExecResult {
        ExecResult {
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
            exit_code: self.exit_code,
            env: None,
        }
    }

    /// Apply negation (! prefix).
    pub fn negate(&mut self) {
        self.exit_code = if self.exit_code == 0 { 1 } else { 0 };
    }
}

/// Pipeline execution state.
#[derive(Debug)]
pub struct PipelineState {
    /// Current stdin for the next command
    pub stdin: String,
    /// Last result from a command
    pub last_result: ExecResult,
    /// Exit codes of all commands (for PIPESTATUS)
    pub pipestatus: Vec<i32>,
    /// Rightmost failing exit code (for pipefail)
    pub pipefail_exit_code: i32,
    /// Saved last argument ($_ before pipeline)
    pub saved_last_arg: Option<String>,
    /// Saved environment (for subshell commands)
    pub saved_env: Option<HashMap<String, String>>,
}

impl PipelineState {
    /// Create a new pipeline state.
    pub fn new() -> Self {
        Self {
            stdin: String::new(),
            last_result: ExecResult::ok(),
            pipestatus: Vec::new(),
            pipefail_exit_code: 0,
            saved_last_arg: None,
            saved_env: None,
        }
    }

    /// Record a command result.
    pub fn record_result(&mut self, result: &ExecResult, is_last: bool, pipe_stderr: bool) {
        self.pipestatus.push(result.exit_code);

        if result.exit_code != 0 {
            self.pipefail_exit_code = result.exit_code;
        }

        if !is_last {
            if pipe_stderr {
                // |& pipes both stdout and stderr to next command's stdin
                self.stdin = format!("{}{}", result.stderr, result.stdout);
                self.last_result = ExecResult {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: result.exit_code,
                    env: None,
                };
            } else {
                // Regular | only pipes stdout
                self.stdin = result.stdout.clone();
                self.last_result = ExecResult {
                    stdout: String::new(),
                    stderr: result.stderr.clone(),
                    exit_code: result.exit_code,
                    env: None,
                };
            }
        } else {
            self.last_result = result.clone();
        }
    }

    /// Get the final exit code, considering pipefail option.
    pub fn final_exit_code(&self, pipefail: bool) -> i32 {
        if pipefail && self.pipefail_exit_code != 0 {
            self.pipefail_exit_code
        } else {
            self.last_result.exit_code
        }
    }
}

impl Default for PipelineState {
    fn default() -> Self {
        Self::new()
    }
}

/// Set PIPESTATUS array in the environment.
pub fn set_pipestatus(env: &mut HashMap<String, String>, pipestatus: &[i32]) {
    // Clear any previous PIPESTATUS entries
    let keys_to_remove: Vec<String> = env
        .keys()
        .filter(|k| k.starts_with("PIPESTATUS_"))
        .cloned()
        .collect();
    for key in keys_to_remove {
        env.remove(&key);
    }

    // Set new PIPESTATUS entries
    for (i, &code) in pipestatus.iter().enumerate() {
        env.insert(format!("PIPESTATUS_{}", i), code.to_string());
    }
    env.insert("PIPESTATUS__length".to_string(), pipestatus.len().to_string());
}

/// Get PIPESTATUS array from the environment.
pub fn get_pipestatus(env: &HashMap<String, String>) -> Vec<i32> {
    let length = env
        .get("PIPESTATUS__length")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    let mut result = Vec::with_capacity(length);
    for i in 0..length {
        let code = env
            .get(&format!("PIPESTATUS_{}", i))
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);
        result.push(code);
    }
    result
}

/// Format timing output for timed pipelines.
pub fn format_timing_output(elapsed_seconds: f64, posix_format: bool) -> String {
    if posix_format {
        // POSIX format (-p): decimal format without leading zeros
        format!("real {:.2}\nuser 0.00\nsys 0.00\n", elapsed_seconds)
    } else {
        // Default bash format: real/user/sys with XmY.YYYs
        let minutes = (elapsed_seconds / 60.0).floor() as u32;
        let seconds = elapsed_seconds % 60.0;
        let real_str = format!("{}m{:.3}s", minutes, seconds);
        format!("\nreal\t{}\nuser\t0m0.000s\nsys\t0m0.000s\n", real_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_result_new() {
        let result = PipelineResult::new("out".to_string(), "err".to_string(), 0);
        assert_eq!(result.stdout, "out");
        assert_eq!(result.stderr, "err");
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.pipestatus, vec![0]);
    }

    #[test]
    fn test_pipeline_result_negate() {
        let mut result = PipelineResult::new(String::new(), String::new(), 0);
        result.negate();
        assert_eq!(result.exit_code, 1);

        result.negate();
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_pipeline_state_record_result() {
        let mut state = PipelineState::new();

        let result1 = ExecResult {
            stdout: "out1".to_string(),
            stderr: "err1".to_string(),
            exit_code: 0,
            env: None,
        };
        state.record_result(&result1, false, false);

        assert_eq!(state.stdin, "out1");
        assert_eq!(state.last_result.stderr, "err1");
        assert_eq!(state.pipestatus, vec![0]);

        let result2 = ExecResult {
            stdout: "out2".to_string(),
            stderr: "err2".to_string(),
            exit_code: 1,
            env: None,
        };
        state.record_result(&result2, true, false);

        assert_eq!(state.last_result.stdout, "out2");
        assert_eq!(state.last_result.stderr, "err2");
        assert_eq!(state.pipestatus, vec![0, 1]);
        assert_eq!(state.pipefail_exit_code, 1);
    }

    #[test]
    fn test_pipeline_state_pipe_stderr() {
        let mut state = PipelineState::new();

        let result = ExecResult {
            stdout: "out".to_string(),
            stderr: "err".to_string(),
            exit_code: 0,
            env: None,
        };
        state.record_result(&result, false, true);

        // |& pipes both stderr and stdout
        assert_eq!(state.stdin, "errout");
    }

    #[test]
    fn test_final_exit_code() {
        let mut state = PipelineState::new();

        let result1 = ExecResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 1,
            env: None,
        };
        state.record_result(&result1, false, false);

        let result2 = ExecResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            env: None,
        };
        state.record_result(&result2, true, false);

        // Without pipefail, use last command's exit code
        assert_eq!(state.final_exit_code(false), 0);

        // With pipefail, use rightmost failing exit code
        assert_eq!(state.final_exit_code(true), 1);
    }

    #[test]
    fn test_set_get_pipestatus() {
        let mut env = HashMap::new();

        set_pipestatus(&mut env, &[0, 1, 2]);

        assert_eq!(env.get("PIPESTATUS_0"), Some(&"0".to_string()));
        assert_eq!(env.get("PIPESTATUS_1"), Some(&"1".to_string()));
        assert_eq!(env.get("PIPESTATUS_2"), Some(&"2".to_string()));
        assert_eq!(env.get("PIPESTATUS__length"), Some(&"3".to_string()));

        let pipestatus = get_pipestatus(&env);
        assert_eq!(pipestatus, vec![0, 1, 2]);
    }

    #[test]
    fn test_format_timing_output_posix() {
        let output = format_timing_output(1.5, true);
        assert!(output.contains("real 1.50"));
        assert!(output.contains("user 0.00"));
        assert!(output.contains("sys 0.00"));
    }

    #[test]
    fn test_format_timing_output_bash() {
        let output = format_timing_output(65.123, false);
        assert!(output.contains("real\t1m5.123s"));
        assert!(output.contains("user\t0m0.000s"));
        assert!(output.contains("sys\t0m0.000s"));
    }
}
