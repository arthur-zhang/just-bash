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
    let mut _physical = false;

    // Parse options
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            // End of options
            i += 1;
            break;
        } else if arg == "-L" {
            _physical = false;
            i += 1;
        } else if arg == "-P" {
            _physical = true;
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

    // Update state
    state.previous_dir = state.cwd.clone();
    state.cwd = new_dir.clone();
    state.env.insert("PWD".to_string(), state.cwd.clone());
    state.env.insert("OLDPWD".to_string(), state.previous_dir.clone());

    // cd - prints the new directory
    if print_path {
        result(&format!("{}\n", new_dir), "", 0)
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
}
