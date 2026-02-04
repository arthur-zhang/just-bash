//! Shell Constants
//!
//! Constants for shell builtins, keywords, and POSIX special builtins.

use std::collections::HashSet;
use std::sync::LazyLock;

/// POSIX special built-in commands.
/// In POSIX mode, these have special behaviors:
/// - Prefix assignments persist after the command
/// - Cannot be redefined as functions
/// - Errors may be fatal
pub static POSIX_SPECIAL_BUILTINS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        ":",
        ".",
        "break",
        "continue",
        "eval",
        "exec",
        "exit",
        "export",
        "readonly",
        "return",
        "set",
        "shift",
        "trap",
        "unset",
    ])
});

/// Check if a command name is a POSIX special built-in
pub fn is_posix_special_builtin(name: &str) -> bool {
    POSIX_SPECIAL_BUILTINS.contains(name)
}

/// Shell keywords (for type, command -v, etc.)
pub static SHELL_KEYWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "if",
        "then",
        "else",
        "elif",
        "fi",
        "case",
        "esac",
        "for",
        "select",
        "while",
        "until",
        "do",
        "done",
        "in",
        "function",
        "{",
        "}",
        "time",
        "[[",
        "]]",
        "!",
    ])
});

/// Check if a name is a shell keyword
pub fn is_shell_keyword(name: &str) -> bool {
    SHELL_KEYWORDS.contains(name)
}

/// Shell builtins (for type, command -v, builtin, etc.)
pub static SHELL_BUILTINS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        ":",
        "true",
        "false",
        "cd",
        "export",
        "unset",
        "exit",
        "local",
        "set",
        "break",
        "continue",
        "return",
        "eval",
        "shift",
        "getopts",
        "compgen",
        "complete",
        "compopt",
        "pushd",
        "popd",
        "dirs",
        "source",
        ".",
        "read",
        "mapfile",
        "readarray",
        "declare",
        "typeset",
        "readonly",
        "let",
        "command",
        "shopt",
        "exec",
        "test",
        "[",
        "echo",
        "printf",
        "pwd",
        "alias",
        "unalias",
        "type",
        "hash",
        "ulimit",
        "umask",
        "trap",
        "times",
        "wait",
        "kill",
        "jobs",
        "fg",
        "bg",
        "disown",
        "suspend",
        "fc",
        "history",
        "help",
        "enable",
        "builtin",
        "caller",
    ])
});

/// Check if a name is a shell builtin
pub fn is_shell_builtin(name: &str) -> bool {
    SHELL_BUILTINS.contains(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_posix_special_builtins() {
        assert!(is_posix_special_builtin(":"));
        assert!(is_posix_special_builtin("eval"));
        assert!(is_posix_special_builtin("export"));
        assert!(!is_posix_special_builtin("echo"));
        assert!(!is_posix_special_builtin("cd"));
    }

    #[test]
    fn test_shell_keywords() {
        assert!(is_shell_keyword("if"));
        assert!(is_shell_keyword("then"));
        assert!(is_shell_keyword("[["));
        assert!(!is_shell_keyword("echo"));
        assert!(!is_shell_keyword("cd"));
    }

    #[test]
    fn test_shell_builtins() {
        assert!(is_shell_builtin("echo"));
        assert!(is_shell_builtin("cd"));
        assert!(is_shell_builtin("export"));
        assert!(!is_shell_builtin("ls"));
        assert!(!is_shell_builtin("grep"));
    }
}
