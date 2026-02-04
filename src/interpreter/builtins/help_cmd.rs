//! help - Display helpful information about builtin commands
//!
//! Usage: help [-s] [pattern ...]
//!
//! If PATTERN is specified, gives detailed help on all commands matching PATTERN,
//! otherwise a list of the builtins is printed. The -s option restricts the output
//! for each builtin command matching PATTERN to a short usage synopsis.

use super::break_cmd::BuiltinResult;

/// Builtin help information: (synopsis, description)
struct BuiltinHelp {
    name: &'static str,
    synopsis: &'static str,
    description: &'static str,
}

/// All builtin help entries
const BUILTIN_HELP: &[BuiltinHelp] = &[
    BuiltinHelp {
        name: ":",
        synopsis: ": [arguments]",
        description: "Null command.\n    No effect; the command does nothing.\n    Exit Status:\n    Always succeeds.",
    },
    BuiltinHelp {
        name: ".",
        synopsis: ". filename [arguments]",
        description: "Execute commands from a file in the current shell.\n    Read and execute commands from FILENAME in the current shell.\n    The entries in $PATH are used to find the directory containing FILENAME.\n    Exit Status:\n    Returns the status of the last command executed in FILENAME.",
    },
    BuiltinHelp {
        name: "[",
        synopsis: "[ arg... ]",
        description: "Evaluate conditional expression.\n    This is a synonym for the \"test\" builtin, but the last argument must\n    be a literal `]', to match the opening `['.",
    },
    BuiltinHelp {
        name: "alias",
        synopsis: "alias [-p] [name[=value] ... ]",
        description: "Define or display aliases.\n    Without arguments, `alias' prints the list of aliases in the reusable\n    form `alias NAME=VALUE' on standard output.\n    Exit Status:\n    alias returns true unless a NAME is supplied for which no alias has been\n    defined.",
    },
    BuiltinHelp {
        name: "break",
        synopsis: "break [n]",
        description: "Exit for, while, or until loops.\n    Exit a FOR, WHILE or UNTIL loop.  If N is specified, break N enclosing\n    loops.\n    Exit Status:\n    The exit status is 0 unless N is not greater than or equal to 1.",
    },
    BuiltinHelp {
        name: "cd",
        synopsis: "cd [-L|-P] [dir]",
        description: "Change the shell working directory.\n    Change the current directory to DIR.  The default DIR is the value of the\n    HOME shell variable.\n    Exit Status:\n    Returns 0 if the directory is changed; non-zero otherwise.",
    },
    BuiltinHelp {
        name: "command",
        synopsis: "command [-pVv] command [arg ...]",
        description: "Execute a simple command or display information about commands.\n    Runs COMMAND with ARGS suppressing shell function lookup, or display\n    information about the specified COMMANDs.\n    Exit Status:\n    Returns exit status of COMMAND, or failure if COMMAND is not found.",
    },
    BuiltinHelp {
        name: "continue",
        synopsis: "continue [n]",
        description: "Resume for, while, or until loops.\n    Resumes the next iteration of the enclosing FOR, WHILE or UNTIL loop.\n    If N is specified, resumes the Nth enclosing loop.\n    Exit Status:\n    The exit status is 0 unless N is not greater than or equal to 1.",
    },
    BuiltinHelp {
        name: "declare",
        synopsis: "declare [-aAfFgilnrtux] [-p] [name[=value] ...]",
        description: "Set variable values and attributes.\n    Declare variables and give them attributes.  If no NAMEs are given,\n    display the attributes and values of all variables.\n    Exit Status:\n    Returns success unless an invalid option is supplied or a variable\n    assignment error occurs.",
    },
    BuiltinHelp {
        name: "echo",
        synopsis: "echo [-neE] [arg ...]",
        description: "Write arguments to the standard output.\n    Display the ARGs, separated by a single space character and followed by a\n    newline, on the standard output.\n    Exit Status:\n    Returns success unless a write error occurs.",
    },
    BuiltinHelp {
        name: "eval",
        synopsis: "eval [arg ...]",
        description: "Execute arguments as a shell command.\n    Combine ARGs into a single string, use the result as input to the shell,\n    and execute the resulting commands.\n    Exit Status:\n    Returns exit status of command or success if command is null.",
    },
    BuiltinHelp {
        name: "exit",
        synopsis: "exit [n]",
        description: "Exit the shell.\n    Exits the shell with a status of N.  If N is omitted, the exit status\n    is that of the last command executed.",
    },
    BuiltinHelp {
        name: "export",
        synopsis: "export [-fn] [name[=value] ...] or export -p",
        description: "Set export attribute for shell variables.\n    Marks each NAME for automatic export to the environment of subsequently\n    executed commands.  If VALUE is supplied, assign VALUE before exporting.\n    Exit Status:\n    Returns success unless an invalid option is given or NAME is invalid.",
    },
    BuiltinHelp {
        name: "false",
        synopsis: "false",
        description: "Return an unsuccessful result.\n    Exit Status:\n    Always fails.",
    },
    BuiltinHelp {
        name: "getopts",
        synopsis: "getopts optstring name [arg]",
        description: "Parse option arguments.\n    Getopts is used by shell procedures to parse positional parameters\n    as options.\n    Exit Status:\n    Returns success if an option is found; fails if the end of options is\n    encountered or an error occurs.",
    },
    BuiltinHelp {
        name: "hash",
        synopsis: "hash [-lr] [-p pathname] [-dt] [name ...]",
        description: "Remember or display program locations.\n    Determine and remember the full pathname of each command NAME.\n    Exit Status:\n    Returns success unless NAME is not found or an invalid option is given.",
    },
    BuiltinHelp {
        name: "help",
        synopsis: "help [-s] [pattern ...]",
        description: "Display information about builtin commands.\n    Displays brief summaries of builtin commands.  If PATTERN is\n    specified, gives detailed help on all commands matching PATTERN,\n    otherwise the list of help topics is printed.\n    Exit Status:\n    Returns success unless PATTERN is not found.",
    },
    BuiltinHelp {
        name: "let",
        synopsis: "let arg [arg ...]",
        description: "Evaluate arithmetic expressions.\n    Evaluate each ARG as an arithmetic expression.  Evaluation is done in\n    fixed-width integers with no check for overflow, though division by 0\n    is trapped and flagged as an error.\n    Exit Status:\n    If the last ARG evaluates to 0, let returns 1; 0 is returned otherwise.",
    },
    BuiltinHelp {
        name: "local",
        synopsis: "local [option] name[=value] ...",
        description: "Define local variables.\n    Create a local variable called NAME, and give it VALUE.  OPTION can\n    be any option accepted by `declare'.\n    Exit Status:\n    Returns success unless an invalid option is supplied, a variable\n    assignment error occurs, or the shell is not executing a function.",
    },
    BuiltinHelp {
        name: "mapfile",
        synopsis: "mapfile [-d delim] [-n count] [-O origin] [-s count] [-t] [-u fd] [-C callback] [-c quantum] [array]",
        description: "Read lines from the standard input into an indexed array variable.\n    Read lines from the standard input into the indexed array variable ARRAY,\n    or from file descriptor FD if the -u option is supplied.\n    Exit Status:\n    Returns success unless an invalid option is given or ARRAY is readonly.",
    },
    BuiltinHelp {
        name: "printf",
        synopsis: "printf [-v var] format [arguments]",
        description: "Formats and prints ARGUMENTS under control of the FORMAT.\n    Exit Status:\n    Returns success unless an invalid option is given or a write or assignment\n    error occurs.",
    },
    BuiltinHelp {
        name: "pwd",
        synopsis: "pwd [-LP]",
        description: "Print the name of the current working directory.\n    Exit Status:\n    Returns 0 unless an invalid option is given or the current directory\n    cannot be read.",
    },
    BuiltinHelp {
        name: "read",
        synopsis: "read [-ers] [-a array] [-d delim] [-i text] [-n nchars] [-N nchars] [-p prompt] [-t timeout] [-u fd] [name ...]",
        description: "Read a line from the standard input and split it into fields.\n    Reads a single line from the standard input, or from file descriptor FD\n    if the -u option is supplied.\n    Exit Status:\n    The return code is zero, unless end-of-file is encountered, read times out,\n    or an invalid file descriptor is supplied as the argument to -u.",
    },
    BuiltinHelp {
        name: "readonly",
        synopsis: "readonly [-aAf] [name[=value] ...] or readonly -p",
        description: "Mark shell variables as unchangeable.\n    Mark each NAME as read-only; the values of these NAMEs may not be\n    changed by subsequent assignment.\n    Exit Status:\n    Returns success unless an invalid option is given or NAME is invalid.",
    },
    BuiltinHelp {
        name: "return",
        synopsis: "return [n]",
        description: "Return from a shell function.\n    Causes a function or sourced script to exit with the return value\n    specified by N.  If N is omitted, the return status is that of the\n    last command executed within the function or script.\n    Exit Status:\n    Returns N, or failure if the shell is not executing a function or script.",
    },
    BuiltinHelp {
        name: "set",
        synopsis: "set [-abefhkmnptuvxBCHP] [-o option-name] [--] [arg ...]",
        description: "Set or unset values of shell options and positional parameters.\n    Change the value of shell attributes and positional parameters, or\n    display the names and values of shell variables.\n    Exit Status:\n    Returns success unless an invalid option is given.",
    },
    BuiltinHelp {
        name: "shift",
        synopsis: "shift [n]",
        description: "Shift positional parameters.\n    Rename the positional parameters $N+1,$N+2 ... to $1,$2 ...  If N is\n    not given, it is assumed to be 1.\n    Exit Status:\n    Returns success unless N is negative or greater than $#.",
    },
    BuiltinHelp {
        name: "shopt",
        synopsis: "shopt [-pqsu] [-o] [optname ...]",
        description: "Set and unset shell options.\n    Change the setting of each shell option OPTNAME.  Without any option\n    arguments, list each supplied OPTNAME, or all shell options if no\n    OPTNAMEs are given, with an indication of whether or not each is set.\n    Exit Status:\n    Returns success if OPTNAME is enabled; fails if an invalid option is\n    given or OPTNAME is disabled.",
    },
    BuiltinHelp {
        name: "source",
        synopsis: "source filename [arguments]",
        description: "Execute commands from a file in the current shell.\n    Read and execute commands from FILENAME in the current shell.\n    The entries in $PATH are used to find the directory containing FILENAME.\n    Exit Status:\n    Returns the status of the last command executed in FILENAME.",
    },
    BuiltinHelp {
        name: "test",
        synopsis: "test [expr]",
        description: "Evaluate conditional expression.\n    Exits with a status of 0 (true) or 1 (false) depending on\n    the evaluation of EXPR.  Expressions may be unary or binary.\n    Exit Status:\n    Returns success if EXPR evaluates to true; fails if EXPR evaluates to\n    false or an invalid argument is given.",
    },
    BuiltinHelp {
        name: "true",
        synopsis: "true",
        description: "Return a successful result.\n    Exit Status:\n    Always succeeds.",
    },
    BuiltinHelp {
        name: "type",
        synopsis: "type [-afptP] name [name ...]",
        description: "Display information about command type.\n    For each NAME, indicate how it would be interpreted if used as a\n    command name.\n    Exit Status:\n    Returns success if all of the NAMEs are found; fails if any are not found.",
    },
    BuiltinHelp {
        name: "unset",
        synopsis: "unset [-f] [-v] [-n] [name ...]",
        description: "Unset values and attributes of shell variables and functions.\n    For each NAME, remove the corresponding variable or function.\n    Exit Status:\n    Returns success unless an invalid option is given or a NAME is read-only.",
    },
    BuiltinHelp {
        name: "wait",
        synopsis: "wait [-fn] [id ...]",
        description: "Wait for job completion and return exit status.\n    Waits for each process identified by an ID, which may be a process ID or a\n    job specification, and reports its termination status.\n    Exit Status:\n    Returns the status of the last ID; fails if ID is invalid or an invalid\n    option is given.",
    },
];

/// Get all builtin names sorted
fn get_all_builtins() -> Vec<&'static str> {
    let mut names: Vec<&str> = BUILTIN_HELP.iter().map(|h| h.name).collect();
    names.sort();
    names
}

/// Handle the help builtin command.
pub fn handle_help(args: &[String]) -> BuiltinResult {
    let mut short_form = false;
    let mut patterns: Vec<&str> = Vec::new();

    // Parse arguments
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            i += 1;
            // Remaining args are patterns
            while i < args.len() {
                patterns.push(&args[i]);
                i += 1;
            }
            break;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            for flag in arg[1..].chars() {
                if flag == 's' {
                    short_form = true;
                } else {
                    return BuiltinResult::failure(
                        &format!("bash: help: -{}: invalid option\n", flag),
                        2,
                    );
                }
            }
            i += 1;
        } else {
            patterns.push(arg);
            i += 1;
        }
    }

    // No patterns: list all builtins
    if patterns.is_empty() {
        return list_all_builtins();
    }

    // With patterns: show help for matching builtins
    let mut stdout = String::new();
    let mut has_error = false;
    let mut stderr = String::new();

    for pattern in patterns {
        let matches = find_matching_builtins(pattern);

        if matches.is_empty() {
            stderr.push_str(&format!(
                "bash: help: no help topics match `{}'. Try `help help' or `man -k {}' or `info {}'.\n",
                pattern, pattern, pattern
            ));
            has_error = true;
            continue;
        }

        for help in matches {
            if short_form {
                stdout.push_str(&format!("{}: {}\n", help.name, help.synopsis));
            } else {
                stdout.push_str(&format!("{}: {}\n{}\n", help.name, help.synopsis, help.description));
            }
        }
    }

    BuiltinResult {
        stdout,
        stderr,
        exit_code: if has_error { 1 } else { 0 },
    }
}

/// Find builtins matching a pattern (supports glob-style wildcards)
fn find_matching_builtins(pattern: &str) -> Vec<&'static BuiltinHelp> {
    BUILTIN_HELP
        .iter()
        .filter(|h| glob_match(pattern, h.name))
        .collect()
}

/// Simple glob matching (supports * and ?)
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    glob_match_impl(&pattern_chars, &text_chars)
}

fn glob_match_impl(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }

    match pattern[0] {
        '*' => {
            // Try matching zero or more characters
            for i in 0..=text.len() {
                if glob_match_impl(&pattern[1..], &text[i..]) {
                    return true;
                }
            }
            false
        }
        '?' => {
            // Match exactly one character
            !text.is_empty() && glob_match_impl(&pattern[1..], &text[1..])
        }
        c => {
            // Match literal character
            !text.is_empty() && text[0] == c && glob_match_impl(&pattern[1..], &text[1..])
        }
    }
}

/// List all builtins in a formatted table
fn list_all_builtins() -> BuiltinResult {
    let mut lines: Vec<String> = Vec::new();

    lines.push("just-bash shell builtins".to_string());
    lines.push("These shell commands are defined internally. Type `help' to see this list.".to_string());
    lines.push("Type `help name' to find out more about the function `name'.".to_string());
    lines.push(String::new());

    // Create two-column output with builtin names
    let max_width = 36;
    let builtins = get_all_builtins();

    // Build pairs for two-column display
    let midpoint = (builtins.len() + 1) / 2;
    for i in 0..midpoint {
        let left = builtins.get(i).copied().unwrap_or("");
        let right = builtins.get(i + midpoint).copied().unwrap_or("");
        if right.is_empty() {
            lines.push(left.to_string());
        } else {
            lines.push(format!("{:width$}{}", left, right, width = max_width));
        }
    }

    BuiltinResult {
        stdout: format!("{}\n", lines.join("\n")),
        stderr: String::new(),
        exit_code: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_no_args() {
        let result = handle_help(&[]);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("just-bash shell builtins"));
    }

    #[test]
    fn test_help_specific_builtin() {
        let result = handle_help(&["echo".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("echo:"));
        assert!(result.stdout.contains("Write arguments"));
    }

    #[test]
    fn test_help_short_form() {
        let result = handle_help(&["-s".to_string(), "echo".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("echo:"));
        // Short form should not include full description
        assert!(!result.stdout.contains("Write arguments"));
    }

    #[test]
    fn test_help_pattern_wildcard() {
        let result = handle_help(&["e*".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("echo:"));
        assert!(result.stdout.contains("eval:"));
        assert!(result.stdout.contains("exit:"));
        assert!(result.stdout.contains("export:"));
    }

    #[test]
    fn test_help_not_found() {
        let result = handle_help(&["nonexistent".to_string()]);
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("no help topics match"));
    }

    #[test]
    fn test_help_invalid_option() {
        let result = handle_help(&["-z".to_string()]);
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("invalid option"));
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("echo", "echo"));
        assert!(glob_match("e*", "echo"));
        assert!(glob_match("*cho", "echo"));
        assert!(glob_match("e?ho", "echo"));
        assert!(glob_match("*", "anything"));
        assert!(!glob_match("echo", "eval"));
        assert!(!glob_match("e?o", "echo"));
    }
}
