//! Word Parsing Utilities
//!
//! String manipulation utilities for parsing words, expansions, and patterns.
//! These are pure functions extracted from the Parser class.

use crate::ast::types::{
    ArithExpr, ArithNumberNode, ArithmeticExpressionNode, BraceExpansionPart, BraceItem,
    BraceRangeValue, DoubleQuotedPart, EscapedPart, GlobPart, LiteralPart, RedirectionOperator,
    SingleQuotedPart, TildeExpansionPart, WordNode, WordPart, AST,
};
use crate::parser::arithmetic_parser::parse_arithmetic_expression;
use crate::parser::lexer::TokenType;

// =============================================================================
// PURE STRING UTILITIES
// =============================================================================

/// Decode a byte array as UTF-8 with error recovery.
/// Valid UTF-8 sequences are decoded to their Unicode characters.
/// Invalid bytes are preserved as Latin-1 characters (byte value = char code).
///
/// This matches bash's behavior for $'\xNN' sequences.
fn decode_utf8_with_recovery(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;

    while i < bytes.len() {
        let b0 = bytes[i];

        // ASCII (0xxxxxxx)
        if b0 < 0x80 {
            result.push(b0 as char);
            i += 1;
            continue;
        }

        // 2-byte sequence (110xxxxx 10xxxxxx)
        if (b0 & 0xe0) == 0xc0 {
            if i + 1 < bytes.len()
                && (bytes[i + 1] & 0xc0) == 0x80
                && b0 >= 0xc2
            // Reject overlong sequences
            {
                let code_point = ((b0 as u32 & 0x1f) << 6) | (bytes[i + 1] as u32 & 0x3f);
                if let Some(c) = char::from_u32(code_point) {
                    result.push(c);
                }
                i += 2;
                continue;
            }
            // Invalid or incomplete - output as Latin-1
            result.push(b0 as char);
            i += 1;
            continue;
        }

        // 3-byte sequence (1110xxxx 10xxxxxx 10xxxxxx)
        if (b0 & 0xf0) == 0xe0 {
            if i + 2 < bytes.len()
                && (bytes[i + 1] & 0xc0) == 0x80
                && (bytes[i + 2] & 0xc0) == 0x80
            {
                // Check for overlong encoding
                if b0 == 0xe0 && bytes[i + 1] < 0xa0 {
                    // Overlong - output first byte as Latin-1
                    result.push(b0 as char);
                    i += 1;
                    continue;
                }
                // Check for surrogate range (U+D800-U+DFFF)
                let code_point = ((b0 as u32 & 0x0f) << 12)
                    | ((bytes[i + 1] as u32 & 0x3f) << 6)
                    | (bytes[i + 2] as u32 & 0x3f);
                if (0xd800..=0xdfff).contains(&code_point) {
                    // Invalid surrogate - output first byte as Latin-1
                    result.push(b0 as char);
                    i += 1;
                    continue;
                }
                if let Some(c) = char::from_u32(code_point) {
                    result.push(c);
                }
                i += 3;
                continue;
            }
            // Invalid or incomplete - output as Latin-1
            result.push(b0 as char);
            i += 1;
            continue;
        }

        // 4-byte sequence (11110xxx 10xxxxxx 10xxxxxx 10xxxxxx)
        if (b0 & 0xf8) == 0xf0 && b0 <= 0xf4 {
            if i + 3 < bytes.len()
                && (bytes[i + 1] & 0xc0) == 0x80
                && (bytes[i + 2] & 0xc0) == 0x80
                && (bytes[i + 3] & 0xc0) == 0x80
            {
                // Check for overlong encoding
                if b0 == 0xf0 && bytes[i + 1] < 0x90 {
                    // Overlong - output first byte as Latin-1
                    result.push(b0 as char);
                    i += 1;
                    continue;
                }
                let code_point = ((b0 as u32 & 0x07) << 18)
                    | ((bytes[i + 1] as u32 & 0x3f) << 12)
                    | ((bytes[i + 2] as u32 & 0x3f) << 6)
                    | (bytes[i + 3] as u32 & 0x3f);
                // Check for valid range (U+10000 to U+10FFFF)
                if code_point > 0x10ffff {
                    // Invalid - output first byte as Latin-1
                    result.push(b0 as char);
                    i += 1;
                    continue;
                }
                if let Some(c) = char::from_u32(code_point) {
                    result.push(c);
                }
                i += 4;
                continue;
            }
            // Invalid or incomplete - output as Latin-1
            result.push(b0 as char);
            i += 1;
            continue;
        }

        // Invalid lead byte (10xxxxxx or 11111xxx) - output as Latin-1
        result.push(b0 as char);
        i += 1;
    }

    result
}

pub fn find_tilde_end(value: &str, start: usize) -> usize {
    let chars: Vec<char> = value.chars().collect();
    let mut i = start + 1;
    while i < chars.len()
        && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '-')
    {
        i += 1;
    }
    i
}

pub fn find_matching_bracket(value: &str, start: usize, open: char, close: char) -> isize {
    let chars: Vec<char> = value.chars().collect();
    let mut depth = 1;
    let mut i = start + 1;

    while i < chars.len() && depth > 0 {
        if chars[i] == open {
            depth += 1;
        } else if chars[i] == close {
            depth -= 1;
        }
        if depth > 0 {
            i += 1;
        }
    }

    if depth == 0 {
        i as isize
    } else {
        -1
    }
}

pub fn find_parameter_operation_end(value: &str, start: usize) -> usize {
    let chars: Vec<char> = value.chars().collect();
    let mut i = start;
    let mut depth = 1;

    while i < chars.len() && depth > 0 {
        let ch = chars[i];

        // Handle escape sequences - \X escapes the next character
        if ch == '\\' && i + 1 < chars.len() {
            i += 2; // Skip escape and the escaped character
            continue;
        }

        // Handle single quotes - content is literal
        if ch == '\'' {
            if let Some(close_idx) = chars[i + 1..].iter().position(|&c| c == '\'') {
                i = i + 1 + close_idx + 1;
                continue;
            }
        }

        // Handle double quotes - content with escapes
        if ch == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < chars.len() {
                i += 1; // Skip closing quote
            }
            continue;
        }

        if ch == '{' {
            depth += 1;
        } else if ch == '}' {
            depth -= 1;
        }
        if depth > 0 {
            i += 1;
        }
    }

    i
}

pub fn find_pattern_end(value: &str, start: usize) -> usize {
    let chars: Vec<char> = value.chars().collect();
    let mut i = start;

    // In bash, if the pattern starts with /, that / IS the pattern.
    // For ${x////c}: after //, the next / is the pattern, followed by / separator, then c
    // So we need to consume at least one character before treating / as a delimiter.
    let mut consumed_any = false;

    while i < chars.len() {
        let ch = chars[i];
        // Only break on / if we've consumed at least one character
        if (ch == '/' && consumed_any) || ch == '}' {
            break;
        }

        // Handle single quotes - skip until closing quote
        if ch == '\'' {
            if let Some(close_idx) = chars[i + 1..].iter().position(|&c| c == '\'') {
                i = i + 1 + close_idx + 1;
                consumed_any = true;
                continue;
            }
        }

        // Handle double quotes - skip until closing quote (handling escapes)
        if ch == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < chars.len() {
                i += 1; // Skip closing quote
            }
            consumed_any = true;
            continue;
        }

        if ch == '\\' {
            i += 2;
            consumed_any = true;
        } else {
            i += 1;
            consumed_any = true;
        }
    }

    i
}

pub fn parse_glob_pattern(value: &str, start: usize) -> (String, usize) {
    let chars: Vec<char> = value.chars().collect();
    let mut i = start;
    let mut pattern = String::new();

    while i < chars.len() {
        let ch = chars[i];

        if ch == '*' || ch == '?' {
            pattern.push(ch);
            i += 1;
        } else if ch == '[' {
            // Character class - need to properly find closing ]
            // Handle POSIX character classes like [[:alpha:]], [^[:alpha:]], etc.
            let close_idx = find_character_class_end(value, i);
            if close_idx == -1 {
                pattern.push(ch);
                i += 1;
            } else {
                let close_idx = close_idx as usize;
                for j in i..=close_idx {
                    pattern.push(chars[j]);
                }
                i = close_idx + 1;
            }
        } else {
            break;
        }
    }

    (pattern, i)
}

/// Find the closing ] of a character class, properly handling:
/// - POSIX character classes like [:alpha:], [:digit:], etc.
/// - Negation [^...]
/// - Literal ] at the start []] or [^]]
/// - Single quotes inside class (bash extension): [^'abc]'] contains literal ]
fn find_character_class_end(value: &str, start: usize) -> isize {
    let chars: Vec<char> = value.chars().collect();
    let mut i = start + 1; // Skip opening [

    // Handle negation
    if i < chars.len() && chars[i] == '^' {
        i += 1;
    }

    // A ] immediately after [ or [^ is literal, not closing
    if i < chars.len() && chars[i] == ']' {
        i += 1;
    }

    while i < chars.len() {
        let ch = chars[i];

        // Handle escape sequences
        // In bash, shell escaping takes precedence over character class escaping.
        // So \" inside a character class means the shell escaped the quote,
        // and this is NOT a valid character class (bash outputs ["] for [\"])
        // Only \] is valid inside a character class to include literal ]
        if ch == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            // If it's an escaped quote or shell special char, this is not a valid character class
            if next == '"' || next == '\'' {
                return -1;
            }
            i += 2; // Skip both the backslash and the escaped character
            continue;
        }

        if ch == ']' {
            return i as isize;
        }

        // If we encounter expansion or quote characters, this is NOT a valid glob
        // character class. In bash, ["$x"] is [ + "$x" + ], not a character class.
        if ch == '"' || ch == '$' || ch == '`' {
            return -1;
        }

        // Handle single quotes inside character class (bash extension)
        // [^'abc]'] - the ] inside quotes is literal, class ends at second ]
        if ch == '\'' {
            if let Some(close_quote) = chars[i + 1..].iter().position(|&c| c == '\'') {
                i = i + 1 + close_quote + 1;
                continue;
            }
        }

        // Handle POSIX character classes [:name:]
        if ch == '[' && i + 1 < chars.len() && chars[i + 1] == ':' {
            // Find closing :]
            let remaining: String = chars[i + 2..].iter().collect();
            if let Some(close_pos) = remaining.find(":]") {
                i = i + 2 + close_pos + 2;
                continue;
            }
        }

        // Handle collating symbols [.name.] and equivalence classes [=name=]
        if ch == '[' && i + 1 < chars.len() && (chars[i + 1] == '.' || chars[i + 1] == '=') {
            let close_char = chars[i + 1];
            let close_seq = format!("{}]", close_char);
            let remaining: String = chars[i + 2..].iter().collect();
            if let Some(close_pos) = remaining.find(&close_seq) {
                i = i + 2 + close_pos + 2;
                continue;
            }
        }

        i += 1;
    }

    -1 // No closing ] found
}

pub fn parse_ansi_c_quoted(value: &str, start: usize) -> (WordPart, usize) {
    let chars: Vec<char> = value.chars().collect();
    let mut result = String::new();
    let mut i = start;

    while i < chars.len() && chars[i] != '\'' {
        let ch = chars[i];

        if ch == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            match next {
                'n' => {
                    result.push('\n');
                    i += 2;
                }
                't' => {
                    result.push('\t');
                    i += 2;
                }
                'r' => {
                    result.push('\r');
                    i += 2;
                }
                '\\' => {
                    result.push('\\');
                    i += 2;
                }
                '\'' => {
                    result.push('\'');
                    i += 2;
                }
                '"' => {
                    result.push('"');
                    i += 2;
                }
                'a' => {
                    result.push('\x07'); // bell
                    i += 2;
                }
                'b' => {
                    result.push('\x08'); // backspace
                    i += 2;
                }
                'e' | 'E' => {
                    result.push('\x1b'); // escape
                    i += 2;
                }
                'f' => {
                    result.push('\x0c'); // form feed
                    i += 2;
                }
                'v' => {
                    result.push('\x0b'); // vertical tab
                    i += 2;
                }
                'x' => {
                    // \xHH - hex escape
                    // Collect consecutive \xHH escapes and decode as UTF-8 with error recovery
                    let mut bytes: Vec<u8> = Vec::new();
                    let mut j = i;
                    while j + 1 < chars.len() && chars[j] == '\\' && chars[j + 1] == 'x' {
                        let hex: String = chars[j + 2..].iter().take(2).collect();
                        if let Ok(code) = u8::from_str_radix(&hex, 16) {
                            bytes.push(code);
                            j += 2 + hex.len();
                        } else {
                            break;
                        }
                    }

                    if !bytes.is_empty() {
                        // Decode bytes as UTF-8 with error recovery
                        // Invalid bytes are preserved as Latin-1 characters
                        result.push_str(&decode_utf8_with_recovery(&bytes));
                        i = j;
                    } else {
                        result.push_str("\\x");
                        i += 2;
                    }
                }
                'u' => {
                    // \uHHHH - unicode escape
                    let hex: String = chars[i + 2..].iter().take(4).collect();
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        if let Some(c) = char::from_u32(code) {
                            result.push(c);
                            i += 6;
                        } else {
                            result.push_str("\\u");
                            i += 2;
                        }
                    } else {
                        result.push_str("\\u");
                        i += 2;
                    }
                }
                'c' => {
                    // \cX - control character escape
                    // Control char = X & 0x1f (mask with 31)
                    // For letters a-z/A-Z: ctrl-A=1, ctrl-Z=26
                    // For special chars: \c- = 0x0d (CR), \c+ = 0x0b (VT), \c" = 0x02
                    if i + 2 < chars.len() {
                        let ctrl_char = chars[i + 2];
                        let code = (ctrl_char as u8) & 0x1f;
                        result.push(code as char);
                        i += 3;
                    } else {
                        // Incomplete \c at end of string
                        result.push_str("\\c");
                        i += 2;
                    }
                }
                '0'..='7' => {
                    // \NNN - octal escape
                    let mut octal = String::new();
                    let mut j = i + 1;
                    while j < chars.len() && j < i + 4 && chars[j] >= '0' && chars[j] <= '7' {
                        octal.push(chars[j]);
                        j += 1;
                    }
                    if let Ok(code) = u8::from_str_radix(&octal, 8) {
                        result.push(code as char);
                    }
                    i = j;
                }
                _ => {
                    // Unknown escape, keep the backslash
                    result.push(ch);
                    i += 1;
                }
            }
        } else {
            result.push(ch);
            i += 1;
        }
    }

    // Skip closing quote
    if i < chars.len() && chars[i] == '\'' {
        i += 1;
    }

    (AST::literal(&result), i)
}

pub fn parse_arith_expr_from_string(input: &str) -> ArithmeticExpressionNode {
    // Trim whitespace - bash allows spaces around arithmetic expressions in slices
    let trimmed = input.trim();
    if trimmed.is_empty() {
        // Empty string means 0
        return ArithmeticExpressionNode {
            expression: ArithExpr::Number(ArithNumberNode { value: 0 }),
            original_text: None,
        };
    }
    // Use the full arithmetic expression parser
    parse_arithmetic_expression(trimmed)
}

/// Split a brace expansion inner content by commas at the top level.
/// Handles nested braces like {a,{b,c},d} correctly.
fn split_brace_items(inner: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for c in inner.chars() {
        if c == '{' {
            depth += 1;
            current.push(c);
        } else if c == '}' {
            depth -= 1;
            current.push(c);
        } else if c == ',' && depth == 0 {
            items.push(current);
            current = String::new();
        } else {
            current.push(c);
        }
    }
    items.push(current);
    items
}

/// Type alias for word parts parser function
pub type WordPartsParser = fn(&str, bool, bool, bool) -> Vec<WordPart>;

pub fn try_parse_brace_expansion(
    value: &str,
    start: usize,
    parse_word_parts_fn: Option<WordPartsParser>,
) -> Option<(WordPart, usize)> {
    // Find matching }
    let close_idx = find_matching_bracket(value, start, '{', '}');
    if close_idx == -1 {
        return None;
    }
    let close_idx = close_idx as usize;

    let chars: Vec<char> = value.chars().collect();
    let inner: String = chars[start + 1..close_idx].iter().collect();

    // Check for range: {a..z} or {1..10} or {1..10..2}
    // Numeric range pattern: -?\d+\.\.-?\d+(?:\.\.-?\d+)?
    if let Some(range_result) = try_parse_numeric_range(&inner) {
        return Some((
            WordPart::BraceExpansion(BraceExpansionPart {
                items: vec![range_result],
            }),
            close_idx + 1,
        ));
    }

    // Character ranges: {a..z} or {a..z..2}
    if let Some(range_result) = try_parse_char_range(&inner) {
        return Some((
            WordPart::BraceExpansion(BraceExpansionPart {
                items: vec![range_result],
            }),
            close_idx + 1,
        ));
    }

    // Check for comma-separated list: {a,b,c}
    if inner.contains(',') {
        if let Some(parse_fn) = parse_word_parts_fn {
            // Split by comma at top level (handling nested braces)
            let raw_items = split_brace_items(&inner);
            // Parse each item as a word with full expansion support
            let items: Vec<BraceItem> = raw_items
                .iter()
                .map(|s| BraceItem::Word {
                    word: AST::word(parse_fn(s, false, false, false)),
                })
                .collect();
            return Some((WordPart::BraceExpansion(BraceExpansionPart { items }), close_idx + 1));
        } else {
            // Legacy fallback: treat items as literals if no parser provided
            let raw_items = split_brace_items(&inner);
            let items: Vec<BraceItem> = raw_items
                .iter()
                .map(|s| BraceItem::Word {
                    word: AST::word(vec![AST::literal(s)]),
                })
                .collect();
            return Some((WordPart::BraceExpansion(BraceExpansionPart { items }), close_idx + 1));
        }
    }

    None
}

/// Try to parse a numeric range like {1..10} or {1..10..2}
fn try_parse_numeric_range(inner: &str) -> Option<BraceItem> {
    // Pattern: -?\d+\.\.-?\d+(?:\.\.-?\d+)?
    let parts: Vec<&str> = inner.split("..").collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }

    let start_num: i64 = parts[0].parse().ok()?;
    let end_num: i64 = parts[1].parse().ok()?;
    let step: Option<i64> = if parts.len() == 3 {
        Some(parts[2].parse().ok()?)
    } else {
        None
    };

    Some(BraceItem::Range {
        start: BraceRangeValue::Number(start_num),
        end: BraceRangeValue::Number(end_num),
        step,
        start_str: Some(parts[0].to_string()),
        end_str: Some(parts[1].to_string()),
    })
}

/// Try to parse a character range like {a..z} or {a..z..2}
fn try_parse_char_range(inner: &str) -> Option<BraceItem> {
    // Pattern: [a-zA-Z]\.\.[a-zA-Z](?:\.\.-?\d+)?
    let parts: Vec<&str> = inner.split("..").collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }

    // Check that start and end are single characters
    if parts[0].len() != 1 || parts[1].len() != 1 {
        return None;
    }

    let start_char = parts[0].chars().next()?;
    let end_char = parts[1].chars().next()?;

    // Must be alphabetic
    if !start_char.is_ascii_alphabetic() || !end_char.is_ascii_alphabetic() {
        return None;
    }

    let step: Option<i64> = if parts.len() == 3 {
        Some(parts[2].parse().ok()?)
    } else {
        None
    };

    Some(BraceItem::Range {
        start: BraceRangeValue::Char(start_char),
        end: BraceRangeValue::Char(end_char),
        step,
        start_str: None,
        end_str: None,
    })
}

/// Convert a WordNode back to a string representation.
/// Used for reconstructing array assignment strings for declare/local.
pub fn word_to_string(word: &WordNode) -> String {
    let mut result = String::new();
    for part in &word.parts {
        match part {
            WordPart::Literal(LiteralPart { value }) => {
                result.push_str(value);
            }
            WordPart::SingleQuoted(SingleQuotedPart { value }) => {
                // Preserve single quotes so empty strings like '' are not lost
                result.push('\'');
                result.push_str(value);
                result.push('\'');
            }
            WordPart::Escaped(EscapedPart { value }) => {
                result.push_str(value);
            }
            WordPart::DoubleQuoted(DoubleQuotedPart { parts }) => {
                // For double-quoted parts, reconstruct them
                result.push('"');
                for inner in parts {
                    match inner {
                        WordPart::Literal(LiteralPart { value })
                        | WordPart::Escaped(EscapedPart { value }) => {
                            result.push_str(value);
                        }
                        WordPart::ParameterExpansion(exp) => {
                            result.push_str("${");
                            result.push_str(&exp.parameter);
                            result.push('}');
                        }
                        _ => {}
                    }
                }
                result.push('"');
            }
            WordPart::ParameterExpansion(exp) => {
                result.push_str("${");
                result.push_str(&exp.parameter);
                result.push('}');
            }
            WordPart::Glob(GlobPart { pattern }) => {
                result.push_str(pattern);
            }
            WordPart::TildeExpansion(TildeExpansionPart { user }) => {
                result.push('~');
                if let Some(u) = user {
                    result.push_str(u);
                }
            }
            WordPart::BraceExpansion(BraceExpansionPart { items }) => {
                // Reconstruct brace expansion syntax
                result.push('{');
                let brace_items: Vec<String> = items
                    .iter()
                    .map(|item| match item {
                        BraceItem::Range {
                            start,
                            end,
                            step,
                            start_str,
                            end_str,
                        } => {
                            // Reconstruct range: {start..end} or {start..end..step}
                            let start_val = start_str
                                .clone()
                                .unwrap_or_else(|| format!("{}", start));
                            let end_val = end_str
                                .clone()
                                .unwrap_or_else(|| format!("{}", end));
                            if let Some(s) = step {
                                format!("{}..{}..{}", start_val, end_val, s)
                            } else {
                                format!("{}..{}", start_val, end_val)
                            }
                        }
                        BraceItem::Word { word } => {
                            // Word item - recurse to convert the word
                            word_to_string(word)
                        }
                    })
                    .collect();
                // If there's only one item and it's a range, use the range syntax
                // Otherwise, join with commas for {a,b,c} syntax
                if brace_items.len() == 1 {
                    if let BraceItem::Range { .. } = &items[0] {
                        result.push_str(&brace_items[0]);
                    } else {
                        result.push_str(&brace_items.join(","));
                    }
                } else {
                    result.push_str(&brace_items.join(","));
                }
                result.push('}');
            }
            _ => {
                // For complex parts, just use a placeholder
                result.push_str(&format!("{:?}", part));
            }
        }
    }
    result
}

pub fn token_to_redirect_op(token_type: TokenType) -> RedirectionOperator {
    match token_type {
        TokenType::Less => RedirectionOperator::Less,
        TokenType::Great => RedirectionOperator::Great,
        TokenType::DGreat => RedirectionOperator::DGreat,
        TokenType::LessAnd => RedirectionOperator::LessAnd,
        TokenType::GreatAnd => RedirectionOperator::GreatAnd,
        TokenType::LessGreat => RedirectionOperator::LessGreat,
        TokenType::Clobber => RedirectionOperator::Clobber,
        TokenType::TLess => RedirectionOperator::TLess,
        TokenType::AndGreat => RedirectionOperator::AndGreat,
        TokenType::AndDGreat => RedirectionOperator::AndDGreat,
        TokenType::DLess => RedirectionOperator::Less,      // Here-doc operator is <
        TokenType::DLessDash => RedirectionOperator::Less,  // Here-doc operator is <
        _ => RedirectionOperator::Great,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_tilde_end() {
        assert_eq!(find_tilde_end("~user/path", 0), 5);
        assert_eq!(find_tilde_end("~/path", 0), 1);
        assert_eq!(find_tilde_end("~user-name/path", 0), 10);
    }

    #[test]
    fn test_find_matching_bracket() {
        assert_eq!(find_matching_bracket("{abc}", 0, '{', '}'), 4);
        assert_eq!(find_matching_bracket("{a{b}c}", 0, '{', '}'), 6);
        assert_eq!(find_matching_bracket("{abc", 0, '{', '}'), -1);
    }

    #[test]
    fn test_split_brace_items() {
        assert_eq!(split_brace_items("a,b,c"), vec!["a", "b", "c"]);
        assert_eq!(split_brace_items("a,{b,c},d"), vec!["a", "{b,c}", "d"]);
    }

    #[test]
    fn test_parse_ansi_c_quoted() {
        let (part, idx) = parse_ansi_c_quoted("hello\\nworld'rest", 0);
        assert_eq!(idx, 13);
        if let WordPart::Literal(LiteralPart { value }) = part {
            assert_eq!(value, "hello\nworld");
        } else {
            panic!("Expected Literal");
        }
    }

    #[test]
    fn test_try_parse_numeric_range() {
        let result = try_parse_numeric_range("1..10");
        assert!(result.is_some());
        if let Some(BraceItem::Range { start, end, step, .. }) = result {
            assert_eq!(start, BraceRangeValue::Number(1));
            assert_eq!(end, BraceRangeValue::Number(10));
            assert_eq!(step, None);
        }

        let result = try_parse_numeric_range("1..10..2");
        assert!(result.is_some());
        if let Some(BraceItem::Range { step, .. }) = result {
            assert_eq!(step, Some(2));
        }
    }

    #[test]
    fn test_try_parse_char_range() {
        let result = try_parse_char_range("a..z");
        assert!(result.is_some());
        if let Some(BraceItem::Range { start, end, step, .. }) = result {
            assert_eq!(start, BraceRangeValue::Char('a'));
            assert_eq!(end, BraceRangeValue::Char('z'));
            assert_eq!(step, None);
        }
    }

    #[test]
    fn test_decode_utf8_with_recovery() {
        // Valid UTF-8
        assert_eq!(decode_utf8_with_recovery(&[0x48, 0x65, 0x6c, 0x6c, 0x6f]), "Hello");
        // Invalid byte should be preserved as Latin-1
        assert_eq!(decode_utf8_with_recovery(&[0xff]), "\u{ff}");
    }
}
