//! shopt builtin - Shell options
//!
//! Implements bash's shopt builtin for managing shell-specific options

use crate::interpreter::helpers::shellopts::{update_bashopts, update_shellopts};
use crate::interpreter::types::InterpreterState;
use super::break_cmd::BuiltinResult;

/// All supported shopt options
const SHOPT_OPTIONS: &[&str] = &[
    "dotglob",
    "expand_aliases",
    "extglob",
    "failglob",
    "globskipdots",
    "globstar",
    "lastpipe",
    "nocaseglob",
    "nocasematch",
    "nullglob",
    "xpg_echo",
];

/// Options that are recognized but not implemented (stubs that return current state)
const STUB_OPTIONS: &[&str] = &[
    "autocd",
    "cdable_vars",
    "cdspell",
    "checkhash",
    "checkjobs",
    "checkwinsize",
    "cmdhist",
    "compat31",
    "compat32",
    "compat40",
    "compat41",
    "compat42",
    "compat43",
    "compat44",
    "complete_fullquote",
    "direxpand",
    "dirspell",
    "execfail",
    "extdebug",
    "extquote",
    "force_fignore",
    "globasciiranges",
    "gnu_errfmt",
    "histappend",
    "histreedit",
    "histverify",
    "hostcomplete",
    "huponexit",
    "inherit_errexit",
    "interactive_comments",
    "lithist",
    "localvar_inherit",
    "localvar_unset",
    "login_shell",
    "mailwarn",
    "no_empty_cmd_completion",
    "progcomp",
    "progcomp_alias",
    "promptvars",
    "restricted_shell",
    "shift_verbose",
    "sourcepath",
];

fn is_shopt_option(opt: &str) -> bool {
    SHOPT_OPTIONS.contains(&opt)
}

fn is_stub_option(opt: &str) -> bool {
    STUB_OPTIONS.contains(&opt)
}

/// Get a shopt option value from state
fn get_shopt_value(state: &InterpreterState, name: &str) -> bool {
    match name {
        "extglob" => state.shopt_options.extglob,
        "dotglob" => state.shopt_options.dotglob,
        "nullglob" => state.shopt_options.nullglob,
        "failglob" => state.shopt_options.failglob,
        "globstar" => state.shopt_options.globstar,
        "globskipdots" => state.shopt_options.globskipdots,
        "nocaseglob" => state.shopt_options.nocaseglob,
        "nocasematch" => state.shopt_options.nocasematch,
        "expand_aliases" => state.shopt_options.expand_aliases,
        "lastpipe" => state.shopt_options.lastpipe,
        "xpg_echo" => state.shopt_options.xpg_echo,
        _ => false,
    }
}

/// Set a shopt option value in state
fn set_shopt_value(state: &mut InterpreterState, name: &str, value: bool) {
    match name {
        "extglob" => state.shopt_options.extglob = value,
        "dotglob" => state.shopt_options.dotglob = value,
        "nullglob" => state.shopt_options.nullglob = value,
        "failglob" => state.shopt_options.failglob = value,
        "globstar" => state.shopt_options.globstar = value,
        "globskipdots" => state.shopt_options.globskipdots = value,
        "nocaseglob" => state.shopt_options.nocaseglob = value,
        "nocasematch" => state.shopt_options.nocasematch = value,
        "expand_aliases" => state.shopt_options.expand_aliases = value,
        "lastpipe" => state.shopt_options.lastpipe = value,
        "xpg_echo" => state.shopt_options.xpg_echo = value,
        _ => {}
    }
}

/// Handle the shopt builtin command.
pub fn handle_shopt(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    // Parse arguments
    let mut set_flag = false;    // -s: set option
    let mut unset_flag = false;  // -u: unset option
    let mut print_flag = false;  // -p: print in reusable form
    let mut quiet_flag = false;  // -q: suppress output, only set exit code
    let mut o_flag = false;      // -o: use set -o option names
    let mut option_names: Vec<&str> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            i += 1;
            break;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            for flag in arg[1..].chars() {
                match flag {
                    's' => set_flag = true,
                    'u' => unset_flag = true,
                    'p' => print_flag = true,
                    'q' => quiet_flag = true,
                    'o' => o_flag = true,
                    _ => {
                        return BuiltinResult::failure(
                            &format!("shopt: -{}: invalid option\n", flag),
                            2,
                        );
                    }
                }
            }
            i += 1;
        } else {
            break;
        }
    }

    // Remaining args are option names
    while i < args.len() {
        option_names.push(&args[i]);
        i += 1;
    }

    // -o flag: use set -o option names instead of shopt options
    if o_flag {
        return handle_set_options(state, &option_names, set_flag, unset_flag, print_flag, quiet_flag);
    }

    // If -s and -u are both set, that's an error
    if set_flag && unset_flag {
        return BuiltinResult::failure(
            "shopt: cannot set and unset shell options simultaneously\n",
            1,
        );
    }

    // No option names: print all options
    if option_names.is_empty() {
        if set_flag || unset_flag {
            // -s or -u without option names: print options with that state
            let mut output: Vec<String> = Vec::new();
            for opt in SHOPT_OPTIONS {
                let value = get_shopt_value(state, opt);
                if set_flag && value {
                    output.push(if print_flag {
                        format!("shopt -s {}", opt)
                    } else {
                        format!("{}\t\ton", opt)
                    });
                } else if unset_flag && !value {
                    output.push(if print_flag {
                        format!("shopt -u {}", opt)
                    } else {
                        format!("{}\t\toff", opt)
                    });
                }
            }
            return BuiltinResult {
                stdout: if output.is_empty() { String::new() } else { format!("{}\n", output.join("\n")) },
                stderr: String::new(),
                exit_code: 0,
            };
        }
        // No flags: print all options
        let mut output: Vec<String> = Vec::new();
        for opt in SHOPT_OPTIONS {
            let value = get_shopt_value(state, opt);
            output.push(if print_flag {
                format!("shopt {} {}", if value { "-s" } else { "-u" }, opt)
            } else {
                format!("{}\t\t{}", opt, if value { "on" } else { "off" })
            });
        }
        return BuiltinResult {
            stdout: format!("{}\n", output.join("\n")),
            stderr: String::new(),
            exit_code: 0,
        };
    }

    // Option names provided
    let mut has_error = false;
    let mut stderr = String::new();
    let mut output: Vec<String> = Vec::new();

    for name in option_names {
        if !is_shopt_option(name) && !is_stub_option(name) {
            stderr.push_str(&format!("shopt: {}: invalid shell option name\n", name));
            has_error = true;
            continue;
        }

        if set_flag {
            // Set the option
            if is_shopt_option(name) {
                set_shopt_value(state, name, true);
                update_bashopts(&mut state.env, &state.shopt_options);
            }
            // Stub options are silently accepted
        } else if unset_flag {
            // Unset the option
            if is_shopt_option(name) {
                set_shopt_value(state, name, false);
                update_bashopts(&mut state.env, &state.shopt_options);
            }
            // Stub options are silently accepted
        } else {
            // Query the option
            if is_shopt_option(name) {
                let value = get_shopt_value(state, name);
                if quiet_flag {
                    if !value {
                        has_error = true;
                    }
                } else if print_flag {
                    output.push(format!("shopt {} {}", if value { "-s" } else { "-u" }, name));
                    if !value {
                        has_error = true;
                    }
                } else {
                    output.push(format!("{}\t\t{}", name, if value { "on" } else { "off" }));
                    if !value {
                        has_error = true;
                    }
                }
            } else {
                // Stub options report as off
                if quiet_flag {
                    has_error = true;
                } else if print_flag {
                    output.push(format!("shopt -u {}", name));
                    has_error = true;
                } else {
                    output.push(format!("{}\t\toff", name));
                    has_error = true;
                }
            }
        }
    }

    BuiltinResult {
        stdout: if output.is_empty() { String::new() } else { format!("{}\n", output.join("\n")) },
        stderr,
        exit_code: if has_error { 1 } else { 0 },
    }
}

/// Handle -o flag: use set -o option names
fn handle_set_options(
    state: &mut InterpreterState,
    option_names: &[&str],
    set_flag: bool,
    unset_flag: bool,
    print_flag: bool,
    quiet_flag: bool,
) -> BuiltinResult {
    // Map set -o option names to ShellOptions fields
    const SET_OPTIONS: &[(&str, fn(&InterpreterState) -> bool, fn(&mut InterpreterState, bool))] = &[
        ("allexport", |s| s.options.allexport, |s, v| s.options.allexport = v),
        ("emacs", |s| s.options.emacs, |s, v| { s.options.emacs = v; if v { s.options.vi = false; } }),
        ("errexit", |s| s.options.errexit, |s, v| s.options.errexit = v),
        ("noclobber", |s| s.options.noclobber, |s, v| s.options.noclobber = v),
        ("noexec", |s| s.options.noexec, |s, v| s.options.noexec = v),
        ("noglob", |s| s.options.noglob, |s, v| s.options.noglob = v),
        ("nounset", |s| s.options.nounset, |s, v| s.options.nounset = v),
        ("pipefail", |s| s.options.pipefail, |s, v| s.options.pipefail = v),
        ("posix", |s| s.options.posix, |s, v| s.options.posix = v),
        ("verbose", |s| s.options.verbose, |s, v| s.options.verbose = v),
        ("vi", |s| s.options.vi, |s, v| { s.options.vi = v; if v { s.options.emacs = false; } }),
        ("xtrace", |s| s.options.xtrace, |s, v| s.options.xtrace = v),
    ];

    // No-op options (recognized but always off)
    const NOOP_OPTIONS: &[&str] = &[
        "braceexpand",
        "errtrace",
        "functrace",
        "hashall",
        "histexpand",
        "history",
        "ignoreeof",
        "interactive-comments",
        "keyword",
        "monitor",
        "nolog",
        "notify",
        "onecmd",
        "physical",
        "privileged",
    ];

    fn find_set_option(name: &str) -> Option<(fn(&InterpreterState) -> bool, fn(&mut InterpreterState, bool))> {
        for (opt_name, getter, setter) in SET_OPTIONS {
            if *opt_name == name {
                return Some((*getter, *setter));
            }
        }
        None
    }

    if option_names.is_empty() {
        // Print all set -o options
        let mut output: Vec<String> = Vec::new();

        // Collect all option names
        let mut all_names: Vec<&str> = SET_OPTIONS.iter().map(|(n, _, _)| *n).collect();
        all_names.extend(NOOP_OPTIONS.iter().copied());
        all_names.sort();

        for opt in all_names {
            let is_noop = NOOP_OPTIONS.contains(&opt);
            let value = if is_noop {
                false
            } else if let Some((getter, _)) = find_set_option(opt) {
                getter(state)
            } else {
                false
            };

            if set_flag && !value { continue; }
            if unset_flag && value { continue; }

            output.push(if print_flag {
                format!("set {} {}", if value { "-o" } else { "+o" }, opt)
            } else {
                format!("{}\t\t{}", opt, if value { "on" } else { "off" })
            });
        }

        return BuiltinResult {
            stdout: if output.is_empty() { String::new() } else { format!("{}\n", output.join("\n")) },
            stderr: String::new(),
            exit_code: 0,
        };
    }

    let mut has_error = false;
    let mut stderr = String::new();
    let mut output: Vec<String> = Vec::new();

    for name in option_names {
        let is_implemented = find_set_option(name).is_some();
        let is_noop = NOOP_OPTIONS.contains(name);

        if !is_implemented && !is_noop {
            stderr.push_str(&format!("shopt: {}: invalid option name\n", name));
            has_error = true;
            continue;
        }

        if is_noop {
            // No-op options are always off and can't be changed
            if !set_flag && !unset_flag {
                // Query the option
                if quiet_flag {
                    has_error = true;
                } else if print_flag {
                    output.push(format!("set +o {}", name));
                    has_error = true;
                } else {
                    output.push(format!("{}\t\toff", name));
                    has_error = true;
                }
            }
            continue;
        }

        let (getter, setter) = find_set_option(name).unwrap();

        if set_flag {
            setter(state, true);
            update_shellopts(&mut state.env, &state.options);
        } else if unset_flag {
            setter(state, false);
            update_shellopts(&mut state.env, &state.options);
        } else {
            let value = getter(state);
            if quiet_flag {
                if !value {
                    has_error = true;
                }
            } else if print_flag {
                output.push(format!("set {} {}", if value { "-o" } else { "+o" }, name));
                if !value {
                    has_error = true;
                }
            } else {
                output.push(format!("{}\t\t{}", name, if value { "on" } else { "off" }));
                if !value {
                    has_error = true;
                }
            }
        }
    }

    BuiltinResult {
        stdout: if output.is_empty() { String::new() } else { format!("{}\n", output.join("\n")) },
        stderr,
        exit_code: if has_error { 1 } else { 0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shopt_list_all() {
        let mut state = InterpreterState::default();
        let result = handle_shopt(&mut state, &[]);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("extglob"));
        assert!(result.stdout.contains("dotglob"));
    }

    #[test]
    fn test_shopt_set_option() {
        let mut state = InterpreterState::default();
        assert!(!state.shopt_options.extglob);

        let result = handle_shopt(&mut state, &["-s".to_string(), "extglob".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(state.shopt_options.extglob);
    }

    #[test]
    fn test_shopt_unset_option() {
        let mut state = InterpreterState::default();
        state.shopt_options.extglob = true;

        let result = handle_shopt(&mut state, &["-u".to_string(), "extglob".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(!state.shopt_options.extglob);
    }

    #[test]
    fn test_shopt_query_option() {
        let mut state = InterpreterState::default();
        state.shopt_options.extglob = true;

        let result = handle_shopt(&mut state, &["extglob".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("on"));
    }

    #[test]
    fn test_shopt_query_unset_option() {
        let mut state = InterpreterState::default();
        state.shopt_options.extglob = false;

        let result = handle_shopt(&mut state, &["extglob".to_string()]);
        assert_eq!(result.exit_code, 1); // Returns 1 if option is off
        assert!(result.stdout.contains("off"));
    }

    #[test]
    fn test_shopt_invalid_option() {
        let mut state = InterpreterState::default();
        let result = handle_shopt(&mut state, &["nonexistent".to_string()]);
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("invalid shell option name"));
    }

    #[test]
    fn test_shopt_o_flag() {
        let mut state = InterpreterState::default();
        let result = handle_shopt(&mut state, &["-o".to_string(), "-s".to_string(), "errexit".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(state.options.errexit);
    }

    #[test]
    fn test_shopt_print_flag() {
        let mut state = InterpreterState::default();
        state.shopt_options.extglob = true;

        let result = handle_shopt(&mut state, &["-p".to_string(), "extglob".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("shopt -s extglob"));
    }

    #[test]
    fn test_shopt_quiet_flag() {
        let mut state = InterpreterState::default();
        state.shopt_options.extglob = true;

        let result = handle_shopt(&mut state, &["-q".to_string(), "extglob".to_string()]);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.is_empty());
    }
}
