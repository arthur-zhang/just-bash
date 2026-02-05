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

use crate::ast::types::{
    WordNode, WordPart, LiteralPart, SingleQuotedPart, DoubleQuotedPart,
    ParameterExpansionPart, CommandSubstitutionPart, ArithmeticExpansionPart,
    TildeExpansionPart, GlobPart, BraceExpansionPart, ScriptNode,
};
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
}
