//! Command and Arithmetic Substitution Parsing Helpers
//!
//! Contains pure string analysis functions and substitution parsing utilities
//! extracted from the main parser.

use crate::ast::types::{CommandSubstitutionPart, ScriptNode, AST};

/// Type for a parser factory function that creates new parser instances.
/// Used to avoid circular dependencies.
pub type ParserFactory = fn(&str) -> ScriptNode;

/// Type for an error reporting function that panics.
pub type ErrorFn = fn(&str);

/// Check if $(( at position `start` in `value` is a command substitution with nested
/// subshell rather than arithmetic expansion. This uses similar logic to the lexer's
/// dparenClosesWithSpacedParens but operates on a string within a word/expansion.
///
/// The key heuristics are:
/// 1. If it closes with `) )` (separated by whitespace or content), it's a subshell
/// 2. If at depth 1 we see `||`, `&&`, or single `|`, it's a command context
/// 3. If it closes with `))`, it's arithmetic
///
/// # Arguments
/// * `value` - The string containing the expansion
/// * `start` - Position of the `$` in `$((` (so `$((` is at start..start+2)
///
/// # Returns
/// true if this should be parsed as command substitution, false for arithmetic
pub fn is_dollar_dparen_subshell(value: &str, start: usize) -> bool {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    let mut pos = start + 3; // Skip past $((
    let mut depth: i32 = 2; // We've seen ((, so we start at depth 2
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while pos < len && depth > 0 {
        let c = chars[pos];

        if in_single_quote {
            if c == '\'' {
                in_single_quote = false;
            }
            pos += 1;
            continue;
        }

        if in_double_quote {
            if c == '\\' {
                // Skip escaped char
                pos += 2;
                continue;
            }
            if c == '"' {
                in_double_quote = false;
            }
            pos += 1;
            continue;
        }

        // Not in quotes
        if c == '\'' {
            in_single_quote = true;
            pos += 1;
            continue;
        }

        if c == '"' {
            in_double_quote = true;
            pos += 1;
            continue;
        }

        if c == '\\' {
            // Skip escaped char
            pos += 2;
            continue;
        }

        if c == '(' {
            depth += 1;
            pos += 1;
            continue;
        }

        if c == ')' {
            depth -= 1;
            if depth == 1 {
                // We just closed the inner subshell, now at outer level
                // Check if next char is another ) - if so, it's )) = arithmetic
                let next_pos = pos + 1;
                if next_pos < len && chars[next_pos] == ')' {
                    // )) - adjacent parens = arithmetic, not nested subshells
                    return false;
                }
                // The ) is followed by something else (whitespace, content, etc.)
                // This indicates it's a subshell with more content after the inner )
                // e.g., $((which cmd || echo fallback)2>/dev/null)
                // After `(which cmd || echo fallback)` we have `2>/dev/null)` before the final `)`
                return true;
            }
            if depth == 0 {
                // We closed all parens without the pattern we're looking for
                return false;
            }
            pos += 1;
            continue;
        }

        // Check for || or && or | at depth 1 (between inner subshells)
        // At depth 1, we're inside the outer (( but outside any inner parens.
        // If we see || or && or | here, it's connecting commands, not arithmetic.
        if depth == 1 {
            if c == '|' && pos + 1 < len && chars[pos + 1] == '|' {
                return true;
            }
            if c == '&' && pos + 1 < len && chars[pos + 1] == '&' {
                return true;
            }
            if c == '|' && pos + 1 < len && chars[pos + 1] != '|' {
                // Single | - pipeline operator
                return true;
            }
        }

        pos += 1;
    }

    // Didn't find a definitive answer - default to arithmetic behavior
    false
}

/// Parse a command substitution starting at the given position.
/// Handles $(...) syntax with proper depth tracking for nested substitutions.
///
/// # Arguments
/// * `value` - The string containing the substitution
/// * `start` - Position of the `$` in `$(`
/// * `create_parser` - Factory function to create a new parser instance
/// * `error` - Error reporting function (diverges)
///
/// # Returns
/// The parsed command substitution part and the ending index
pub fn parse_command_substitution_from_string(
    value: &str,
    start: usize,
    create_parser: ParserFactory,
    error: ErrorFn,
) -> (CommandSubstitutionPart, usize) {
    let chars: Vec<char> = value.chars().collect();
    // Skip $(
    let cmd_start = start + 2;
    let mut depth = 1;
    let mut i = cmd_start;

    // Track context for case statements
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut case_depth = 0;
    let mut in_case_pattern = false;
    let mut word_buffer = String::new();

    while i < chars.len() && depth > 0 {
        let c = chars[i];

        if in_single_quote {
            if c == '\'' {
                in_single_quote = false;
            }
        } else if in_double_quote {
            if c == '\\' && i + 1 < chars.len() {
                i += 1; // Skip escaped char
            } else if c == '"' {
                in_double_quote = false;
            }
        } else {
            // Not in quotes
            if c == '\'' {
                in_single_quote = true;
                word_buffer.clear();
            } else if c == '"' {
                in_double_quote = true;
                word_buffer.clear();
            } else if c == '\\' && i + 1 < chars.len() {
                i += 1; // Skip escaped char
                word_buffer.clear();
            } else if c.is_ascii_alphabetic() || c == '_' {
                word_buffer.push(c);
            } else {
                // Check for keywords
                if word_buffer == "case" {
                    case_depth += 1;
                    in_case_pattern = false;
                } else if word_buffer == "in" && case_depth > 0 {
                    in_case_pattern = true;
                } else if word_buffer == "esac" && case_depth > 0 {
                    case_depth -= 1;
                    in_case_pattern = false;
                }
                word_buffer.clear();

                if c == '(' {
                    // Check for $( which starts nested command substitution
                    if i > 0 && chars[i - 1] == '$' {
                        depth += 1;
                    } else if !in_case_pattern {
                        depth += 1;
                    }
                } else if c == ')' {
                    if in_case_pattern {
                        // ) ends the case pattern, doesn't affect depth
                        in_case_pattern = false;
                    } else {
                        depth -= 1;
                    }
                } else if c == ';' {
                    // ;; in case body means next pattern
                    if case_depth > 0 && i + 1 < chars.len() && chars[i + 1] == ';' {
                        in_case_pattern = true;
                    }
                }
            }
        }

        if depth > 0 {
            i += 1;
        }
    }

    // Check for unclosed command substitution
    if depth > 0 {
        error("unexpected EOF while looking for matching `)'");
        unreachable!("error function should panic");
    }

    let cmd_str: String = chars[cmd_start..i].iter().collect();
    // Use a new Parser instance to avoid overwriting the caller's parser's tokens
    let body = create_parser(&cmd_str);

    (
        CommandSubstitutionPart {
            body,
            legacy: false,
        },
        i + 1,
    )
}

/// Parse a backtick command substitution starting at the given position.
/// Handles `...` syntax with proper escape processing.
///
/// # Arguments
/// * `value` - The string containing the substitution
/// * `start` - Position of the opening backtick
/// * `in_double_quotes` - Whether the backtick is inside double quotes
/// * `create_parser` - Factory function to create a new parser instance
/// * `error` - Error reporting function (diverges)
///
/// # Returns
/// The parsed command substitution part and the ending index
pub fn parse_backtick_substitution_from_string(
    value: &str,
    start: usize,
    in_double_quotes: bool,
    create_parser: ParserFactory,
    error: ErrorFn,
) -> (CommandSubstitutionPart, usize) {
    let chars: Vec<char> = value.chars().collect();
    let cmd_start = start + 1;
    let mut i = cmd_start;
    let mut cmd_str = String::new();

    // Process backtick escaping rules:
    // \$ \` \\ \<newline> have backslash removed
    // \" has backslash removed ONLY inside double quotes
    // \x for other chars keeps the backslash
    while i < chars.len() && chars[i] != '`' {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            // In unquoted context: only \$ \` \\ \newline are special
            // In double-quoted context: also \" is special
            let is_special = next == '$'
                || next == '`'
                || next == '\\'
                || next == '\n'
                || (in_double_quotes && next == '"');
            if is_special {
                // Remove the backslash, keep the next char (or nothing for newline)
                if next != '\n' {
                    cmd_str.push(next);
                }
                i += 2;
            } else {
                // Keep the backslash for other characters
                cmd_str.push(chars[i]);
                i += 1;
            }
        } else {
            cmd_str.push(chars[i]);
            i += 1;
        }
    }

    // Check for unclosed backtick substitution
    if i >= chars.len() {
        error("unexpected EOF while looking for matching ``'");
        unreachable!("error function should panic");
    }

    // Use a new Parser instance to avoid overwriting the caller's parser's tokens
    let body = create_parser(&cmd_str);

    (
        CommandSubstitutionPart { body, legacy: true },
        i + 1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_dollar_dparen_subshell_arithmetic() {
        // Simple arithmetic expression
        assert!(!is_dollar_dparen_subshell("$((1+2))", 0));
        // Nested arithmetic
        assert!(!is_dollar_dparen_subshell("$((x+(y*2)))", 0));
    }

    #[test]
    fn test_is_dollar_dparen_subshell_subshell() {
        // The key heuristic is `) )` pattern - space/content between closing parens
        // e.g., $((cmd) 2>/dev/null) where the inner ) is followed by content before outer )
        assert!(is_dollar_dparen_subshell("$((cmd) x)", 0));
        // Content between inner ) and outer )
        assert!(is_dollar_dparen_subshell("$((which cmd) 2>/dev/null)", 0));
    }

    #[test]
    fn test_is_dollar_dparen_subshell_nested() {
        // Content after inner closing paren
        assert!(is_dollar_dparen_subshell("$((which cmd) 2>/dev/null)", 0));
    }
}
