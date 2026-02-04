//! mapfile/readarray - Read lines from stdin into an array
//!
//! Usage: mapfile [-d delim] [-n count] [-O origin] [-s count] [-t] [array]
//!        readarray [-d delim] [-n count] [-O origin] [-s count] [-t] [array]
//!
//! Options:
//!   -d delim   Use delim as line delimiter (default: newline)
//!   -n count   Read at most count lines (0 = all)
//!   -O origin  Start assigning at index origin (default: 0)
//!   -s count   Skip first count lines
//!   -t         Remove trailing delimiter from each line
//!   array      Array name (default: MAPFILE)

use crate::interpreter::builtins::BuiltinResult;
use crate::interpreter::helpers::clear_array;
use crate::interpreter::types::InterpreterState;

pub fn handle_mapfile(state: &mut InterpreterState, args: &[String], stdin: &str) -> BuiltinResult {
    // Parse options
    let mut delimiter = "\n".to_string();
    let mut max_count: usize = 0; // 0 = unlimited
    let mut origin: usize = 0;
    let mut skip_count: usize = 0;
    let mut trim_delimiter = false;
    let mut array_name = "MAPFILE".to_string();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-d" && i + 1 < args.len() {
            // In bash, -d '' means use NUL byte as delimiter
            let next = &args[i + 1];
            delimiter = if next.is_empty() {
                "\0".to_string()
            } else {
                next.clone()
            };
            i += 2;
        } else if arg == "-n" && i + 1 < args.len() {
            max_count = args[i + 1].parse().unwrap_or(0);
            i += 2;
        } else if arg == "-O" && i + 1 < args.len() {
            origin = args[i + 1].parse().unwrap_or(0);
            i += 2;
        } else if arg == "-s" && i + 1 < args.len() {
            skip_count = args[i + 1].parse().unwrap_or(0);
            i += 2;
        } else if arg == "-t" {
            trim_delimiter = true;
            i += 1;
        } else if arg == "-u" || arg == "-C" || arg == "-c" {
            // Skip unsupported options that take arguments
            i += 2;
        } else if !arg.starts_with('-') {
            array_name = arg.clone();
            i += 1;
        } else {
            // Unknown option, skip
            i += 1;
        }
    }

    // Use stdin from parameter, or fall back to group_stdin
    let effective_stdin = if stdin.is_empty() {
        state.group_stdin.as_deref().unwrap_or("")
    } else {
        stdin
    };

    // Split input by delimiter
    let mut lines: Vec<String> = Vec::new();
    let mut remaining = effective_stdin.to_string();
    let mut line_count: usize = 0;
    let mut skipped: usize = 0;

    while !remaining.is_empty() {
        let delim_index = remaining.find(&delimiter);

        if delim_index.is_none() {
            // No more delimiters, add remaining content as last line (if not empty)
            if !remaining.is_empty() {
                if skipped < skip_count {
                    skipped += 1;
                } else if max_count == 0 || line_count < max_count {
                    // Bash truncates at NUL bytes
                    let mut last_line = remaining.clone();
                    if let Some(nul_idx) = last_line.find('\0') {
                        last_line = last_line[..nul_idx].to_string();
                    }
                    lines.push(last_line);
                    line_count += 1;
                }
            }
            break;
        }

        let delim_idx = delim_index.unwrap();

        // Found delimiter
        let mut line = remaining[..delim_idx].to_string();
        // Bash truncates lines at NUL bytes (unlike 'read' which ignores them)
        if let Some(nul_index) = line.find('\0') {
            line = line[..nul_index].to_string();
        }
        // For other delimiters, include unless -t flag is set
        if !trim_delimiter && delimiter != "\0" {
            line.push_str(&delimiter);
        }

        remaining = remaining[delim_idx + delimiter.len()..].to_string();

        if skipped < skip_count {
            skipped += 1;
            continue;
        }

        if max_count > 0 && line_count >= max_count {
            break;
        }

        lines.push(line);
        line_count += 1;
    }

    // Clear existing array ONLY if not using -O (offset) option
    // When using -O, we want to preserve existing elements and append starting at origin
    if origin == 0 {
        clear_array(&mut state.env, &array_name);
    }

    for (j, line) in lines.iter().enumerate() {
        let key = format!("{}_{}", array_name, origin + j);
        state.env.insert(key, line.clone());
    }

    // Set array length metadata to be the max of existing length and new end position
    let length_key = format!("{}__length", array_name);
    let existing_length: usize = state
        .env
        .get(&length_key)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let new_end_index = origin + lines.len();
    state
        .env
        .insert(length_key, new_end_index.max(existing_length).to_string());

    // Consume from group_stdin if we used it
    if state.group_stdin.is_some() && stdin.is_empty() {
        state.group_stdin = Some(String::new());
    }

    BuiltinResult {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> InterpreterState {
        InterpreterState::default()
    }

    #[test]
    fn test_mapfile_basic() {
        let mut state = make_state();
        let result = handle_mapfile(&mut state, &[], "line1\nline2\nline3\n");
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("MAPFILE_0"), Some(&"line1\n".to_string()));
        assert_eq!(state.env.get("MAPFILE_1"), Some(&"line2\n".to_string()));
        assert_eq!(state.env.get("MAPFILE_2"), Some(&"line3\n".to_string()));
    }

    #[test]
    fn test_mapfile_trim() {
        let mut state = make_state();
        let result = handle_mapfile(&mut state, &["-t".to_string()], "line1\nline2\n");
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("MAPFILE_0"), Some(&"line1".to_string()));
        assert_eq!(state.env.get("MAPFILE_1"), Some(&"line2".to_string()));
    }

    #[test]
    fn test_mapfile_custom_array() {
        let mut state = make_state();
        let result = handle_mapfile(&mut state, &["myarray".to_string()], "a\nb\n");
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("myarray_0"), Some(&"a\n".to_string()));
        assert_eq!(state.env.get("myarray_1"), Some(&"b\n".to_string()));
    }

    #[test]
    fn test_mapfile_skip() {
        let mut state = make_state();
        let result = handle_mapfile(
            &mut state,
            &["-s".to_string(), "1".to_string()],
            "skip\nkeep1\nkeep2\n",
        );
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("MAPFILE_0"), Some(&"keep1\n".to_string()));
        assert_eq!(state.env.get("MAPFILE_1"), Some(&"keep2\n".to_string()));
    }

    #[test]
    fn test_mapfile_max_count() {
        let mut state = make_state();
        let result = handle_mapfile(
            &mut state,
            &["-n".to_string(), "2".to_string()],
            "a\nb\nc\nd\n",
        );
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("MAPFILE_0"), Some(&"a\n".to_string()));
        assert_eq!(state.env.get("MAPFILE_1"), Some(&"b\n".to_string()));
        assert_eq!(state.env.get("MAPFILE_2"), None);
    }

    #[test]
    fn test_mapfile_origin() {
        let mut state = make_state();
        // First populate some elements
        state.env.insert("arr_0".to_string(), "existing".to_string());
        let result = handle_mapfile(
            &mut state,
            &["-O".to_string(), "2".to_string(), "arr".to_string()],
            "new1\nnew2\n",
        );
        assert_eq!(result.exit_code, 0);
        // Original element should be preserved
        assert_eq!(state.env.get("arr_0"), Some(&"existing".to_string()));
        // New elements start at index 2
        assert_eq!(state.env.get("arr_2"), Some(&"new1\n".to_string()));
        assert_eq!(state.env.get("arr_3"), Some(&"new2\n".to_string()));
    }

    #[test]
    fn test_mapfile_custom_delimiter() {
        let mut state = make_state();
        let result = handle_mapfile(
            &mut state,
            &["-d".to_string(), ":".to_string(), "-t".to_string()],
            "a:b:c:",
        );
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("MAPFILE_0"), Some(&"a".to_string()));
        assert_eq!(state.env.get("MAPFILE_1"), Some(&"b".to_string()));
        assert_eq!(state.env.get("MAPFILE_2"), Some(&"c".to_string()));
    }
}
