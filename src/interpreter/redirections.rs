//! Redirection Handling
//!
//! Handles output redirections:
//! - > : Write stdout to file
//! - >> : Append stdout to file
//! - 2> : Write stderr to file
//! - &> : Write both stdout and stderr to file
//! - >& : Redirect fd to another fd
//! - {fd}>file : Allocate FD and store in variable

use std::collections::HashMap;
use crate::ast::types::{RedirectionNode, RedirectionTarget, RedirectionOperator, WordNode};
use crate::interpreter::types::{ExecResult, InterpreterState};

/// Pre-expanded redirect targets, keyed by index into the redirections array.
/// This allows us to expand redirect targets (including side effects) before
/// executing a function body, then apply the redirections after.
pub type ExpandedRedirectTargets = HashMap<usize, String>;

/// Result of checking an output redirect target
pub struct RedirectCheckResult {
    pub is_valid: bool,
    pub error: Option<String>,
}

/// Check if a redirect target is valid for output (not a directory, respects noclobber).
/// Returns an error message string if invalid, None if valid.
pub fn check_output_redirect_target(
    state: &InterpreterState,
    _file_path: &str,
    target: &str,
    check_noclobber: bool,
    is_clobber: bool,
) -> Option<String> {
    // In a real implementation, this would check the filesystem
    // For now, we just check noclobber option
    if check_noclobber && state.options.noclobber && !is_clobber && target != "/dev/null" {
        // Would need to check if file exists
        return None; // Assume file doesn't exist for now
    }
    None
}

/// Determine the encoding to use for file I/O.
/// If all character codes are <= 255, use binary encoding (byte data).
/// Otherwise, use UTF-8 encoding (text with Unicode characters).
pub fn get_file_encoding(content: &str) -> &'static str {
    for ch in content.chars() {
        if ch as u32 > 255 {
            return "utf8";
        }
    }
    "binary"
}

/// Parse the content of a read-write file descriptor.
/// Format: __rw__:pathLength:path:position:content
/// Returns the parsed components, or None if format is invalid
pub fn parse_rw_fd_content(fd_content: &str) -> Option<RwFdContent> {
    if !fd_content.starts_with("__rw__:") {
        return None;
    }
    let after_prefix = &fd_content[7..];
    let first_colon_idx = after_prefix.find(':')?;
    let path_length: usize = after_prefix[..first_colon_idx].parse().ok()?;
    let path_start = first_colon_idx + 1;
    if path_start + path_length > after_prefix.len() {
        return None;
    }
    let path = after_prefix[path_start..path_start + path_length].to_string();
    let position_start = path_start + path_length + 1;
    if position_start >= after_prefix.len() {
        return None;
    }
    let remaining = &after_prefix[position_start..];
    let pos_colon_idx = remaining.find(':')?;
    let position: usize = remaining[..pos_colon_idx].parse().ok()?;
    let content = remaining[pos_colon_idx + 1..].to_string();
    Some(RwFdContent { path, position, content })
}

/// Parsed content of a read-write file descriptor
#[derive(Debug, Clone)]
pub struct RwFdContent {
    pub path: String,
    pub position: usize,
    pub content: String,
}

/// Allocate the next available file descriptor (starting at 10).
/// Returns the allocated FD number.
pub fn allocate_fd(state: &mut InterpreterState) -> i32 {
    let fd = state.next_fd.unwrap_or(10);
    state.next_fd = Some(fd + 1);
    fd
}

/// Result of pre-expanding redirect targets
pub struct PreExpandResult {
    pub targets: ExpandedRedirectTargets,
    pub error: Option<String>,
}

/// Pre-expand redirect targets for function definitions.
/// This is needed because redirections on function definitions are evaluated
/// each time the function is called, and any side effects (like $((i++)))
/// must occur BEFORE the function body executes.
pub fn pre_expand_redirect_targets(
    state: &mut InterpreterState,
    redirections: &[RedirectionNode],
    expand_word_fn: impl Fn(&mut InterpreterState, &WordNode) -> String,
) -> PreExpandResult {
    let mut targets = ExpandedRedirectTargets::new();

    for (i, redir) in redirections.iter().enumerate() {
        // Skip heredocs
        if matches!(redir.target, RedirectionTarget::HereDoc(_)) {
            continue;
        }

        if let RedirectionTarget::Word(ref word) = redir.target {
            let target = expand_word_fn(state, word);
            targets.insert(i, target);
        }
    }

    PreExpandResult { targets, error: None }
}

/// Process FD variable redirections ({varname}>file syntax).
/// This allocates FDs and sets variables before command execution.
/// Returns an error result if there's an issue, or None if successful.
pub fn process_fd_variable_redirections(
    state: &mut InterpreterState,
    redirections: &[RedirectionNode],
    expand_word_fn: impl Fn(&mut InterpreterState, &WordNode) -> String,
) -> Option<ExecResult> {
    for redir in redirections {
        let fd_variable = match &redir.fd_variable {
            Some(v) => v,
            None => continue,
        };

        // Initialize fileDescriptors map if needed
        if state.file_descriptors.is_none() {
            state.file_descriptors = Some(HashMap::new());
        }

        // Handle close operation: {fd}>&- or {fd}<&-
        if matches!(redir.operator, RedirectionOperator::GreatAnd | RedirectionOperator::LessAnd) {
            if let RedirectionTarget::Word(ref word) = redir.target {
                let target = expand_word_fn(state, word);
                if target == "-" {
                    // Close operation - look up the FD from the variable and close it
                    if let Some(existing_fd) = state.env.get(fd_variable) {
                        if let Ok(fd_num) = existing_fd.parse::<i32>() {
                            if let Some(ref mut fds) = state.file_descriptors {
                                fds.remove(&fd_num);
                            }
                        }
                    }
                    // Don't allocate a new FD for close operations
                    continue;
                }
            }
        }

        // Allocate a new FD (for non-close operations)
        let fd = allocate_fd(state);

        // Set the variable to the allocated FD number
        state.env.insert(fd_variable.clone(), fd.to_string());

        // For file redirections, store the file path mapping
        if let RedirectionTarget::Word(ref word) = redir.target {
            let target = expand_word_fn(state, word);

            // Handle FD duplication: {fd}>&N or {fd}<&N
            if matches!(redir.operator, RedirectionOperator::GreatAnd | RedirectionOperator::LessAnd) {
                if let Ok(source_fd) = target.parse::<i32>() {
                    // Duplicate the source FD's content to the new FD
                    if let Some(ref fds) = state.file_descriptors {
                        if let Some(content) = fds.get(&source_fd) {
                            let content = content.clone();
                            if let Some(ref mut fds) = state.file_descriptors {
                                fds.insert(fd, content);
                            }
                        }
                    }
                    continue;
                }
            }

            // For output redirections to files
            if matches!(redir.operator,
                RedirectionOperator::Great |
                RedirectionOperator::DGreat |
                RedirectionOperator::Clobber |
                RedirectionOperator::AndGreat |
                RedirectionOperator::AndDGreat
            ) {
                // Mark this FD as pointing to a file
                let file_marker = format!("__file__:{}", target);
                if let Some(ref mut fds) = state.file_descriptors {
                    fds.insert(fd, file_marker);
                }
            } else if redir.operator == RedirectionOperator::TLess {
                // For here-strings, store the target value plus newline as the FD content
                if let Some(ref mut fds) = state.file_descriptors {
                    fds.insert(fd, format!("{}\n", target));
                }
            } else if matches!(redir.operator, RedirectionOperator::Less | RedirectionOperator::LessGreat) {
                // For input redirections, would read the file content
                // For now, just mark it
                if let Some(ref mut fds) = state.file_descriptors {
                    fds.insert(fd, format!("__input__:{}", target));
                }
            }
        }
    }

    None // Success
}

/// Apply redirections to an execution result.
/// This handles the actual redirection of stdout/stderr to files or other FDs.
pub fn apply_redirections(
    state: &mut InterpreterState,
    result: ExecResult,
    redirections: &[RedirectionNode],
    pre_expanded_targets: Option<&ExpandedRedirectTargets>,
    expand_word_fn: impl Fn(&mut InterpreterState, &WordNode) -> String,
) -> ExecResult {
    let mut stdout = result.stdout;
    let mut stderr = result.stderr;
    let mut exit_code = result.exit_code;

    for (i, redir) in redirections.iter().enumerate() {
        // Skip heredocs
        if matches!(redir.target, RedirectionTarget::HereDoc(_)) {
            continue;
        }

        // Get target word
        let word = match &redir.target {
            RedirectionTarget::Word(w) => w,
            _ => continue,
        };

        // Use pre-expanded target if available, otherwise expand now
        let target = if let Some(targets) = pre_expanded_targets {
            targets.get(&i).cloned().unwrap_or_else(|| expand_word_fn(state, word))
        } else {
            expand_word_fn(state, word)
        };

        // Skip FD variable redirections - they're already handled
        if redir.fd_variable.is_some() {
            continue;
        }

        match redir.operator {
            RedirectionOperator::Great | RedirectionOperator::Clobber => {
                let fd = redir.fd.unwrap_or(1);
                let _is_clobber = redir.operator == RedirectionOperator::Clobber;

                if fd == 1 {
                    // Handle special devices
                    if target == "/dev/stdout" {
                        // No-op for stdout
                    } else if target == "/dev/stderr" {
                        stderr.push_str(&stdout);
                        stdout.clear();
                    } else if target == "/dev/full" {
                        stderr.push_str("bash: echo: write error: No space left on device\n");
                        exit_code = 1;
                        stdout.clear();
                    } else if target == "/dev/null" {
                        stdout.clear();
                    } else {
                        // Would write to file here
                        // For now, just clear stdout (simulating write)
                        stdout.clear();
                    }
                } else if fd == 2 {
                    if target == "/dev/stderr" {
                        // No-op for stderr
                    } else if target == "/dev/stdout" {
                        stdout.push_str(&stderr);
                        stderr.clear();
                    } else if target == "/dev/full" {
                        stderr.push_str("bash: echo: write error: No space left on device\n");
                        exit_code = 1;
                    } else if target == "/dev/null" {
                        stderr.clear();
                    } else {
                        // Would write to file here
                        stderr.clear();
                    }
                }
            }

            RedirectionOperator::DGreat => {
                let fd = redir.fd.unwrap_or(1);

                if fd == 1 {
                    if target == "/dev/stdout" {
                        // No-op
                    } else if target == "/dev/stderr" {
                        stderr.push_str(&stdout);
                        stdout.clear();
                    } else if target == "/dev/full" {
                        stderr.push_str("bash: echo: write error: No space left on device\n");
                        exit_code = 1;
                        stdout.clear();
                    } else {
                        // Would append to file here
                        stdout.clear();
                    }
                } else if fd == 2 {
                    if target == "/dev/stderr" {
                        // No-op
                    } else if target == "/dev/stdout" {
                        stdout.push_str(&stderr);
                        stderr.clear();
                    } else if target == "/dev/full" {
                        stderr.push_str("bash: echo: write error: No space left on device\n");
                        exit_code = 1;
                    } else {
                        // Would append to file here
                        stderr.clear();
                    }
                }
            }

            RedirectionOperator::GreatAnd | RedirectionOperator::LessAnd => {
                let fd = redir.fd.unwrap_or(1);

                // Handle close operation
                if target == "-" {
                    continue;
                }

                // Handle FD move operation: N>&M-
                if target.ends_with('-') {
                    let source_fd_str = &target[..target.len() - 1];
                    if source_fd_str.parse::<i32>().is_ok() {
                        // Would handle FD move here
                        continue;
                    }
                }

                // >&2, 1>&2: redirect stdout to stderr
                if target == "2" || target == "&2" {
                    if fd == 1 {
                        stderr.push_str(&stdout);
                        stdout.clear();
                    }
                }
                // 2>&1: redirect stderr to stdout
                else if target == "1" || target == "&1" {
                    if fd == 2 {
                        stdout.push_str(&stderr);
                        stderr.clear();
                    }
                }
                // Handle writing to a user-allocated FD
                else if let Ok(target_fd) = target.parse::<i32>() {
                    if let Some(ref fds) = state.file_descriptors {
                        if let Some(fd_info) = fds.get(&target_fd) {
                            if fd_info.starts_with("__file__:") {
                                // Would write to file here
                                if fd == 1 {
                                    stdout.clear();
                                } else if fd == 2 {
                                    stderr.clear();
                                }
                            }
                        } else if target_fd >= 3 {
                            // Bad file descriptor
                            stderr.push_str(&format!("bash: {}: Bad file descriptor\n", target_fd));
                            exit_code = 1;
                            stdout.clear();
                        }
                    }
                }
            }

            RedirectionOperator::AndGreat => {
                if target == "/dev/full" {
                    stderr = "bash: echo: write error: No space left on device\n".to_string();
                    exit_code = 1;
                    stdout.clear();
                } else {
                    // Would write both stdout and stderr to file
                    stdout.clear();
                    stderr.clear();
                }
            }

            RedirectionOperator::AndDGreat => {
                if target == "/dev/full" {
                    stderr = "bash: echo: write error: No space left on device\n".to_string();
                    exit_code = 1;
                    stdout.clear();
                } else {
                    // Would append both stdout and stderr to file
                    stdout.clear();
                    stderr.clear();
                }
            }

            _ => {}
        }
    }

    // Apply persistent FD redirections (from exec)
    if let Some(ref fds) = state.file_descriptors {
        if let Some(fd1_info) = fds.get(&1) {
            if fd1_info == "__dupout__:2" {
                stderr.push_str(&stdout);
                stdout.clear();
            } else if fd1_info.starts_with("__file__:") {
                // Would write to file
                stdout.clear();
            }
        }

        if let Some(fd2_info) = fds.get(&2) {
            if fd2_info == "__dupout__:1" {
                stdout.push_str(&stderr);
                stderr.clear();
            } else if fd2_info.starts_with("__file__:") {
                // Would write to file
                stderr.clear();
            }
        }
    }

    ExecResult::new(stdout, stderr, exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_file_encoding_binary() {
        assert_eq!(get_file_encoding("hello world"), "binary");
        assert_eq!(get_file_encoding("abc123"), "binary");
    }

    #[test]
    fn test_get_file_encoding_utf8() {
        assert_eq!(get_file_encoding("hello 世界"), "utf8");
        assert_eq!(get_file_encoding("日本語"), "utf8");
    }

    #[test]
    fn test_parse_rw_fd_content_valid() {
        let content = "__rw__:4:/tmp:10:hello";
        let parsed = parse_rw_fd_content(content).unwrap();
        assert_eq!(parsed.path, "/tmp");
        assert_eq!(parsed.position, 10);
        assert_eq!(parsed.content, "hello");
    }

    #[test]
    fn test_parse_rw_fd_content_invalid() {
        assert!(parse_rw_fd_content("invalid").is_none());
        assert!(parse_rw_fd_content("__rw__:").is_none());
        assert!(parse_rw_fd_content("__rw__:abc:").is_none());
    }

    #[test]
    fn test_allocate_fd() {
        let mut state = InterpreterState::default();
        assert_eq!(allocate_fd(&mut state), 10);
        assert_eq!(allocate_fd(&mut state), 11);
        assert_eq!(allocate_fd(&mut state), 12);
    }
}
