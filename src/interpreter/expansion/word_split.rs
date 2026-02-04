//! Word Splitting
//!
//! IFS-based word splitting for unquoted expansions.

use crate::ast::types::{
    InnerParameterOperation, ParameterExpansionPart, ParameterOperation, WordPart,
};
use crate::interpreter::expansion::analysis::{glob_pattern_has_var_ref, is_operation_word_entirely_quoted};
use crate::interpreter::helpers::{get_ifs, split_by_ifs_for_expansion_ex, IfsExpansionSplitResult};
use std::collections::HashMap;

/// Segment for word splitting - represents an expanded part with metadata.
#[derive(Debug, Clone)]
pub struct WordSplitSegment {
    /// The expanded value of this segment
    pub value: String,
    /// Whether this segment is subject to IFS splitting
    pub is_splittable: bool,
    /// Whether this segment is quoted (can anchor empty words)
    pub is_quoted: bool,
}

/// Result of smart word splitting.
#[derive(Debug, Clone)]
pub struct SmartWordSplitResult {
    pub words: Vec<String>,
}

/// Perform smart word splitting on pre-expanded segments.
///
/// In bash, word splitting respects quoted parts. When you have:
/// - $a"$b" where a="1 2" and b="3 4"
/// - The unquoted $a gets split by IFS: "1 2" -> ["1", "2"]
/// - The quoted "$b" does NOT get split, it joins with the last field from $a
/// - Result: ["1", "23 4"] (the "2" joins with "3 4")
///
/// This differs from pure literal words which are never IFS-split.
pub fn smart_word_split(
    segments: &[WordSplitSegment],
    ifs_chars: &str,
) -> SmartWordSplitResult {
    // Check if there's any splittable segment
    let has_any_splittable = segments.iter().any(|s| s.is_splittable);

    // If there's no splittable expansion, return the joined value as-is
    // (pure literals are not subject to IFS splitting)
    if !has_any_splittable {
        let joined: String = segments.iter().map(|s| s.value.as_str()).collect();
        return SmartWordSplitResult {
            words: if joined.is_empty() { vec![] } else { vec![joined] },
        };
    }

    // Now do the smart word splitting:
    // - Splittable parts get split by IFS
    // - Non-splittable parts (quoted, literals) join with adjacent fields
    let mut words: Vec<String> = Vec::new();
    let mut current_word = String::new();
    let mut has_produced_word = false;
    let mut pending_word_break = false;
    let mut prev_was_quoted_empty = false;

    for segment in segments {
        if !segment.is_splittable {
            // Non-splittable: append to current word (no splitting)
            if pending_word_break {
                if segment.is_quoted && segment.value.is_empty() {
                    // Quoted empty after trailing IFS delimiter: push current word and an empty word
                    if !current_word.is_empty() {
                        words.push(current_word);
                        current_word = String::new();
                    }
                    // The quoted empty anchors an empty word
                    words.push(String::new());
                    has_produced_word = true;
                    pending_word_break = false;
                    prev_was_quoted_empty = true;
                } else if !segment.value.is_empty() {
                    // Non-empty content: push current word (if any) and start new word
                    if !current_word.is_empty() {
                        words.push(current_word);
                        current_word = String::new();
                    }
                    current_word = segment.value.clone();
                    pending_word_break = false;
                    prev_was_quoted_empty = false;
                } else {
                    // Empty non-quoted segment with pending break: just append (noop)
                    current_word.push_str(&segment.value);
                    prev_was_quoted_empty = false;
                }
            } else {
                current_word.push_str(&segment.value);
                prev_was_quoted_empty = segment.is_quoted && segment.value.is_empty();
            }
        } else {
            // Splittable: split by IFS using extended version that tracks trailing delimiters
            let IfsExpansionSplitResult {
                words: parts,
                had_leading_delimiter,
                had_trailing_delimiter,
            } = split_by_ifs_for_expansion_ex(&segment.value, ifs_chars);

            // If the previous segment was a quoted empty and this splittable segment
            // has leading IFS delimiter, the quoted empty should anchor an empty word
            if prev_was_quoted_empty && had_leading_delimiter && current_word.is_empty() {
                words.push(String::new());
                has_produced_word = true;
            }

            if parts.is_empty() {
                // Empty expansion produces nothing - continue building current word
                if had_trailing_delimiter {
                    pending_word_break = true;
                }
            } else if parts.len() == 1 {
                // Single result: just append to current word
                current_word.push_str(&parts[0]);
                has_produced_word = true;
                pending_word_break = had_trailing_delimiter;
            } else {
                // Multiple results from split:
                // - First part joins with current word
                // - Middle parts become separate words
                // - Last part starts the new current word
                current_word.push_str(&parts[0]);
                words.push(current_word);
                has_produced_word = true;

                // Add middle parts as separate words
                for part in &parts[1..parts.len() - 1] {
                    words.push(part.clone());
                }

                // Last part becomes the new current word
                current_word = parts[parts.len() - 1].clone();
                pending_word_break = had_trailing_delimiter;
            }
            prev_was_quoted_empty = false;
        }
    }

    // Add the remaining current word
    if !current_word.is_empty() {
        words.push(current_word);
    } else if words.is_empty() && has_produced_word {
        // The only content was from a split that produced [""] (empty string)
        words.push(String::new());
    }

    SmartWordSplitResult { words }
}

/// Check if a string starts with an IFS character.
pub fn starts_with_ifs(value: &str, ifs_chars: &str) -> bool {
    if let Some(first_char) = value.chars().next() {
        ifs_chars.contains(first_char)
    } else {
        false
    }
}

/// Word splitting for default value parts where Literal parts ARE splittable.
/// This is used when processing ${var:-"a b" c} where the default value has
/// mixed quoted and unquoted parts. The unquoted Literal parts should be split.
pub fn smart_word_split_with_unquoted_literals(
    segments: &[WordSplitSegment],
    ifs_chars: &str,
) -> SmartWordSplitResult {
    let mut words: Vec<String> = Vec::new();
    let mut current_word = String::new();
    let mut has_produced_word = false;
    let mut pending_word_break = false;

    for segment in segments {
        if !segment.is_splittable {
            // Non-splittable (quoted): append to current word
            if pending_word_break && !segment.value.is_empty() {
                if !current_word.is_empty() {
                    words.push(current_word);
                    current_word = String::new();
                }
                current_word = segment.value.clone();
                pending_word_break = false;
            } else {
                current_word.push_str(&segment.value);
            }
        } else {
            // Splittable: check if it starts with IFS (causes word break)
            let starts_with_ifs_char = starts_with_ifs(&segment.value, ifs_chars);

            // If the segment starts with IFS and we have accumulated content,
            // finish the current word first
            if starts_with_ifs_char && !current_word.is_empty() {
                words.push(current_word);
                current_word = String::new();
                has_produced_word = true;
            }

            // Split by IFS using extended version
            let IfsExpansionSplitResult {
                words: parts,
                had_trailing_delimiter,
                ..
            } = split_by_ifs_for_expansion_ex(&segment.value, ifs_chars);

            if parts.is_empty() {
                // Empty expansion produces nothing
                if had_trailing_delimiter {
                    pending_word_break = true;
                }
            } else if parts.len() == 1 {
                current_word.push_str(&parts[0]);
                has_produced_word = true;
                pending_word_break = had_trailing_delimiter;
            } else {
                // Multiple results from split
                current_word.push_str(&parts[0]);
                words.push(current_word);
                has_produced_word = true;

                for part in &parts[1..parts.len() - 1] {
                    words.push(part.clone());
                }

                current_word = parts[parts.len() - 1].clone();
                pending_word_break = had_trailing_delimiter;
            }
        }
    }

    if !current_word.is_empty() {
        words.push(current_word);
    } else if words.is_empty() && has_produced_word {
        words.push(String::new());
    }

    SmartWordSplitResult { words }
}

/// Simple IFS-based word splitting for a single string.
/// This is the basic word splitting used for unquoted expansions.
pub fn simple_word_split(value: &str, env: &HashMap<String, String>) -> Vec<String> {
    let ifs_chars = get_ifs(env);
    let result = split_by_ifs_for_expansion_ex(value, ifs_chars);
    result.words
}

/// Check if a word part is splittable (subject to IFS splitting).
/// Unquoted parameter expansions, command substitutions, and arithmetic expansions
/// are splittable. Quoted parts (DoubleQuoted, SingleQuoted) are NOT splittable.
pub fn is_part_splittable(part: &WordPart) -> bool {
    match part {
        // Quoted parts are never splittable
        WordPart::DoubleQuoted(_) | WordPart::SingleQuoted(_) => false,

        // Literal parts are not splittable (they join with adjacent fields)
        WordPart::Literal(_) => false,

        // Escaped parts are not splittable
        WordPart::Escaped(_) => false,

        // Glob parts are splittable only if they contain variable references
        // e.g., +($ABC) where ABC contains IFS characters should be split
        WordPart::Glob(g) => glob_pattern_has_var_ref(&g.pattern),

        // Parameter expansion is splittable unless its operation word is entirely quoted
        WordPart::ParameterExpansion(pe) => {
            // Word splitting behavior depends on whether the default value is entirely quoted:
            //
            // - ${v:-"AxBxC"} - entirely quoted default value, should NOT be split
            //   The quotes protect the entire default value from word splitting.
            //
            // - ${v:-x"AxBxC"x} - mixed quoted/unquoted parts, SHOULD be split
            //   The unquoted parts (x) act as potential word boundaries when containing IFS chars.
            //   The quoted part "AxBxC" is protected from internal splitting.
            //
            // - ${v:-AxBxC} - entirely unquoted, SHOULD be split
            //   All IFS chars in the result cause word boundaries.
            !is_operation_word_entirely_quoted(pe)
        }

        // Command substitution is always splittable
        WordPart::CommandSubstitution(_) => true,

        // Arithmetic expansion is always splittable
        WordPart::ArithmeticExpansion(_) => true,

        // Process substitution is not splittable (produces a filename)
        WordPart::ProcessSubstitution(_) => false,

        // Brace expansion is not splittable at this stage
        WordPart::BraceExpansion(_) => false,

        // Tilde expansion is not splittable
        WordPart::TildeExpansion(_) => false,
    }
}

/// Check if a DoubleQuoted part contains only simple literals (no expansions).
/// This is used to determine if special IFS handling is needed.
pub fn is_simple_quoted_literal(part: &WordPart) -> bool {
    match part {
        WordPart::SingleQuoted(_) => true, // Single quotes always contain only literals
        WordPart::DoubleQuoted(dq) => {
            // Check that all parts inside the double quotes are literals
            dq.parts.iter().all(|p| matches!(p, WordPart::Literal(_)))
        }
        _ => false,
    }
}

/// Get the word parts from an operation if it has a word field (DefaultValue, AssignDefault, UseAlternative, ErrorIfUnset)
pub fn get_operation_word_parts(op: &ParameterOperation) -> Option<&Vec<WordPart>> {
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

/// Check if a ParameterExpansion has a default/alternative value with mixed quoted/unquoted parts.
/// These need special handling to preserve quote boundaries during IFS splitting.
///
/// This function returns non-null only when:
/// 1. The default value has mixed quoted and unquoted parts
/// 2. The quoted parts contain only simple literals (no $@, $*, or other expansions)
///
/// Cases like ${var:-"$@"x} should NOT use special handling because $@ has special
/// behavior that needs to be preserved.
pub fn has_mixed_quoted_default_value(part: &ParameterExpansionPart) -> Option<&Vec<WordPart>> {
    let op = part.operation.as_ref()?;
    let op_word_parts = get_operation_word_parts(op)?;

    if op_word_parts.len() <= 1 {
        return None;
    }

    // Check if the operation word has simple quoted parts (only literals inside)
    let has_simple_quoted_parts = op_word_parts.iter().any(is_simple_quoted_literal);
    let has_unquoted_parts = op_word_parts.iter().any(|p| {
        matches!(
            p,
            WordPart::Literal(_)
                | WordPart::ParameterExpansion(_)
                | WordPart::CommandSubstitution(_)
                | WordPart::ArithmeticExpansion(_)
        )
    });

    // Only apply special handling when we have simple quoted literals and unquoted parts
    // This handles cases like ${var:-"2_3"x_x"4_5"} where the IFS char should only
    // split at the unquoted underscore, not inside the quoted strings
    if has_simple_quoted_parts && has_unquoted_parts {
        Some(op_word_parts)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_word_split() {
        let env = HashMap::new(); // Default IFS is " \t\n"
        let result = simple_word_split("hello world", &env);
        assert_eq!(result, vec!["hello", "world"]);
    }

    #[test]
    fn test_simple_word_split_custom_ifs() {
        let mut env = HashMap::new();
        env.insert("IFS".to_string(), ":".to_string());
        let result = simple_word_split("a:b:c", &env);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_smart_word_split_no_splittable() {
        let segments = vec![
            WordSplitSegment {
                value: "hello".to_string(),
                is_splittable: false,
                is_quoted: false,
            },
            WordSplitSegment {
                value: " world".to_string(),
                is_splittable: false,
                is_quoted: true,
            },
        ];
        let result = smart_word_split(&segments, " \t\n");
        assert_eq!(result.words, vec!["hello world"]);
    }

    #[test]
    fn test_smart_word_split_with_splittable() {
        // Simulates: $a"$b" where a="1 2" and b="3 4"
        let segments = vec![
            WordSplitSegment {
                value: "1 2".to_string(),
                is_splittable: true,
                is_quoted: false,
            },
            WordSplitSegment {
                value: "3 4".to_string(),
                is_splittable: false,
                is_quoted: true,
            },
        ];
        let result = smart_word_split(&segments, " \t\n");
        assert_eq!(result.words, vec!["1", "23 4"]);
    }

    #[test]
    fn test_smart_word_split_multiple_splits() {
        // Simulates: $a$b where a="1 2" and b="3 4"
        let segments = vec![
            WordSplitSegment {
                value: "1 2".to_string(),
                is_splittable: true,
                is_quoted: false,
            },
            WordSplitSegment {
                value: "3 4".to_string(),
                is_splittable: true,
                is_quoted: false,
            },
        ];
        let result = smart_word_split(&segments, " \t\n");
        assert_eq!(result.words, vec!["1", "23", "4"]);
    }

    #[test]
    fn test_starts_with_ifs() {
        assert!(starts_with_ifs(" hello", " \t\n"));
        assert!(starts_with_ifs("\thello", " \t\n"));
        assert!(!starts_with_ifs("hello", " \t\n"));
        assert!(!starts_with_ifs("", " \t\n"));
    }

    #[test]
    fn test_smart_word_split_empty_quoted() {
        // Simulates: $a"" where a="1 2"
        let segments = vec![
            WordSplitSegment {
                value: "1 2".to_string(),
                is_splittable: true,
                is_quoted: false,
            },
            WordSplitSegment {
                value: "".to_string(),
                is_splittable: false,
                is_quoted: true,
            },
        ];
        let result = smart_word_split(&segments, " \t\n");
        assert_eq!(result.words, vec!["1", "2"]);
    }
}
