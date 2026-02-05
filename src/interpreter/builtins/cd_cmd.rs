//! cd - Change directory builtin
//!
//! Supports:
//! - cd [dir] - change to directory
//! - cd - - change to previous directory (OLDPWD)
//! - cd ~ - change to home directory
//! - cd -L - use logical path (default)
//! - cd -P - use physical path (resolve symlinks)
//! - CDPATH support for relative paths

use crate::interpreter::types::{ExecResult, InterpreterState};
use crate::interpreter::helpers::result::{result, failure};

/// Handle the cd builtin command
pub fn handle_cd(
    state: &mut InterpreterState,
    args: &[String],
) -> ExecResult {
    let mut target: String;
    let mut print_path = false;
    let mut physical = false;

    // Parse options
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            // End of options
            i += 1;
            break;
        } else if arg == "-L" {
            physical = false;
            i += 1;
        } else if arg == "-P" {
            physical = true;
            i += 1;
        } else if arg.starts_with('-') && arg != "-" {
            // Unknown option - ignore for now
            i += 1;
        } else {
            break;
        }
    }

    // Get the target directory
    let remaining_args: Vec<&String> = args[i..].iter().collect();
    if remaining_args.is_empty() {
        target = state.env.get("HOME").cloned().unwrap_or_else(|| "/".to_string());
    } else if remaining_args[0] == "~" {
        target = state.env.get("HOME").cloned().unwrap_or_else(|| "/".to_string());
    } else if remaining_args[0] == "-" {
        target = state.previous_dir.clone();
        print_path = true; // cd - prints the new directory
    } else {
        target = remaining_args[0].clone();
    }

    // CDPATH support: if target doesn't start with / or ., search CDPATH directories
    // CDPATH is only used for relative paths that don't start with .
    if !target.starts_with('/')
        && !target.starts_with("./")
        && !target.starts_with("../")
        && target != "."
        && target != ".."
    {
        if let Some(cdpath) = state.env.get("CDPATH") {
            let cdpath_dirs: Vec<&str> = cdpath.split(':').filter(|d| !d.is_empty()).collect();
            for dir in cdpath_dirs {
                let candidate = if dir.starts_with('/') {
                    format!("{}/{}", dir, target)
                } else {
                    format!("{}/{}/{}", state.cwd, dir, target)
                };
                // In a real implementation, we would check if the directory exists
                // For now, we just use the first CDPATH entry
                if std::path::Path::new(&candidate).is_dir() {
                    target = candidate;
                    print_path = true;
                    break;
                }
            }
        }
    }

    // Normalize the path
    let path_to_check = if target.starts_with('/') {
        target.clone()
    } else {
        format!("{}/{}", state.cwd, target)
    };

    let new_dir = normalize_path(&path_to_check);

    // Check if the directory exists
    let path = std::path::Path::new(&new_dir);
    if !path.exists() {
        return failure(&format!("bash: cd: {}: No such file or directory\n", target));
    }
    if !path.is_dir() {
        return failure(&format!("bash: cd: {}: Not a directory\n", target));
    }

    // If -P is specified, resolve symlinks to get the physical path
    let final_dir = if physical {
        match std::fs::canonicalize(&new_dir) {
            Ok(canonical) => canonical.to_string_lossy().to_string(),
            Err(_) => new_dir.clone(), // If canonicalize fails, use the logical path
        }
    } else {
        new_dir.clone()
    };

    // Update state
    state.previous_dir = state.cwd.clone();
    state.cwd = final_dir.clone();
    state.env.insert("PWD".to_string(), state.cwd.clone());
    state.env.insert("OLDPWD".to_string(), state.previous_dir.clone());

    // cd - prints the new directory
    if print_path {
        result(&format!("{}\n", final_dir), "", 0)
    } else {
        result("", "", 0)
    }
}

/// Normalize a path by resolving . and .. components
fn normalize_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    let mut result_parts: Vec<&str> = Vec::new();

    for part in parts {
        if part == ".." {
            result_parts.pop();
        } else {
            result_parts.push(part);
        }
    }

    if result_parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", result_parts.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path("/foo/bar"), "/foo/bar");
        assert_eq!(normalize_path("/foo/../bar"), "/bar");
        assert_eq!(normalize_path("/foo/./bar"), "/foo/bar");
        assert_eq!(normalize_path("/foo/bar/.."), "/foo");
        assert_eq!(normalize_path("/foo/bar/../.."), "/");
        assert_eq!(normalize_path("/foo//bar"), "/foo/bar");
    }

    #[test]
    fn test_handle_cd_to_tmp() {
        let mut state = InterpreterState::default();
        state.cwd = "/".to_string();
        state.env.insert("HOME".to_string(), "/tmp".to_string());

        // cd to /tmp
        let result = handle_cd(&mut state, &["tmp".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.cwd, "/tmp");
        assert_eq!(state.env.get("PWD"), Some(&"/tmp".to_string()));
    }

    #[test]
    fn test_handle_cd_home() {
        let mut state = InterpreterState::default();
        state.cwd = "/var".to_string();
        state.env.insert("HOME".to_string(), "/tmp".to_string());

        // cd with no args goes to HOME
        let result = handle_cd(&mut state, &[]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.cwd, "/tmp");
    }

    #[test]
    fn test_handle_cd_previous() {
        let mut state = InterpreterState::default();
        state.cwd = "/tmp".to_string();
        state.previous_dir = "/var".to_string();

        // cd - goes to previous directory
        let result = handle_cd(&mut state, &["-".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.cwd, "/var");
        // Should print the new directory
        assert!(result.stdout.contains("/var"));
    }

    #[test]
    fn test_handle_cd_physical_option() {
        let mut state = InterpreterState::default();
        state.cwd = "/".to_string();

        // cd -P to /tmp (should resolve symlinks)
        let result = handle_cd(&mut state, &["-P".to_string(), "tmp".to_string()]);
        assert_eq!(result.exit_code, 0);
        // The path should be resolved (on macOS /tmp is a symlink to /private/tmp)
        // We just check that it succeeded and the path is set
        assert!(!state.cwd.is_empty());
    }
}
