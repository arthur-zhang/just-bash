//! read - Read a line of input builtin
//!
//! Supports:
//! - read VAR - read into variable
//! - read -r VAR - raw mode (no backslash escaping)
//! - read -d DELIM VAR - custom delimiter
//! - read -n N VAR - read at most N characters
//! - read -N N VAR - read exactly N characters
//! - read -a ARRAY - read into array
//! - read -t TIMEOUT - timeout in seconds
//! - read -u FD - read from file descriptor

use crate::interpreter::builtins::BuiltinResult;
use crate::interpreter::helpers::{clear_array, get_ifs, split_by_ifs_for_read, strip_trailing_ifs_whitespace};
use crate::interpreter::types::InterpreterState;

/// Parse the content of a read-write file descriptor.
/// Format: __rw__:pathLength:path:position:content
fn parse_rw_fd_content(fd_content: &str) -> Option<(String, usize, String)> {
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
    Some((path, position, content))
}

/// Encode read-write file descriptor content.
fn encode_rw_fd_content(path: &str, position: usize, content: &str) -> String {
    format!("__rw__:{}:{}:{}:{}", path.len(), path, position, content)
}

pub fn handle_read(
    state: &mut InterpreterState,
    args: &[String],
    stdin: &str,
    stdin_source_fd: i32,
) -> BuiltinResult {
    // Parse options
    let mut raw = false;
    let mut delimiter = "\n".to_string();
    let mut _prompt = String::new();
    let mut nchars: i32 = -1; // -n option: number of characters to read (with IFS splitting)
    let mut nchars_exact: i32 = -1; // -N option: read exactly N characters (no processing)
    let mut array_name: Option<String> = None; // -a option: read into array
    let mut file_descriptor: i32 = -1; // -u option: read from file descriptor
    let mut timeout: f64 = -1.0; // -t option: timeout in seconds
    let mut var_names: Vec<String> = Vec::new();

    let mut i = 0;
    let mut invalid_n_arg = false;

    // Helper to parse smooshed options like -rn1 or -rd ''
    while i < args.len() {
        let arg = &args[i];
        if arg.starts_with('-') && arg.len() > 1 && arg != "--" {
            let chars: Vec<char> = arg.chars().collect();
            let mut j = 1; // skip the '-'
            while j < chars.len() {
                let ch = chars[j];
                match ch {
                    'r' => {
                        raw = true;
                        j += 1;
                    }
                    's' => {
                        // Silent - ignore in non-interactive mode
                        j += 1;
                    }
                    'd' => {
                        // -d requires value: either rest of this arg or next arg
                        if j + 1 < chars.len() {
                            delimiter = chars[j + 1..].iter().collect();
                            break;
                        } else if i + 1 < args.len() {
                            i += 1;
                            delimiter = args[i].clone();
                        }
                        break;
                    }
                    'n' => {
                        // -n requires value: either rest of this arg or next arg
                        if j + 1 < chars.len() {
                            let num_str: String = chars[j + 1..].iter().collect();
                            nchars = num_str.parse().unwrap_or_else(|_| {
                                invalid_n_arg = true;
                                0
                            });
                            if nchars < 0 {
                                invalid_n_arg = true;
                                nchars = 0;
                            }
                            break;
                        } else if i + 1 < args.len() {
                            i += 1;
                            nchars = args[i].parse().unwrap_or_else(|_| {
                                invalid_n_arg = true;
                                0
                            });
                            if nchars < 0 {
                                invalid_n_arg = true;
                                nchars = 0;
                            }
                        }
                        break;
                    }
                    'N' => {
                        // -N requires value: either rest of this arg or next arg
                        if j + 1 < chars.len() {
                            let num_str: String = chars[j + 1..].iter().collect();
                            nchars_exact = num_str.parse().unwrap_or_else(|_| {
                                invalid_n_arg = true;
                                0
                            });
                            if nchars_exact < 0 {
                                invalid_n_arg = true;
                                nchars_exact = 0;
                            }
                            break;
                        } else if i + 1 < args.len() {
                            i += 1;
                            nchars_exact = args[i].parse().unwrap_or_else(|_| {
                                invalid_n_arg = true;
                                0
                            });
                            if nchars_exact < 0 {
                                invalid_n_arg = true;
                                nchars_exact = 0;
                            }
                        }
                        break;
                    }
                    'a' => {
                        // -a requires value: either rest of this arg or next arg
                        if j + 1 < chars.len() {
                            array_name = Some(chars[j + 1..].iter().collect());
                            break;
                        } else if i + 1 < args.len() {
                            i += 1;
                            array_name = Some(args[i].clone());
                        }
                        break;
                    }
                    'p' => {
                        // -p requires value: either rest of this arg or next arg
                        if j + 1 < chars.len() {
                            _prompt = chars[j + 1..].iter().collect();
                            break;
                        } else if i + 1 < args.len() {
                            i += 1;
                            _prompt = args[i].clone();
                        }
                        break;
                    }
                    'u' => {
                        // -u requires value: file descriptor number
                        if j + 1 < chars.len() {
                            let num_str: String = chars[j + 1..].iter().collect();
                            file_descriptor = num_str.parse().unwrap_or(-1);
                            if file_descriptor < 0 {
                                return BuiltinResult {
                                    stdout: String::new(),
                                    stderr: String::new(),
                                    exit_code: 1,
                                };
                            }
                            break;
                        } else if i + 1 < args.len() {
                            i += 1;
                            file_descriptor = args[i].parse().unwrap_or(-1);
                            if file_descriptor < 0 {
                                return BuiltinResult {
                                    stdout: String::new(),
                                    stderr: String::new(),
                                    exit_code: 1,
                                };
                            }
                        }
                        break;
                    }
                    't' => {
                        // -t requires value: timeout in seconds (can be float)
                        if j + 1 < chars.len() {
                            let num_str: String = chars[j + 1..].iter().collect();
                            timeout = num_str.parse().unwrap_or(0.0);
                            break;
                        } else if i + 1 < args.len() {
                            i += 1;
                            timeout = args[i].parse().unwrap_or(0.0);
                        }
                        break;
                    }
                    'e' | 'i' | 'P' => {
                        // Interactive options - skip (with potential argument for -i)
                        if ch == 'i' && i + 1 < args.len() {
                            i += 1;
                        }
                        j += 1;
                    }
                    _ => {
                        // Unknown option, skip
                        j += 1;
                    }
                }
            }
            i += 1;
        } else if arg == "--" {
            i += 1;
            // Rest are variable names
            while i < args.len() {
                var_names.push(args[i].clone());
                i += 1;
            }
        } else {
            var_names.push(arg.clone());
            i += 1;
        }
    }

    // Return error if -n had invalid argument
    if invalid_n_arg {
        return BuiltinResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 1,
        };
    }

    // Default variable is REPLY
    if var_names.is_empty() && array_name.is_none() {
        var_names.push("REPLY".to_string());
    }

    // Handle -t 0: check if input is available without reading
    if timeout == 0.0 {
        // Clear any variables to empty (read doesn't actually read anything)
        if let Some(ref arr_name) = array_name {
            clear_array(&mut state.env, arr_name);
        } else {
            for name in &var_names {
                state.env.insert(name.clone(), String::new());
            }
            if var_names.is_empty() {
                state.env.insert("REPLY".to_string(), String::new());
            }
        }
        return BuiltinResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        };
    }

    // Handle negative timeout - bash returns exit code 1
    if timeout < 0.0 && timeout != -1.0 {
        return BuiltinResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 1,
        };
    }

    // Use stdin from parameter, or fall back to group_stdin (for piped groups/while loops)
    // If -u is specified, use the file descriptor content instead
    let mut effective_stdin = stdin.to_string();

    if file_descriptor >= 0 {
        // Read from specified file descriptor
        if let Some(ref fds) = state.file_descriptors {
            effective_stdin = fds.get(&file_descriptor).cloned().unwrap_or_default();
        } else {
            effective_stdin = String::new();
        }
    } else if effective_stdin.is_empty() {
        if let Some(ref group_stdin) = state.group_stdin {
            effective_stdin = group_stdin.clone();
        }
    }

    // Handle -d '' (empty delimiter) - reads until NUL byte
    let effective_delimiter = if delimiter.is_empty() {
        "\0".to_string()
    } else {
        delimiter
    };

    // Get input
    let mut line = String::new();
    let mut consumed: usize = 0;
    let mut found_delimiter = true;

    // Helper closure to consume from the appropriate source
    let consume_input = |state: &mut InterpreterState, bytes_consumed: usize, effective_stdin: &str| {
        if file_descriptor >= 0 {
            if let Some(ref mut fds) = state.file_descriptors {
                fds.insert(file_descriptor, effective_stdin[bytes_consumed..].to_string());
            }
        } else if stdin_source_fd >= 0 {
            if let Some(ref mut fds) = state.file_descriptors {
                if let Some(fd_content) = fds.get(&stdin_source_fd).cloned() {
                    if fd_content.starts_with("__rw__:") {
                        if let Some((path, position, content)) = parse_rw_fd_content(&fd_content) {
                            let new_position = position + bytes_consumed;
                            fds.insert(stdin_source_fd, encode_rw_fd_content(&path, new_position, &content));
                        }
                    }
                }
            }
        } else if state.group_stdin.is_some() && stdin.is_empty() {
            state.group_stdin = Some(effective_stdin[bytes_consumed..].to_string());
        }
    };

    let effective_stdin_chars: Vec<char> = effective_stdin.chars().collect();

    if nchars_exact >= 0 {
        // -N: Read exactly N characters (ignores delimiters, no IFS splitting)
        let to_read = (nchars_exact as usize).min(effective_stdin_chars.len());
        line = effective_stdin_chars[..to_read].iter().collect();
        consumed = line.len();
        found_delimiter = to_read >= nchars_exact as usize;

        // Consume from appropriate source
        consume_input(state, consumed, &effective_stdin);

        // With -N, assign entire content to first variable (no IFS splitting)
        let var_name = var_names.first().map(|s| s.as_str()).unwrap_or("REPLY");
        state.env.insert(var_name.to_string(), line);
        // Set remaining variables to empty
        for j in 1..var_names.len() {
            state.env.insert(var_names[j].clone(), String::new());
        }
        return BuiltinResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: if found_delimiter { 0 } else { 1 },
        };
    } else if nchars >= 0 {
        // -n: Read at most N characters (or until delimiter/EOF), then apply IFS splitting
        let mut char_count = 0;
        let mut input_pos = 0;
        let mut hit_delimiter = false;
        while input_pos < effective_stdin_chars.len() && char_count < nchars as usize {
            let ch = effective_stdin_chars[input_pos];
            if effective_delimiter.starts_with(ch) {
                consumed = input_pos + 1;
                hit_delimiter = true;
                break;
            }
            if !raw && ch == '\\' && input_pos + 1 < effective_stdin_chars.len() {
                let next_char = effective_stdin_chars[input_pos + 1];
                if next_char == '\n' && effective_delimiter == "\n" {
                    // Backslash-newline is a line continuation
                    input_pos += 2;
                    consumed = input_pos;
                    continue;
                }
                if effective_delimiter.starts_with(next_char) {
                    // Backslash-delimiter: counts as one char (the escaped delimiter)
                    input_pos += 2;
                    char_count += 1;
                    line.push(next_char);
                    consumed = input_pos;
                    continue;
                }
                line.push(next_char);
                input_pos += 2;
                char_count += 1;
                consumed = input_pos;
            } else {
                line.push(ch);
                input_pos += 1;
                char_count += 1;
                consumed = input_pos;
            }
        }
        found_delimiter = char_count >= nchars as usize || hit_delimiter;
        consume_input(state, consumed, &effective_stdin);
    } else {
        // Read until delimiter, handling line continuation (backslash-newline) if not raw mode
        consumed = 0;
        let mut input_pos = 0;

        while input_pos < effective_stdin_chars.len() {
            let ch = effective_stdin_chars[input_pos];

            // Check for delimiter
            if effective_delimiter.starts_with(ch) {
                consumed = input_pos + effective_delimiter.len();
                found_delimiter = true;
                break;
            }

            // In non-raw mode, handle backslash escapes
            if !raw && ch == '\\' && input_pos + 1 < effective_stdin_chars.len() {
                let next_char = effective_stdin_chars[input_pos + 1];

                if next_char == '\n' {
                    // Backslash-newline is line continuation: skip both
                    input_pos += 2;
                    continue;
                }

                if effective_delimiter.starts_with(next_char) {
                    // Backslash-delimiter: escape the delimiter, include it literally
                    line.push(next_char);
                    input_pos += 2;
                    continue;
                }

                // Other backslash escapes: keep both for now
                line.push(ch);
                line.push(next_char);
                input_pos += 2;
                continue;
            }

            line.push(ch);
            input_pos += 1;
        }

        if input_pos >= effective_stdin_chars.len() {
            found_delimiter = false;
            consumed = input_pos;
            if line.is_empty() && effective_stdin.is_empty() {
                for name in &var_names {
                    state.env.insert(name.clone(), String::new());
                }
                if let Some(ref arr_name) = array_name {
                    clear_array(&mut state.env, arr_name);
                }
                return BuiltinResult {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: 1,
                };
            }
        }

        consume_input(state, consumed, &effective_stdin);
    }

    // Remove trailing newline if present and delimiter is newline
    if effective_delimiter == "\n" && line.ends_with('\n') {
        line.pop();
    }

    // Helper to process backslash escapes (remove backslashes, keep escaped chars)
    let process_backslash_escapes = |s: &str| -> String {
        if raw {
            return s.to_string();
        }
        let mut result = String::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\\' && i + 1 < chars.len() {
                result.push(chars[i + 1]);
                i += 2;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }
        result
    };

    // If no variable names given (only REPLY), store whole line without IFS splitting
    if var_names.len() == 1 && var_names[0] == "REPLY" {
        state.env.insert("REPLY".to_string(), process_backslash_escapes(&line));
        return BuiltinResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: if found_delimiter { 0 } else { 1 },
        };
    }

    // Split by IFS (default is space, tab, newline)
    let ifs = get_ifs(&state.env).to_string();

    // Handle array assignment (-a)
    if let Some(ref arr_name) = array_name {
        let result = split_by_ifs_for_read(&line, &ifs, None, raw);
        clear_array(&mut state.env, arr_name);
        for (j, word) in result.words.iter().enumerate() {
            let key = format!("{}_{}", arr_name, j);
            state.env.insert(key, process_backslash_escapes(word));
        }
        return BuiltinResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: if found_delimiter { 0 } else { 1 },
        };
    }

    // Use the advanced IFS splitting for read with proper whitespace/non-whitespace handling
    let max_split = var_names.len();
    let split_result = split_by_ifs_for_read(&line, &ifs, Some(max_split), raw);

    // Assign words to variables
    for (j, name) in var_names.iter().enumerate() {
        if j < var_names.len() - 1 {
            // Assign single word, processing backslash escapes
            let word = split_result.words.get(j).map(|s| s.as_str()).unwrap_or("");
            state.env.insert(name.clone(), process_backslash_escapes(word));
        } else {
            // Last variable gets all remaining content from original line
            if j < split_result.word_starts.len() {
                let start = split_result.word_starts[j];
                let mut value = line.chars().skip(start).collect::<String>();
                value = strip_trailing_ifs_whitespace(&value, &ifs, raw);
                value = process_backslash_escapes(&value);
                state.env.insert(name.clone(), value);
            } else {
                state.env.insert(name.clone(), String::new());
            }
        }
    }

    BuiltinResult {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: if found_delimiter { 0 } else { 1 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> InterpreterState {
        InterpreterState::default()
    }

    #[test]
    fn test_read_basic() {
        let mut state = make_state();
        let result = handle_read(&mut state, &[], "hello\n", -1);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("REPLY"), Some(&"hello".to_string()));
    }

    #[test]
    fn test_read_multiple_vars() {
        let mut state = make_state();
        let result = handle_read(
            &mut state,
            &["a".to_string(), "b".to_string(), "c".to_string()],
            "one two three four\n",
            -1,
        );
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("a"), Some(&"one".to_string()));
        assert_eq!(state.env.get("b"), Some(&"two".to_string()));
        assert_eq!(state.env.get("c"), Some(&"three four".to_string()));
    }

    #[test]
    fn test_read_no_newline() {
        let mut state = make_state();
        let result = handle_read(&mut state, &[], "hello", -1);
        assert_eq!(result.exit_code, 1); // No delimiter found
        assert_eq!(state.env.get("REPLY"), Some(&"hello".to_string()));
    }

    #[test]
    fn test_read_empty_input() {
        let mut state = make_state();
        let result = handle_read(&mut state, &[], "", -1);
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_read_raw_mode() {
        let mut state = make_state();
        let result = handle_read(&mut state, &["-r".to_string()], "hello\\nworld\n", -1);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("REPLY"), Some(&"hello\\nworld".to_string()));
    }

    #[test]
    fn test_read_nchars() {
        let mut state = make_state();
        let result = handle_read(&mut state, &["-n".to_string(), "5".to_string()], "hello world\n", -1);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("REPLY"), Some(&"hello".to_string()));
    }

    #[test]
    fn test_read_nchars_exact() {
        let mut state = make_state();
        let result = handle_read(&mut state, &["-N".to_string(), "5".to_string()], "hello world\n", -1);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("REPLY"), Some(&"hello".to_string()));
    }

    #[test]
    fn test_read_custom_delimiter() {
        let mut state = make_state();
        let result = handle_read(&mut state, &["-d".to_string(), ":".to_string()], "hello:world", -1);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("REPLY"), Some(&"hello".to_string()));
    }

    #[test]
    fn test_read_array() {
        let mut state = make_state();
        let result = handle_read(&mut state, &["-a".to_string(), "arr".to_string()], "one two three\n", -1);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("arr_0"), Some(&"one".to_string()));
        assert_eq!(state.env.get("arr_1"), Some(&"two".to_string()));
        assert_eq!(state.env.get("arr_2"), Some(&"three".to_string()));
    }

    #[test]
    fn test_read_timeout_zero() {
        let mut state = make_state();
        let result = handle_read(&mut state, &["-t".to_string(), "0".to_string()], "hello\n", -1);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("REPLY"), Some(&"".to_string()));
    }
}
