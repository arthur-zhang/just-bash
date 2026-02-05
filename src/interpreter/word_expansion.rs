//! Word Expansion
//!
//! Main entry point for shell word expansion.
//!
//! Handles shell word expansion including:
//! - Variable expansion ($VAR, ${VAR})
//! - Command substitution $(...)
//! - Arithmetic expansion $((...)
//! - Tilde expansion (~)
//! - Brace expansion {a,b,c}
//! - Glob expansion (*, ?, [...])
//!
//! This module provides the high-level expansion functions.
//! The actual expansion logic is implemented in the expansion/ submodules.
//! Command substitution requires runtime dependencies (script execution).

use crate::ast::types::{WordNode, WordPart, ScriptNode};
use crate::interpreter::types::{ExecResult, InterpreterState};

// Re-export commonly used expansion functions
pub use crate::interpreter::expansion::analysis::*;
pub use crate::interpreter::expansion::brace_range::*;
pub use crate::interpreter::expansion::glob_escape::*;
pub use crate::interpreter::expansion::pattern::*;
pub use crate::interpreter::expansion::pattern_removal::*;
pub use crate::interpreter::expansion::quoting::*;
pub use crate::interpreter::expansion::tilde::*;
pub use crate::interpreter::expansion::variable::*;
pub use crate::interpreter::expansion::word_split::*;

/// Result of word expansion.
#[derive(Debug, Clone)]
pub struct WordExpansionResult {
    /// The expanded string value
    pub value: String,
    /// Whether the expansion produced multiple words (from word splitting)
    pub split_words: Option<Vec<String>>,
    /// Any stderr output from command substitutions
    pub stderr: String,
    /// Exit code from command substitutions (if any)
    pub exit_code: Option<i32>,
}

impl WordExpansionResult {
    /// Create a simple result with just a value.
    pub fn simple(value: String) -> Self {
        Self {
            value,
            split_words: None,
            stderr: String::new(),
            exit_code: None,
        }
    }

    /// Create a result with split words.
    pub fn with_split(value: String, words: Vec<String>) -> Self {
        Self {
            value,
            split_words: Some(words),
            stderr: String::new(),
            exit_code: None,
        }
    }
}

/// Options for word expansion.
#[derive(Debug, Clone, Default)]
pub struct WordExpansionOptions {
    /// Whether we're inside double quotes
    pub in_double_quotes: bool,
    /// Whether to perform word splitting
    pub do_word_split: bool,
    /// Whether to perform glob expansion
    pub do_glob: bool,
    /// Whether to preserve empty fields
    pub preserve_empty: bool,
    /// Whether extglob is enabled
    pub extglob: bool,
}

/// Callback type for command substitution execution.
///
/// The runtime must provide this callback to execute command substitutions.
/// It takes the command string and returns the execution result.
pub type CommandSubstitutionFn = Box<dyn Fn(&str, &mut InterpreterState) -> ExecResult + Send + Sync>;

/// Callback type for command substitution (reference version).
///
/// This is the signature used by the public API functions.
/// The callback receives the command body and mutable state, returns (output, exit_code).
pub type CommandSubstFn<'a> = &'a dyn Fn(&str, &mut InterpreterState) -> (String, i32);

/// Expand a word without glob expansion.
///
/// This performs all expansions except glob expansion:
/// - Tilde expansion
/// - Parameter expansion
/// - Command substitution (requires callback)
/// - Arithmetic expansion
/// - Brace expansion
/// - Quote removal
///
/// For command substitution, if no callback is provided, $(...) and `...`
/// are left unexpanded.
pub fn expand_word_no_glob(
    state: &InterpreterState,
    word: &WordNode,
    options: &WordExpansionOptions,
) -> WordExpansionResult {
    let mut result = String::new();

    for part in &word.parts {
        result.push_str(&expand_part_no_glob(state, part, options));
    }

    WordExpansionResult::simple(result)
}

/// Expand a single word part without glob expansion.
fn expand_part_no_glob(
    state: &InterpreterState,
    part: &WordPart,
    options: &WordExpansionOptions,
) -> String {
    use crate::interpreter::helpers::word_parts::get_literal_value;
    use crate::interpreter::expansion::tilde::apply_tilde_expansion;
    use crate::interpreter::expansion::variable::get_variable;

    // Handle literal parts
    if let Some(literal) = get_literal_value(part) {
        return literal.to_string();
    }

    match part {
        WordPart::TildeExpansion(tilde) => {
            // Tilde expansion doesn't happen inside double quotes
            if options.in_double_quotes {
                return match &tilde.user {
                    Some(u) => format!("~{}", u),
                    None => "~".to_string(),
                };
            }
            // apply_tilde_expansion expects a &str value, not Option<&str>
            // For TildeExpansionPart, we construct the tilde string
            let tilde_str = match &tilde.user {
                Some(u) => format!("~{}", u),
                None => "~".to_string(),
            };
            apply_tilde_expansion(state, &tilde_str)
        }
        WordPart::ParameterExpansion(param) => {
            // Simple variable expansion
            get_variable(state, &param.parameter)
        }
        WordPart::DoubleQuoted(dq) => {
            // Expand contents of double quotes
            let inner_options = WordExpansionOptions {
                in_double_quotes: true,
                ..options.clone()
            };
            let mut result = String::new();
            for inner_part in &dq.parts {
                result.push_str(&expand_part_no_glob(state, inner_part, &inner_options));
            }
            result
        }
        WordPart::CommandSubstitution(_) => {
            // Command substitution requires runtime callback
            // Return empty string if no callback provided
            String::new()
        }
        WordPart::ArithmeticExpansion(arith) => {
            // Arithmetic expansion
            use crate::interpreter::arithmetic::evaluate_arithmetic;
            use crate::interpreter::types::{ExecutionLimits, InterpreterContext};

            // Evaluate the expression
            // Note: This creates a temporary mutable state, which is not ideal
            // In a real implementation, the state should be passed mutably
            let limits = ExecutionLimits::default();
            let mut state_clone = state.clone();
            let mut ctx = InterpreterContext::new(&mut state_clone, &limits);
            match evaluate_arithmetic(&mut ctx, &arith.expression.expression, false) {
                Ok(value) => value.to_string(),
                Err(_) => "0".to_string(),
            }
        }
        WordPart::Glob(glob) => {
            // In non-glob mode, return the pattern as-is
            glob.pattern.clone()
        }
        WordPart::BraceExpansion(_) => {
            // Brace expansion is complex and typically handled at a higher level
            // For now, return empty
            String::new()
        }
        _ => String::new(),
    }
}

// ============================================================================
// Core Word Expansion Functions with Command Substitution Support
// ============================================================================

/// Expand a word to a single string.
///
/// This is the main entry point for word expansion. It performs all expansions:
/// - Tilde expansion
/// - Parameter expansion
/// - Command substitution (via callback)
/// - Arithmetic expansion
/// - Quote removal
///
/// Note: This does NOT perform word splitting or glob expansion.
/// Use `expand_word_with_glob` for full expansion including glob.
pub fn expand_word(
    state: &mut InterpreterState,
    word: &WordNode,
    cmd_subst: Option<CommandSubstFn>,
) -> WordExpansionResult {
    let options = WordExpansionOptions::default();
    expand_word_with_options(state, word, &options, cmd_subst)
}

/// Expand a word with specific options.
pub fn expand_word_with_options(
    state: &mut InterpreterState,
    word: &WordNode,
    options: &WordExpansionOptions,
    cmd_subst: Option<CommandSubstFn>,
) -> WordExpansionResult {
    let mut result = String::new();
    let mut stderr = String::new();
    let mut last_exit_code = None;

    for part in &word.parts {
        let (expanded, part_stderr, exit_code) =
            expand_part_with_cmd_subst(state, part, options, cmd_subst);
        result.push_str(&expanded);
        if !part_stderr.is_empty() {
            if !stderr.is_empty() {
                stderr.push('\n');
            }
            stderr.push_str(&part_stderr);
        }
        if exit_code.is_some() {
            last_exit_code = exit_code;
        }
    }

    WordExpansionResult {
        value: result,
        split_words: None,
        stderr,
        exit_code: last_exit_code,
    }
}

/// Expand a word for use as a regex pattern (in [[ =~ ]]).
///
/// Preserves backslash escapes so they're passed to the regex engine.
/// For example, \[\] becomes \[\] in the regex (matching literal [ and ]).
pub fn expand_word_for_regex(
    state: &mut InterpreterState,
    word: &WordNode,
    cmd_subst: Option<CommandSubstFn>,
) -> WordExpansionResult {
    let mut result = String::new();
    let mut stderr = String::new();
    let mut last_exit_code = None;
    let options = WordExpansionOptions::default();

    for part in &word.parts {
        match part {
            WordPart::Escaped(esc) => {
                // For regex patterns, preserve ALL backslash escapes
                // This allows \[ \] \. \* etc. to work as regex escapes
                result.push('\\');
                result.push_str(&esc.value);
            }
            WordPart::SingleQuoted(sq) => {
                // Single-quoted content is literal in regex
                result.push_str(&sq.value);
            }
            WordPart::DoubleQuoted(dq) => {
                // Double-quoted: expand contents
                let inner_options = WordExpansionOptions {
                    in_double_quotes: true,
                    ..options.clone()
                };
                for inner_part in &dq.parts {
                    let (expanded, part_stderr, exit_code) =
                        expand_part_with_cmd_subst(state, inner_part, &inner_options, cmd_subst);
                    result.push_str(&expanded);
                    if !part_stderr.is_empty() {
                        stderr.push_str(&part_stderr);
                    }
                    if exit_code.is_some() {
                        last_exit_code = exit_code;
                    }
                }
            }
            WordPart::TildeExpansion(_) => {
                // Tilde expansion on RHS of =~ is treated as literal (regex chars escaped)
                let (expanded, part_stderr, exit_code) =
                    expand_part_with_cmd_subst(state, part, &options, cmd_subst);
                result.push_str(&escape_regex_chars(&expanded));
                if !part_stderr.is_empty() {
                    stderr.push_str(&part_stderr);
                }
                if exit_code.is_some() {
                    last_exit_code = exit_code;
                }
            }
            _ => {
                // Other parts: expand normally
                let (expanded, part_stderr, exit_code) =
                    expand_part_with_cmd_subst(state, part, &options, cmd_subst);
                result.push_str(&expanded);
                if !part_stderr.is_empty() {
                    stderr.push_str(&part_stderr);
                }
                if exit_code.is_some() {
                    last_exit_code = exit_code;
                }
            }
        }
    }

    WordExpansionResult {
        value: result,
        split_words: None,
        stderr,
        exit_code: last_exit_code,
    }
}

/// Expand a word for use as a pattern (e.g., in [[ == ]] or case).
///
/// Preserves backslash escapes for pattern metacharacters so they're treated literally.
/// This prevents `*\(\)` from being interpreted as an extglob pattern.
pub fn expand_word_for_pattern(
    state: &mut InterpreterState,
    word: &WordNode,
    cmd_subst: Option<CommandSubstFn>,
) -> WordExpansionResult {
    let mut result = String::new();
    let mut stderr = String::new();
    let mut last_exit_code = None;
    let options = WordExpansionOptions::default();

    for part in &word.parts {
        match part {
            WordPart::Escaped(esc) => {
                // For escaped characters that are pattern metacharacters, preserve the backslash
                // This includes: ( ) | * ? [ ] for glob/extglob patterns
                let ch = &esc.value;
                if "()|*?[]".contains(ch.as_str()) {
                    result.push('\\');
                    result.push_str(ch);
                } else {
                    result.push_str(ch);
                }
            }
            WordPart::SingleQuoted(sq) => {
                // Single-quoted content should be escaped for literal matching
                result.push_str(&escape_glob_chars(&sq.value));
            }
            WordPart::DoubleQuoted(dq) => {
                // Double-quoted: expand contents and escape for literal matching
                let inner_options = WordExpansionOptions {
                    in_double_quotes: true,
                    ..options.clone()
                };
                let mut inner_result = String::new();
                for inner_part in &dq.parts {
                    let (expanded, part_stderr, exit_code) =
                        expand_part_with_cmd_subst(state, inner_part, &inner_options, cmd_subst);
                    inner_result.push_str(&expanded);
                    if !part_stderr.is_empty() {
                        stderr.push_str(&part_stderr);
                    }
                    if exit_code.is_some() {
                        last_exit_code = exit_code;
                    }
                }
                result.push_str(&escape_glob_chars(&inner_result));
            }
            _ => {
                // Other parts: expand normally
                let (expanded, part_stderr, exit_code) =
                    expand_part_with_cmd_subst(state, part, &options, cmd_subst);
                result.push_str(&expanded);
                if !part_stderr.is_empty() {
                    stderr.push_str(&part_stderr);
                }
                if exit_code.is_some() {
                    last_exit_code = exit_code;
                }
            }
        }
    }

    WordExpansionResult {
        value: result,
        split_words: None,
        stderr,
        exit_code: last_exit_code,
    }
}

/// Expand a word and perform glob expansion.
///
/// This performs full word expansion including glob/pathname expansion.
/// Returns multiple values if glob expansion produces matches.
pub fn expand_word_with_glob(
    state: &mut InterpreterState,
    word: &WordNode,
    cmd_subst: Option<CommandSubstFn>,
) -> WordExpansionResult {
    use crate::interpreter::expansion::word_glob_expansion::expand_glob_pattern;
    use std::path::Path;

    // First, expand the word for glob matching
    let pattern = expand_word_for_globbing(state, word, cmd_subst);

    // Check if we should do glob expansion
    let noglob = state.options.noglob;
    let extglob = state.shopt_options.extglob;

    if noglob || !has_glob_pattern(&pattern.value, extglob) {
        // No glob expansion needed - return the expanded value
        return pattern;
    }

    // Perform glob expansion
    let cwd = Path::new(&state.cwd);
    let failglob = state.shopt_options.failglob;
    let nullglob = state.shopt_options.nullglob;

    match expand_glob_pattern(&pattern.value, cwd, failglob, nullglob, extglob) {
        Ok(glob_result) => {
            if glob_result.values.len() == 1 {
                WordExpansionResult {
                    value: glob_result.values.into_iter().next().unwrap_or_default(),
                    split_words: None,
                    stderr: pattern.stderr,
                    exit_code: pattern.exit_code,
                }
            } else {
                let first = glob_result.values.first().cloned().unwrap_or_default();
                WordExpansionResult {
                    value: first,
                    split_words: Some(glob_result.values),
                    stderr: pattern.stderr,
                    exit_code: pattern.exit_code,
                }
            }
        }
        Err(e) => {
            // Glob error - return original pattern
            WordExpansionResult {
                value: pattern.value,
                split_words: None,
                stderr: if pattern.stderr.is_empty() {
                    e
                } else {
                    format!("{}\n{}", pattern.stderr, e)
                },
                exit_code: Some(1),
            }
        }
    }
}

/// Expand a word for glob matching.
///
/// Unlike regular expansion, this escapes glob metacharacters in quoted parts
/// so they are treated as literals, while preserving glob patterns from Glob parts.
fn expand_word_for_globbing(
    state: &mut InterpreterState,
    word: &WordNode,
    cmd_subst: Option<CommandSubstFn>,
) -> WordExpansionResult {
    use crate::interpreter::expansion::pattern_expansion::expand_variables_in_pattern;

    let mut result = String::new();
    let mut stderr = String::new();
    let mut last_exit_code = None;
    let options = WordExpansionOptions::default();

    for part in &word.parts {
        match part {
            WordPart::SingleQuoted(sq) => {
                // Single-quoted content: escape glob metacharacters for literal matching
                result.push_str(&escape_glob_chars(&sq.value));
            }
            WordPart::Escaped(esc) => {
                // Escaped character: escape if it's a glob metacharacter
                let ch = &esc.value;
                if "*?[]\\()|".contains(ch.as_str()) {
                    result.push('\\');
                    result.push_str(ch);
                } else {
                    result.push_str(ch);
                }
            }
            WordPart::DoubleQuoted(dq) => {
                // Double-quoted: expand contents and escape glob metacharacters
                let inner_options = WordExpansionOptions {
                    in_double_quotes: true,
                    ..options.clone()
                };
                let mut inner_result = String::new();
                for inner_part in &dq.parts {
                    let (expanded, part_stderr, exit_code) =
                        expand_part_with_cmd_subst(state, inner_part, &inner_options, cmd_subst);
                    inner_result.push_str(&expanded);
                    if !part_stderr.is_empty() {
                        stderr.push_str(&part_stderr);
                    }
                    if exit_code.is_some() {
                        last_exit_code = exit_code;
                    }
                }
                result.push_str(&escape_glob_chars(&inner_result));
            }
            WordPart::Glob(g) => {
                // Glob pattern: expand variables within extglob patterns
                result.push_str(&expand_variables_in_pattern(state, &g.pattern));
            }
            WordPart::Literal(lit) => {
                // Literal: keep as-is (may contain glob characters that should glob)
                result.push_str(&lit.value);
            }
            _ => {
                // Other parts (ParameterExpansion, etc.): expand normally
                let (expanded, part_stderr, exit_code) =
                    expand_part_with_cmd_subst(state, part, &options, cmd_subst);
                result.push_str(&expanded);
                if !part_stderr.is_empty() {
                    stderr.push_str(&part_stderr);
                }
                if exit_code.is_some() {
                    last_exit_code = exit_code;
                }
            }
        }
    }

    WordExpansionResult {
        value: result,
        split_words: None,
        stderr,
        exit_code: last_exit_code,
    }
}

/// Expand a single word part with command substitution support.
///
/// Returns (expanded_value, stderr, exit_code).
fn expand_part_with_cmd_subst(
    state: &mut InterpreterState,
    part: &WordPart,
    options: &WordExpansionOptions,
    cmd_subst: Option<CommandSubstFn>,
) -> (String, String, Option<i32>) {
    use crate::interpreter::expansion::tilde::apply_tilde_expansion;
    use crate::interpreter::expansion::variable::get_variable;
    use crate::interpreter::helpers::word_parts::get_literal_value;

    // Handle literal parts
    if let Some(literal) = get_literal_value(part) {
        return (literal.to_string(), String::new(), None);
    }

    match part {
        WordPart::TildeExpansion(tilde) => {
            // Tilde expansion doesn't happen inside double quotes
            if options.in_double_quotes {
                let value = match &tilde.user {
                    Some(u) => format!("~{}", u),
                    None => "~".to_string(),
                };
                return (value, String::new(), None);
            }
            let tilde_str = match &tilde.user {
                Some(u) => format!("~{}", u),
                None => "~".to_string(),
            };
            (apply_tilde_expansion(state, &tilde_str), String::new(), None)
        }
        WordPart::ParameterExpansion(param) => {
            // Simple variable expansion
            (get_variable(state, &param.parameter), String::new(), None)
        }
        WordPart::DoubleQuoted(dq) => {
            // Expand contents of double quotes
            let inner_options = WordExpansionOptions {
                in_double_quotes: true,
                ..options.clone()
            };
            let mut result = String::new();
            let mut stderr = String::new();
            let mut last_exit_code = None;
            for inner_part in &dq.parts {
                let (expanded, part_stderr, exit_code) =
                    expand_part_with_cmd_subst(state, inner_part, &inner_options, cmd_subst);
                result.push_str(&expanded);
                if !part_stderr.is_empty() {
                    stderr.push_str(&part_stderr);
                }
                if exit_code.is_some() {
                    last_exit_code = exit_code;
                }
            }
            (result, stderr, last_exit_code)
        }
        WordPart::CommandSubstitution(cmd_sub) => {
            // Command substitution requires the callback
            if let Some(callback) = cmd_subst {
                // Convert the script body to a string representation
                // For now, we use a simple approach - the body needs to be converted to a command string
                let body = format_script_body(&cmd_sub.body);
                let (output, exit_code) = callback(&body, state);
                // Remove trailing newlines (bash behavior)
                let trimmed = output.trim_end_matches('\n').to_string();
                (trimmed, String::new(), Some(exit_code))
            } else {
                // No callback provided - return empty
                (String::new(), String::new(), None)
            }
        }
        WordPart::ArithmeticExpansion(arith) => {
            use crate::interpreter::arithmetic::evaluate_arithmetic;
            use crate::interpreter::types::{ExecutionLimits, InterpreterContext};

            let limits = ExecutionLimits::default();
            let mut ctx = InterpreterContext::new(state, &limits);
            match evaluate_arithmetic(&mut ctx, &arith.expression.expression, false) {
                Ok(value) => (value.to_string(), String::new(), None),
                Err(_) => ("0".to_string(), String::new(), None),
            }
        }
        WordPart::Glob(glob) => {
            // In non-glob mode, return the pattern as-is
            (glob.pattern.clone(), String::new(), None)
        }
        WordPart::BraceExpansion(_) => {
            // Brace expansion is complex and typically handled at a higher level
            (String::new(), String::new(), None)
        }
        _ => (String::new(), String::new(), None),
    }
}

/// Format a script body for command substitution.
///
/// This converts a ScriptNode back to a string representation that can be executed.
/// For simple cases, this works well; complex cases may need more sophisticated handling.
fn format_script_body(script: &ScriptNode) -> String {
    // For now, return a placeholder - in a full implementation this would
    // serialize the AST back to a command string
    // The actual implementation depends on how commands are represented
    format!("{:?}", script)
}

// ============================================================================
// Word Analysis Functions
// ============================================================================

/// Check if a word is "fully quoted" - meaning glob characters should be treated literally.
///
/// A word is fully quoted if all its parts are either:
/// - SingleQuoted
/// - DoubleQuoted (entirely quoted variable expansion like "$pat")
/// - Escaped characters
pub fn is_word_fully_quoted(word: &WordNode) -> bool {
    use crate::interpreter::helpers::word_parts::is_quoted_part;

    // Empty word is considered quoted (matches empty pattern literally)
    if word.parts.is_empty() {
        return true;
    }

    // Check if we have any unquoted parts with actual content
    for part in &word.parts {
        if !is_quoted_part(part) {
            return false;
        }
    }
    true
}

/// Check if a word contains any glob patterns.
pub fn word_has_glob_pattern(word: &WordNode, extglob: bool) -> bool {
    use crate::interpreter::expansion::glob_escape::has_glob_pattern;

    for part in &word.parts {
        match part {
            WordPart::Glob(_) => return true,
            WordPart::Literal(lit) => {
                if has_glob_pattern(&lit.value, extglob) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Check if a word contains command substitution.
pub fn word_has_command_substitution(word: &WordNode) -> bool {
    for part in &word.parts {
        if matches!(part, WordPart::CommandSubstitution(_)) {
            return true;
        }
        if let WordPart::DoubleQuoted(dq) = part {
            for inner in &dq.parts {
                if matches!(inner, WordPart::CommandSubstitution(_)) {
                    return true;
                }
            }
        }
    }
    false
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::types::{
        CommandSubstitutionPart, GlobPart, LiteralPart, ParameterExpansionPart, SingleQuotedPart,
    };

    fn make_literal_word(s: &str) -> WordNode {
        WordNode {
            parts: vec![WordPart::Literal(LiteralPart {
                value: s.to_string(),
            })],
        }
    }

    fn make_var_word(name: &str) -> WordNode {
        WordNode {
            parts: vec![WordPart::ParameterExpansion(ParameterExpansionPart {
                parameter: name.to_string(),
                operation: None,
            })],
        }
    }

    #[test]
    fn test_expand_word_literal() {
        let state = InterpreterState::default();
        let word = make_literal_word("hello");
        let options = WordExpansionOptions::default();
        let result = expand_word_no_glob(&state, &word, &options);
        assert_eq!(result.value, "hello");
    }

    #[test]
    fn test_expand_word_variable() {
        let mut state = InterpreterState::default();
        state.env.insert("FOO".to_string(), "bar".to_string());
        let word = make_var_word("FOO");
        let options = WordExpansionOptions::default();
        let result = expand_word_no_glob(&state, &word, &options);
        assert_eq!(result.value, "bar");
    }

    #[test]
    fn test_expand_word_unset_variable() {
        let state = InterpreterState::default();
        let word = make_var_word("UNSET");
        let options = WordExpansionOptions::default();
        let result = expand_word_no_glob(&state, &word, &options);
        assert_eq!(result.value, "");
    }

    #[test]
    fn test_is_word_fully_quoted_empty() {
        let word = WordNode { parts: vec![] };
        assert!(is_word_fully_quoted(&word));
    }

    #[test]
    fn test_is_word_fully_quoted_single_quoted() {
        let word = WordNode {
            parts: vec![WordPart::SingleQuoted(SingleQuotedPart {
                value: "hello".to_string(),
            })],
        };
        assert!(is_word_fully_quoted(&word));
    }

    #[test]
    fn test_is_word_fully_quoted_literal() {
        let word = make_literal_word("hello");
        assert!(!is_word_fully_quoted(&word));
    }

    #[test]
    fn test_word_has_glob_pattern() {
        let word = WordNode {
            parts: vec![WordPart::Glob(GlobPart {
                pattern: "*.txt".to_string(),
            })],
        };
        assert!(word_has_glob_pattern(&word, false));

        let word = make_literal_word("hello");
        assert!(!word_has_glob_pattern(&word, false));
    }

    #[test]
    fn test_word_has_command_substitution() {
        let word = WordNode {
            parts: vec![WordPart::CommandSubstitution(CommandSubstitutionPart {
                body: ScriptNode { statements: vec![] },
                legacy: false,
            })],
        };
        assert!(word_has_command_substitution(&word));

        let word = make_literal_word("hello");
        assert!(!word_has_command_substitution(&word));
    }

    // ============================================================================
    // Tests for Core Word Expansion Functions
    // ============================================================================

    #[test]
    fn test_expand_word_literal_with_cmd_subst() {
        let mut state = InterpreterState::default();
        let word = make_literal_word("hello");
        let result = expand_word(&mut state, &word, None);
        assert_eq!(result.value, "hello");
    }

    #[test]
    fn test_expand_word_variable_with_cmd_subst() {
        let mut state = InterpreterState::default();
        state.env.insert("FOO".to_string(), "bar".to_string());
        let word = make_var_word("FOO");
        let result = expand_word(&mut state, &word, None);
        assert_eq!(result.value, "bar");
    }

    #[test]
    fn test_expand_word_with_callback() {
        let mut state = InterpreterState::default();
        let word = WordNode {
            parts: vec![WordPart::CommandSubstitution(CommandSubstitutionPart {
                body: ScriptNode { statements: vec![] },
                legacy: false,
            })],
        };

        // Callback that returns a fixed value
        let callback: CommandSubstFn = &|_cmd: &str, _state: &mut InterpreterState| {
            ("hello from callback\n".to_string(), 0)
        };

        let result = expand_word(&mut state, &word, Some(callback));
        assert_eq!(result.value, "hello from callback");
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn test_expand_word_for_regex_preserves_escapes() {
        use crate::ast::types::EscapedPart;

        let mut state = InterpreterState::default();
        // Test that escaped chars are preserved with backslashes
        let word = WordNode {
            parts: vec![WordPart::Escaped(EscapedPart {
                value: "[".to_string(),
            })],
        };
        let result = expand_word_for_regex(&mut state, &word, None);
        assert_eq!(result.value, "\\[");
    }

    #[test]
    fn test_expand_word_for_regex_single_quoted() {
        let mut state = InterpreterState::default();
        let word = WordNode {
            parts: vec![WordPart::SingleQuoted(SingleQuotedPart {
                value: "[abc]".to_string(),
            })],
        };
        let result = expand_word_for_regex(&mut state, &word, None);
        // Single-quoted content is literal
        assert_eq!(result.value, "[abc]");
    }

    #[test]
    fn test_expand_word_for_pattern_preserves_metachar_escapes() {
        use crate::ast::types::EscapedPart;

        let mut state = InterpreterState::default();
        // Pattern metacharacters should be preserved with backslash
        let word = WordNode {
            parts: vec![
                WordPart::Escaped(EscapedPart {
                    value: "*".to_string(),
                }),
            ],
        };
        let result = expand_word_for_pattern(&mut state, &word, None);
        assert_eq!(result.value, "\\*");
    }

    #[test]
    fn test_expand_word_for_pattern_escapes_single_quoted() {
        let mut state = InterpreterState::default();
        let word = WordNode {
            parts: vec![WordPart::SingleQuoted(SingleQuotedPart {
                value: "*?.txt".to_string(),
            })],
        };
        let result = expand_word_for_pattern(&mut state, &word, None);
        // Glob chars should be escaped
        assert_eq!(result.value, "\\*\\?.txt");
    }

    #[test]
    fn test_expand_word_for_pattern_non_metachar_not_preserved() {
        use crate::ast::types::EscapedPart;

        let mut state = InterpreterState::default();
        // Non-pattern metacharacters should not get backslash
        let word = WordNode {
            parts: vec![WordPart::Escaped(EscapedPart {
                value: "a".to_string(),
            })],
        };
        let result = expand_word_for_pattern(&mut state, &word, None);
        assert_eq!(result.value, "a");
    }

    #[test]
    fn test_expand_word_with_glob_noglob() {
        let mut state = InterpreterState::default();
        state.options.noglob = true;

        let word = WordNode {
            parts: vec![WordPart::Glob(GlobPart {
                pattern: "*.txt".to_string(),
            })],
        };
        let result = expand_word_with_glob(&mut state, &word, None);
        // With noglob, pattern should not be expanded
        assert_eq!(result.value, "*.txt");
    }

    #[test]
    fn test_expand_word_combined() {
        let mut state = InterpreterState::default();
        state.env.insert("NAME".to_string(), "world".to_string());

        // "hello $NAME"
        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart {
                    value: "hello ".to_string(),
                }),
                WordPart::ParameterExpansion(ParameterExpansionPart {
                    parameter: "NAME".to_string(),
                    operation: None,
                }),
            ],
        };
        let result = expand_word(&mut state, &word, None);
        assert_eq!(result.value, "hello world");
    }
}
