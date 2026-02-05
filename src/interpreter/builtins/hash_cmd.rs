//! hash - Manage the hash table of remembered command locations
//!
//! hash [-lr] [-p pathname] [-dt] [name ...]
//!
//! Hash maintains a hash table of recently executed commands for faster lookup.
//!
//! Options:
//!   (no args)  Display the hash table
//!   name       Add name to the hash table (look up in PATH)
//!   -r         Clear the hash table
//!   -d name    Remove name from the hash table
//!   -l         Display in a format that can be reused as input
//!   -p path    Use path as the full pathname for name (hash -p /path name)
//!   -t name    Print the remembered location of name

use std::collections::HashMap;
use crate::interpreter::types::InterpreterState;

/// Result type for builtin commands
pub type BuiltinResult = (String, String, i32);

/// Get the hash table, initializing if needed
fn get_hash_table(state: &mut InterpreterState) -> &mut HashMap<String, String> {
    if state.hash_table.is_none() {
        state.hash_table = Some(HashMap::new());
    }
    state.hash_table.as_mut().unwrap()
}

/// Handle the `hash` builtin command.
///
/// Note: The -p option with name lookup requires filesystem access.
/// This implementation handles all options except PATH lookup for adding names.
pub fn handle_hash(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    // Parse options
    let mut clear_table = false;
    let mut delete_mode = false;
    let mut list_mode = false;
    let mut path_mode = false;
    let mut show_path = false;
    let mut pathname = String::new();
    let mut names: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            i += 1;
            // Remaining args are names
            names.extend(args[i..].iter().cloned());
            break;
        }
        if arg == "-r" {
            clear_table = true;
            i += 1;
        } else if arg == "-d" {
            delete_mode = true;
            i += 1;
        } else if arg == "-l" {
            list_mode = true;
            i += 1;
        } else if arg == "-t" {
            show_path = true;
            i += 1;
        } else if arg == "-p" {
            path_mode = true;
            i += 1;
            if i >= args.len() {
                return (String::new(), "bash: hash: -p: option requires an argument\n".to_string(), 1);
            }
            pathname = args[i].clone();
            i += 1;
        } else if arg.starts_with('-') && arg.len() > 1 {
            // Handle combined options like -rt
            for ch in arg[1..].chars() {
                match ch {
                    'r' => clear_table = true,
                    'd' => delete_mode = true,
                    'l' => list_mode = true,
                    't' => show_path = true,
                    'p' => {
                        return (String::new(), "bash: hash: -p: option requires an argument\n".to_string(), 1);
                    }
                    _ => {
                        return (String::new(), format!("bash: hash: -{}: invalid option\n", ch), 1);
                    }
                }
            }
            i += 1;
        } else {
            names.push(arg.clone());
            i += 1;
        }
    }

    // Handle -r (clear table)
    if clear_table {
        // bash allows extra args with -r (just ignores them)
        let hash_table = get_hash_table(state);
        hash_table.clear();
        return (String::new(), String::new(), 0);
    }

    // Handle -d (delete from table)
    if delete_mode {
        if names.is_empty() {
            return (String::new(), "bash: hash: -d: option requires an argument\n".to_string(), 1);
        }
        let mut has_error = false;
        let mut stderr = String::new();
        let hash_table = get_hash_table(state);
        for name in &names {
            if !hash_table.contains_key(name) {
                stderr.push_str(&format!("bash: hash: {}: not found\n", name));
                has_error = true;
            } else {
                hash_table.remove(name);
            }
        }
        if has_error {
            return (String::new(), stderr, 1);
        }
        return (String::new(), String::new(), 0);
    }

    // Handle -t (show path for names)
    if show_path {
        if names.is_empty() {
            return (String::new(), "bash: hash: -t: option requires an argument\n".to_string(), 1);
        }
        let mut stdout = String::new();
        let mut has_error = false;
        let mut stderr = String::new();
        let hash_table = get_hash_table(state);
        for name in &names {
            if let Some(cached_path) = hash_table.get(name) {
                // If multiple names, show "name\tpath" format
                if names.len() > 1 {
                    stdout.push_str(&format!("{}\t{}\n", name, cached_path));
                } else {
                    stdout.push_str(&format!("{}\n", cached_path));
                }
            } else {
                stderr.push_str(&format!("bash: hash: {}: not found\n", name));
                has_error = true;
            }
        }
        if has_error {
            return (stdout, stderr, 1);
        }
        return (stdout, String::new(), 0);
    }

    // Handle -p (associate pathname with name)
    if path_mode {
        if names.is_empty() {
            return (String::new(), "bash: hash: usage: hash [-lr] [-p pathname] [-dt] [name ...]\n".to_string(), 1);
        }
        // Associate the pathname with the first name
        let name = &names[0];
        let hash_table = get_hash_table(state);
        hash_table.insert(name.clone(), pathname);
        return (String::new(), String::new(), 0);
    }

    // No args - display hash table
    if names.is_empty() {
        let hash_table = get_hash_table(state);
        if hash_table.is_empty() {
            return ("hash: hash table empty\n".to_string(), String::new(), 0);
        }

        let mut stdout = String::new();
        if list_mode {
            // Reusable format: builtin hash -p /path/to/cmd cmd
            for (name, path) in hash_table.iter() {
                stdout.push_str(&format!("builtin hash -p {} {}\n", path, name));
            }
        } else {
            // Default format (bash style: hits command table)
            stdout.push_str("hits\tcommand\n");
            for (_, path) in hash_table.iter() {
                // We don't track hits, so just show 1
                stdout.push_str(&format!("   1\t{}\n", path));
            }
        }
        return (stdout, String::new(), 0);
    }

    // Add names to hash table (look up in PATH)
    // Note: This requires filesystem access to check if commands exist
    // For now, we return an error for names that contain /
    let mut has_error = false;
    let mut stderr = String::new();

    for name in &names {
        // Skip if name contains / (it's a path, not looked up in PATH)
        if name.contains('/') {
            stderr.push_str(&format!("bash: hash: {}: cannot use / in name\n", name));
            has_error = true;
            continue;
        }

        // Note: Actual PATH lookup requires filesystem access
        // The runtime should handle this by calling handle_hash_add_from_path
        stderr.push_str(&format!("bash: hash: {}: not found\n", name));
        has_error = true;
    }

    if has_error {
        return (String::new(), stderr, 1);
    }
    (String::new(), String::new(), 0)
}

/// Add a command to the hash table with a specific path.
/// This is called by the runtime after verifying the path exists.
pub fn hash_add(state: &mut InterpreterState, name: &str, path: &str) {
    let hash_table = get_hash_table(state);
    hash_table.insert(name.to_string(), path.to_string());
}

/// Look up a command in the hash table.
pub fn hash_lookup<'a>(state: &'a InterpreterState, name: &str) -> Option<&'a String> {
    state.hash_table.as_ref().and_then(|table| table.get(name))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_hash_empty_table() {
        let mut state = InterpreterState::default();
        let (stdout, stderr, code) = handle_hash(&mut state, &[]);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("hash table empty"));
    }

    #[test]
    fn test_handle_hash_clear() {
        let mut state = InterpreterState::default();
        state.hash_table = Some(HashMap::from([
            ("ls".to_string(), "/bin/ls".to_string()),
            ("cat".to_string(), "/bin/cat".to_string()),
        ]));
        let (stdout, stderr, code) = handle_hash(&mut state, &["-r".to_string()]);
        assert_eq!(code, 0);
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
        assert!(state.hash_table.unwrap().is_empty());
    }

    #[test]
    fn test_handle_hash_add_with_path() {
        let mut state = InterpreterState::default();
        let args = vec!["-p".to_string(), "/usr/bin/ls".to_string(), "ls".to_string()];
        let (stdout, stderr, code) = handle_hash(&mut state, &args);
        assert_eq!(code, 0);
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
        assert_eq!(state.hash_table.unwrap().get("ls"), Some(&"/usr/bin/ls".to_string()));
    }

    #[test]
    fn test_handle_hash_delete() {
        let mut state = InterpreterState::default();
        state.hash_table = Some(HashMap::from([
            ("ls".to_string(), "/bin/ls".to_string()),
        ]));
        let args = vec!["-d".to_string(), "ls".to_string()];
        let (stdout, stderr, code) = handle_hash(&mut state, &args);
        assert_eq!(code, 0);
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
        assert!(!state.hash_table.unwrap().contains_key("ls"));
    }

    #[test]
    fn test_handle_hash_delete_not_found() {
        let mut state = InterpreterState::default();
        let args = vec!["-d".to_string(), "nonexistent".to_string()];
        let (_, stderr, code) = handle_hash(&mut state, &args);
        assert_eq!(code, 1);
        assert!(stderr.contains("not found"));
    }

    #[test]
    fn test_handle_hash_show_path() {
        let mut state = InterpreterState::default();
        state.hash_table = Some(HashMap::from([
            ("ls".to_string(), "/bin/ls".to_string()),
        ]));
        let args = vec!["-t".to_string(), "ls".to_string()];
        let (stdout, stderr, code) = handle_hash(&mut state, &args);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert_eq!(stdout, "/bin/ls\n");
    }

    #[test]
    fn test_handle_hash_show_path_multiple() {
        let mut state = InterpreterState::default();
        state.hash_table = Some(HashMap::from([
            ("ls".to_string(), "/bin/ls".to_string()),
            ("cat".to_string(), "/bin/cat".to_string()),
        ]));
        let args = vec!["-t".to_string(), "ls".to_string(), "cat".to_string()];
        let (stdout, stderr, code) = handle_hash(&mut state, &args);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("ls\t/bin/ls"));
        assert!(stdout.contains("cat\t/bin/cat"));
    }

    #[test]
    fn test_handle_hash_list_mode() {
        let mut state = InterpreterState::default();
        state.hash_table = Some(HashMap::from([
            ("ls".to_string(), "/bin/ls".to_string()),
        ]));
        let args = vec!["-l".to_string()];
        let (stdout, stderr, code) = handle_hash(&mut state, &args);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("builtin hash -p /bin/ls ls"));
    }

    #[test]
    fn test_handle_hash_display() {
        let mut state = InterpreterState::default();
        state.hash_table = Some(HashMap::from([
            ("ls".to_string(), "/bin/ls".to_string()),
        ]));
        let (stdout, stderr, code) = handle_hash(&mut state, &[]);
        assert_eq!(code, 0);
        assert!(stderr.is_empty());
        assert!(stdout.contains("hits\tcommand"));
        assert!(stdout.contains("/bin/ls"));
    }

    #[test]
    fn test_handle_hash_slash_in_name() {
        let mut state = InterpreterState::default();
        let args = vec!["./script".to_string()];
        let (_, stderr, code) = handle_hash(&mut state, &args);
        assert_eq!(code, 1);
        assert!(stderr.contains("cannot use / in name"));
    }

    #[test]
    fn test_hash_add_and_lookup() {
        let mut state = InterpreterState::default();
        hash_add(&mut state, "ls", "/bin/ls");
        assert_eq!(hash_lookup(&state, "ls"), Some(&"/bin/ls".to_string()));
        assert_eq!(hash_lookup(&state, "nonexistent"), None);
    }
}
