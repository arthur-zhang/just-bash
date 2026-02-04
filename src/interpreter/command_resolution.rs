//! Command Resolution
//!
//! Handles PATH-based command resolution and lookup for external commands.

use std::collections::HashMap;

/// Result type for command resolution
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveCommandResult {
    /// Found a registered command at the given path
    Command { path: String },
    /// Found an executable script at the given path
    Script { path: String },
    /// Command not found
    NotFound { path: Option<String> },
    /// Permission denied (file exists but not executable, or is a directory)
    PermissionDenied { path: String },
}

impl ResolveCommandResult {
    /// Check if the result is a successful resolution
    pub fn is_found(&self) -> bool {
        matches!(self, ResolveCommandResult::Command { .. } | ResolveCommandResult::Script { .. })
    }

    /// Get the path if resolution was successful
    pub fn path(&self) -> Option<&str> {
        match self {
            ResolveCommandResult::Command { path } => Some(path),
            ResolveCommandResult::Script { path } => Some(path),
            ResolveCommandResult::NotFound { path } => path.as_deref(),
            ResolveCommandResult::PermissionDenied { path } => Some(path),
        }
    }
}

/// Default PATH value when not set in environment
pub const DEFAULT_PATH: &str = "/usr/bin:/bin";

/// Split PATH into individual directories
pub fn split_path(path_env: &str) -> Vec<&str> {
    path_env.split(':').filter(|s| !s.is_empty()).collect()
}

/// Check if a command name contains a path separator (making it a path reference)
pub fn is_path_command(command_name: &str) -> bool {
    command_name.contains('/')
}

/// Get the command name from a path (the last component)
pub fn command_name_from_path(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Build a full path from a directory and command name
pub fn build_command_path(dir: &str, command_name: &str) -> String {
    if dir.ends_with('/') {
        format!("{}{}", dir, command_name)
    } else {
        format!("{}/{}", dir, command_name)
    }
}

/// Check if a directory is a system directory where command stubs live
pub fn is_system_directory(dir: &str) -> bool {
    dir == "/bin" || dir == "/usr/bin"
}

/// Check if a file mode indicates the file is executable
pub fn is_executable_mode(mode: u32) -> bool {
    (mode & 0o111) != 0
}

/// Hash table operations for command path caching
pub struct CommandHashTable {
    table: HashMap<String, String>,
}

impl CommandHashTable {
    /// Create a new empty hash table
    pub fn new() -> Self {
        Self { table: HashMap::new() }
    }

    /// Get the cached path for a command
    pub fn get(&self, command: &str) -> Option<&str> {
        self.table.get(command).map(|s| s.as_str())
    }

    /// Cache a command path
    pub fn insert(&mut self, command: &str, path: &str) {
        self.table.insert(command.to_string(), path.to_string());
    }

    /// Remove a command from the cache
    pub fn remove(&mut self, command: &str) -> bool {
        self.table.remove(command).is_some()
    }

    /// Clear all cached entries
    pub fn clear(&mut self) {
        self.table.clear();
    }

    /// Get all cached entries
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.table.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Get the number of cached entries
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

impl Default for CommandHashTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Command type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandType {
    /// Shell builtin command
    Builtin,
    /// Shell function
    Function,
    /// Alias
    Alias,
    /// Shell keyword (if, while, etc.)
    Keyword,
    /// External command found via PATH
    File,
    /// Not found
    NotFound,
}

impl CommandType {
    /// Get the string representation for `type` builtin output
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandType::Builtin => "builtin",
            CommandType::Function => "function",
            CommandType::Alias => "alias",
            CommandType::Keyword => "keyword",
            CommandType::File => "file",
            CommandType::NotFound => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_path() {
        let dirs = split_path("/usr/bin:/bin:/usr/local/bin");
        assert_eq!(dirs, vec!["/usr/bin", "/bin", "/usr/local/bin"]);
    }

    #[test]
    fn test_split_path_empty_entries() {
        let dirs = split_path("/usr/bin::/bin:");
        assert_eq!(dirs, vec!["/usr/bin", "/bin"]);
    }

    #[test]
    fn test_is_path_command() {
        assert!(is_path_command("/bin/ls"));
        assert!(is_path_command("./script.sh"));
        assert!(is_path_command("../bin/cmd"));
        assert!(!is_path_command("ls"));
        assert!(!is_path_command("echo"));
    }

    #[test]
    fn test_command_name_from_path() {
        assert_eq!(command_name_from_path("/bin/ls"), "ls");
        assert_eq!(command_name_from_path("./script.sh"), "script.sh");
        assert_eq!(command_name_from_path("echo"), "echo");
    }

    #[test]
    fn test_build_command_path() {
        assert_eq!(build_command_path("/usr/bin", "ls"), "/usr/bin/ls");
        assert_eq!(build_command_path("/usr/bin/", "ls"), "/usr/bin/ls");
    }

    #[test]
    fn test_is_system_directory() {
        assert!(is_system_directory("/bin"));
        assert!(is_system_directory("/usr/bin"));
        assert!(!is_system_directory("/usr/local/bin"));
        assert!(!is_system_directory("/home/user/bin"));
    }

    #[test]
    fn test_is_executable_mode() {
        assert!(is_executable_mode(0o755)); // rwxr-xr-x
        assert!(is_executable_mode(0o100)); // --x------
        assert!(is_executable_mode(0o010)); // -----x---
        assert!(is_executable_mode(0o001)); // --------x
        assert!(!is_executable_mode(0o644)); // rw-r--r--
        assert!(!is_executable_mode(0o000)); // ---------
    }

    #[test]
    fn test_command_hash_table() {
        let mut table = CommandHashTable::new();
        assert!(table.is_empty());

        table.insert("ls", "/bin/ls");
        table.insert("cat", "/bin/cat");

        assert_eq!(table.len(), 2);
        assert_eq!(table.get("ls"), Some("/bin/ls"));
        assert_eq!(table.get("cat"), Some("/bin/cat"));
        assert_eq!(table.get("echo"), None);

        assert!(table.remove("ls"));
        assert!(!table.remove("ls")); // Already removed
        assert_eq!(table.len(), 1);

        table.clear();
        assert!(table.is_empty());
    }

    #[test]
    fn test_resolve_command_result() {
        let found = ResolveCommandResult::Command { path: "/bin/ls".to_string() };
        assert!(found.is_found());
        assert_eq!(found.path(), Some("/bin/ls"));

        let script = ResolveCommandResult::Script { path: "./script.sh".to_string() };
        assert!(script.is_found());
        assert_eq!(script.path(), Some("./script.sh"));

        let not_found = ResolveCommandResult::NotFound { path: None };
        assert!(!not_found.is_found());
        assert_eq!(not_found.path(), None);

        let denied = ResolveCommandResult::PermissionDenied { path: "/bin/ls".to_string() };
        assert!(!denied.is_found());
        assert_eq!(denied.path(), Some("/bin/ls"));
    }

    #[test]
    fn test_command_type() {
        assert_eq!(CommandType::Builtin.as_str(), "builtin");
        assert_eq!(CommandType::Function.as_str(), "function");
        assert_eq!(CommandType::Alias.as_str(), "alias");
        assert_eq!(CommandType::Keyword.as_str(), "keyword");
        assert_eq!(CommandType::File.as_str(), "file");
        assert_eq!(CommandType::NotFound.as_str(), "");
    }
}
