//! Word Analysis
//!
//! Functions for analyzing word parts to determine what types of expansions are present.

use crate::ast::types::{
    InnerParameterOperation, ParameterExpansionPart, ParameterOperation, WordPart,
};
use regex_lite::Regex;

/// Check if a glob pattern string contains variable references ($var or ${var})
/// This is used to detect when IFS splitting should apply to expanded glob patterns.
pub fn glob_pattern_has_var_ref(pattern: &str) -> bool {
    // Look for $varname or ${...} patterns
    // Skip escaped $ (e.g., \$)
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2; // Skip next character
            continue;
        }
        if chars[i] == '$' {
            if let Some(&next) = chars.get(i + 1) {
                // Check for ${...} or $varname
                if next == '{' || next.is_ascii_alphabetic() || next == '_' {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// Get the word parts from an operation if it has a word field
fn get_operation_word_parts(op: &ParameterOperation) -> Option<&Vec<WordPart>> {
    match op {
        ParameterOperation::Inner(inner) => match inner {
            InnerParameterOperation::DefaultValue(dv) => Some(&dv.word.parts),
            InnerParameterOperation::AssignDefault(ad) => Some(&ad.word.parts),
            InnerParameterOperation::UseAlternative(ua) => Some(&ua.word.parts),
            InnerParameterOperation::ErrorIfUnset(eiu) => eiu.word.as_ref().map(|w| &w.parts),
            _ => None,
        },
        _ => None,
    }
}

/// Check if a parameter expansion has quoted parts in its operation word
/// e.g., ${v:-"AxBxC"} has a quoted default value
fn has_quoted_operation_word(part: &ParameterExpansionPart) -> bool {
    let word_parts = part.operation.as_ref().and_then(get_operation_word_parts);

    if let Some(parts) = word_parts {
        for p in parts {
            match p {
                WordPart::DoubleQuoted(_) | WordPart::SingleQuoted(_) => return true,
                _ => {}
            }
        }
    }
    false
}

/// Check if a parameter expansion's operation word is entirely quoted (all parts are quoted).
/// This is different from has_quoted_operation_word which returns true if ANY part is quoted.
///
/// For word splitting purposes:
/// - ${v:-"AxBxC"} - entirely quoted, should NOT be split
/// - ${v:-x"AxBxC"x} - mixed quoted/unquoted, SHOULD be split (on unquoted parts)
/// - ${v:-AxBxC} - entirely unquoted, SHOULD be split
pub fn is_operation_word_entirely_quoted(part: &ParameterExpansionPart) -> bool {
    let word_parts = part.operation.as_ref().and_then(get_operation_word_parts);

    match word_parts {
        Some(parts) if !parts.is_empty() => {
            // Check if ALL parts are quoted (DoubleQuoted or SingleQuoted)
            for p in parts {
                match p {
                    WordPart::DoubleQuoted(_) | WordPart::SingleQuoted(_) => {}
                    _ => return false, // Found an unquoted part
                }
            }
            true // All parts are quoted
        }
        _ => false,
    }
}

/// Result of analyzing word parts
#[derive(Debug, Clone, Default)]
pub struct WordPartsAnalysis {
    pub has_quoted: bool,
    pub has_command_sub: bool,
    pub has_array_var: bool,
    pub has_array_at_expansion: bool,
    pub has_param_expansion: bool,
    pub has_var_name_prefix_expansion: bool,
    pub has_indirection: bool,
}

/// Check if operation is PatternRemoval or PatternReplacement
fn is_pattern_operation(op: &ParameterOperation) -> bool {
    matches!(
        op,
        ParameterOperation::Inner(InnerParameterOperation::PatternRemoval(_))
            | ParameterOperation::Inner(InnerParameterOperation::PatternReplacement(_))
    )
}

/// Analyze word parts for expansion behavior
pub fn analyze_word_parts(parts: &[WordPart]) -> WordPartsAnalysis {
    let mut result = WordPartsAnalysis::default();
    let array_pattern = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*\[[@*]\]$").unwrap();

    for part in parts {
        match part {
            WordPart::SingleQuoted(_) => {
                result.has_quoted = true;
            }
            WordPart::DoubleQuoted(dq) => {
                result.has_quoted = true;
                // Check for "${a[@]}" inside double quotes
                // BUT NOT if there's an operation like ${#a[@]} (Length) or other operations
                for inner in &dq.parts {
                    if let WordPart::ParameterExpansion(pe) = inner {
                        // Check if it's array[@] or array[*]
                        if array_pattern.is_match(&pe.parameter) {
                            // Set has_array_at_expansion for:
                            // - No operation: ${arr[@]}
                            // - PatternRemoval: ${arr[@]#pattern}, ${arr[@]%pattern}
                            // - PatternReplacement: ${arr[@]/pattern/replacement}
                            match &pe.operation {
                                None => {
                                    result.has_array_at_expansion = true;
                                }
                                Some(op) if is_pattern_operation(op) => {
                                    result.has_array_at_expansion = true;
                                }
                                _ => {}
                            }
                        }
                        // Check for ${!prefix@} or ${!prefix*} inside double quotes
                        match &pe.operation {
                            Some(ParameterOperation::VarNamePrefix(_))
                            | Some(ParameterOperation::ArrayKeys(_)) => {
                                result.has_var_name_prefix_expansion = true;
                            }
                            Some(ParameterOperation::Indirection(_)) => {
                                result.has_indirection = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            WordPart::CommandSubstitution(_) => {
                result.has_command_sub = true;
            }
            WordPart::ParameterExpansion(pe) => {
                result.has_param_expansion = true;
                if pe.parameter == "@" || pe.parameter == "*" {
                    result.has_array_var = true;
                }
                // Check if the parameter expansion has quoted parts in its operation
                // e.g., ${v:-"AxBxC"} - the quoted default value should prevent word splitting
                if has_quoted_operation_word(pe) {
                    result.has_quoted = true;
                }
                // Check for unquoted ${!prefix@} or ${!prefix*}
                match &pe.operation {
                    Some(ParameterOperation::VarNamePrefix(_))
                    | Some(ParameterOperation::ArrayKeys(_)) => {
                        result.has_var_name_prefix_expansion = true;
                    }
                    Some(ParameterOperation::Indirection(_)) => {
                        result.has_indirection = true;
                    }
                    _ => {}
                }
            }
            // Check Glob parts for variable references - patterns like +($ABC) contain
            // parameter expansions that should be subject to IFS splitting
            WordPart::Glob(g) => {
                if glob_pattern_has_var_ref(&g.pattern) {
                    result.has_param_expansion = true;
                }
            }
            _ => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_pattern_has_var_ref() {
        assert!(glob_pattern_has_var_ref("$var"));
        assert!(glob_pattern_has_var_ref("${var}"));
        assert!(glob_pattern_has_var_ref("prefix$var"));
        assert!(glob_pattern_has_var_ref("+($ABC)"));
        assert!(!glob_pattern_has_var_ref("plain"));
        assert!(!glob_pattern_has_var_ref(r"\$var")); // escaped
        assert!(!glob_pattern_has_var_ref("$1")); // not a var name
    }
}
