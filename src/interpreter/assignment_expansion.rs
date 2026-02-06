//! Assignment Expansion Helpers
//!
//! Handles expansion of assignment arguments for local/declare/typeset builtins.
//! - Array assignments: name=(elem1 elem2 ...)
//! - Scalar assignments: name=value, name+=value, name[index]=value

use crate::ast::types::{WordNode, WordPart, LiteralPart, SingleQuotedPart, DoubleQuotedPart, EscapedPart, ParameterExpansionPart};
use crate::interpreter::types::InterpreterContext;

/// Check if a Word represents an array assignment (name=(...)) and expand it
/// while preserving quote structure for elements.
/// Returns the expanded string like "name=(elem1 elem2 ...)" or None if not an array assignment.
pub fn expand_local_array_assignment(
    ctx: &mut InterpreterContext,
    word: &WordNode,
) -> Option<String> {
    // First, join all parts to check if this looks like an array assignment
    let full_literal: String = word.parts.iter()
        .map(|p| {
            if let WordPart::Literal(LiteralPart { value }) = p {
                value.clone()
            } else {
                "\x00".to_string()
            }
        })
        .collect();

    // Check for array assignment pattern: name=(...)
    let array_re = regex_lite::Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)=\(").ok()?;
    let captures = array_re.captures(&full_literal)?;

    if !full_literal.ends_with(')') {
        return None;
    }

    let name = captures.get(1)?.as_str();
    let mut elements: Vec<String> = Vec::new();
    let mut in_array_content = false;
    let mut pending_literal = String::new();
    // Track whether we've seen a quoted part (SingleQuoted, DoubleQuoted) since
    // last element push. This ensures empty quoted strings like '' are preserved.
    let mut has_quoted_content = false;

    for part in &word.parts {
        match part {
            WordPart::Literal(LiteralPart { value }) => {
                let mut val = value.clone();
                if !in_array_content {
                    if let Some(idx) = val.find("=(") {
                        in_array_content = true;
                        val = val[idx + 2..].to_string();
                    }
                }

                if in_array_content {
                    if val.ends_with(')') {
                        val = val[..val.len() - 1].to_string();
                    }

                    // Split by whitespace but preserve separators (like TS split(/(\s+)/))
                    let ws_re = regex_lite::Regex::new(r"(\s+)").unwrap();
                    let tokens: Vec<&str> = ws_re.split(&val).collect();
                    let separators: Vec<&str> = ws_re.find_iter(&val).map(|m| m.as_str()).collect();

                    for (i, token) in tokens.iter().enumerate() {
                        if !token.is_empty() {
                            pending_literal.push_str(token);
                        }
                        // If there's a separator after this token, push pending element
                        if i < separators.len() {
                            if !pending_literal.is_empty() || has_quoted_content {
                                elements.push(pending_literal.clone());
                                pending_literal.clear();
                                has_quoted_content = false;
                            }
                        }
                    }
                }
            }
            _ if in_array_content => {
                match part {
                    WordPart::SingleQuoted(SingleQuotedPart { value }) => {
                        has_quoted_content = true;
                        pending_literal.push_str(value);
                    }
                    WordPart::DoubleQuoted(DoubleQuotedPart { parts }) => {
                        has_quoted_content = true;
                        for inner_part in parts {
                            pending_literal.push_str(&expand_word_part_simple(ctx, inner_part));
                        }
                    }
                    WordPart::Escaped(EscapedPart { value }) => {
                        has_quoted_content = true;
                        pending_literal.push_str(value);
                    }
                    WordPart::ParameterExpansion(ParameterExpansionPart { parameter, .. }) => {
                        if let Some(val) = ctx.state.env.get(parameter) {
                            pending_literal.push_str(val);
                        }
                    }
                    _ => {
                        pending_literal.push_str(&expand_word_part_simple(ctx, part));
                    }
                }
            }
            _ => {}
        }
    }

    // Push final element if we have content OR saw quoted part
    if !pending_literal.is_empty() || has_quoted_content {
        elements.push(pending_literal);
    }

    // Build result string with proper quoting
    let keyed_re = regex_lite::Regex::new(r"^\[.+\]=").unwrap();
    let quoted_elements: Vec<String> = elements.iter()
        .map(|elem| {
            // Don't quote keyed elements like ['key']=value or [index]=value
            if keyed_re.is_match(elem) {
                return elem.clone();
            }
            if elem.is_empty() {
                return "''".to_string();
            }
            if elem.chars().any(|c| " \t\n\"'\\$`!*?[]{}|&;<>()".contains(c))
                && !elem.starts_with('\'')
                && !elem.starts_with('"')
            {
                return format!("'{}'", elem.replace('\'', "'\\''"));
            }
            elem.clone()
        })
        .collect();

    Some(format!("{}=({})", name, quoted_elements.join(" ")))
}

/// Check if a Word represents a scalar assignment (name=value, name+=value, or name[index]=value)
/// and expand it WITHOUT glob expansion on the value part.
/// Returns the expanded string like "name=expanded_value" or None if not a scalar assignment.
pub fn expand_scalar_assignment_arg(
    ctx: &mut InterpreterContext,
    word: &WordNode,
) -> Option<String> {
    // Look for = in the word parts to detect assignment pattern
    let mut eq_part_index: Option<usize> = None;
    let mut eq_char_index: Option<usize> = None;
    let mut is_append = false;

    let var_re = regex_lite::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").ok()?;
    let array_re = regex_lite::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*\[[^\]]+\]$").ok()?;

    for (i, part) in word.parts.iter().enumerate() {
        if let WordPart::Literal(LiteralPart { value }) = part {
            // Check for += first
            if let Some(append_idx) = value.find("+=") {
                let before = &value[..append_idx];
                if var_re.is_match(before) || array_re.is_match(before) {
                    eq_part_index = Some(i);
                    eq_char_index = Some(append_idx);
                    is_append = true;
                    break;
                }
            }

            // Check for regular =
            if let Some(eq_idx) = value.find('=') {
                if eq_idx == 0 || value.chars().nth(eq_idx - 1) != Some('+') {
                    let before = &value[..eq_idx];
                    if var_re.is_match(before) || array_re.is_match(before) {
                        eq_part_index = Some(i);
                        eq_char_index = Some(eq_idx);
                        break;
                    }
                }
            }
        }
    }

    let eq_part_idx = eq_part_index?;
    let eq_char_idx = eq_char_index?;

    let eq_part = &word.parts[eq_part_idx];
    let WordPart::Literal(LiteralPart { value: eq_value }) = eq_part else {
        return None;
    };

    let operator_len = if is_append { 2 } else { 1 };
    let name_from_eq_part = &eq_value[..eq_char_idx];
    let value_from_eq_part = &eq_value[eq_char_idx + operator_len..];

    // Construct the name
    let mut name = String::new();
    for part in &word.parts[..eq_part_idx] {
        name.push_str(&expand_word_part_simple(ctx, part));
    }
    name.push_str(name_from_eq_part);

    // Construct the value
    let mut value = String::from(value_from_eq_part);
    for part in &word.parts[eq_part_idx + 1..] {
        value.push_str(&expand_word_part_simple(ctx, part));
    }

    let operator = if is_append { "+=" } else { "=" };
    Some(format!("{}{}{}", name, operator, value))
}

/// Simple word part expansion
fn expand_word_part_simple(ctx: &InterpreterContext, part: &WordPart) -> String {
    match part {
        WordPart::Literal(LiteralPart { value }) => value.clone(),
        WordPart::SingleQuoted(SingleQuotedPart { value }) => value.clone(),
        WordPart::Escaped(EscapedPart { value }) => value.clone(),
        WordPart::ParameterExpansion(ParameterExpansionPart { parameter, .. }) => {
            ctx.state.env.get(parameter).cloned().unwrap_or_default()
        }
        WordPart::DoubleQuoted(DoubleQuotedPart { parts }) => {
            parts.iter()
                .map(|p| expand_word_part_simple(ctx, p))
                .collect()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::types::{InterpreterState, ExecutionLimits};

    fn make_ctx() -> (InterpreterState, ExecutionLimits) {
        (InterpreterState::default(), ExecutionLimits::default())
    }

    #[test]
    fn test_expand_scalar_assignment_simple() {
        let (mut state, limits) = make_ctx();
        let mut ctx = InterpreterContext::new(&mut state, &limits);

        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart { value: "foo=bar".to_string() }),
            ],
        };

        let result = expand_scalar_assignment_arg(&mut ctx, &word);
        assert_eq!(result, Some("foo=bar".to_string()));
    }

    #[test]
    fn test_expand_scalar_assignment_with_variable() {
        let (mut state, limits) = make_ctx();
        state.env.insert("x".to_string(), "hello".to_string());
        let mut ctx = InterpreterContext::new(&mut state, &limits);

        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart { value: "foo=".to_string() }),
                WordPart::ParameterExpansion(ParameterExpansionPart {
                    parameter: "x".to_string(),
                    operation: None,
                }),
            ],
        };

        let result = expand_scalar_assignment_arg(&mut ctx, &word);
        assert_eq!(result, Some("foo=hello".to_string()));
    }

    #[test]
    fn test_not_an_assignment() {
        let (mut state, limits) = make_ctx();
        let mut ctx = InterpreterContext::new(&mut state, &limits);

        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart { value: "echo".to_string() }),
            ],
        };

        let result = expand_scalar_assignment_arg(&mut ctx, &word);
        assert_eq!(result, None);
    }

    #[test]
    fn test_array_assignment_simple() {
        let (mut state, limits) = make_ctx();
        let mut ctx = InterpreterContext::new(&mut state, &limits);

        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart { value: "arr=(a b c)".to_string() }),
            ],
        };

        let result = expand_local_array_assignment(&mut ctx, &word);
        assert_eq!(result, Some("arr=(a b c)".to_string()));
    }

    #[test]
    fn test_array_assignment_empty_quoted_string() {
        let (mut state, limits) = make_ctx();
        let mut ctx = InterpreterContext::new(&mut state, &limits);

        // arr=('' "")  — empty single-quoted and double-quoted strings
        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart { value: "arr=(".to_string() }),
                WordPart::SingleQuoted(SingleQuotedPart { value: "".to_string() }),
                WordPart::Literal(LiteralPart { value: " ".to_string() }),
                WordPart::DoubleQuoted(DoubleQuotedPart { parts: vec![] }),
                WordPart::Literal(LiteralPart { value: ")".to_string() }),
            ],
        };

        let result = expand_local_array_assignment(&mut ctx, &word);
        assert_eq!(result, Some("arr=('' '')".to_string()));
    }

    #[test]
    fn test_array_assignment_with_variable() {
        let (mut state, limits) = make_ctx();
        state.env.insert("x".to_string(), "hello world".to_string());
        let mut ctx = InterpreterContext::new(&mut state, &limits);

        // arr=($x)
        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart { value: "arr=(".to_string() }),
                WordPart::ParameterExpansion(ParameterExpansionPart {
                    parameter: "x".to_string(),
                    operation: None,
                }),
                WordPart::Literal(LiteralPart { value: ")".to_string() }),
            ],
        };

        let result = expand_local_array_assignment(&mut ctx, &word);
        // The variable expands to "hello world" which becomes a single pending element
        assert_eq!(result, Some("arr=('hello world')".to_string()));
    }

    #[test]
    fn test_array_assignment_keyed_element() {
        let (mut state, limits) = make_ctx();
        let mut ctx = InterpreterContext::new(&mut state, &limits);

        // arr=([0]=foo [1]=bar)
        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart { value: "arr=([0]=foo [1]=bar)".to_string() }),
            ],
        };

        let result = expand_local_array_assignment(&mut ctx, &word);
        assert_eq!(result, Some("arr=([0]=foo [1]=bar)".to_string()));
    }

    #[test]
    fn test_scalar_assignment_append() {
        let (mut state, limits) = make_ctx();
        let mut ctx = InterpreterContext::new(&mut state, &limits);

        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart { value: "foo+=bar".to_string() }),
            ],
        };

        let result = expand_scalar_assignment_arg(&mut ctx, &word);
        assert_eq!(result, Some("foo+=bar".to_string()));
    }

    #[test]
    fn test_scalar_assignment_array_index() {
        let (mut state, limits) = make_ctx();
        let mut ctx = InterpreterContext::new(&mut state, &limits);

        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart { value: "arr[0]=val".to_string() }),
            ],
        };

        let result = expand_scalar_assignment_arg(&mut ctx, &word);
        assert_eq!(result, Some("arr[0]=val".to_string()));
    }
}
