//! Helper functions for parsing primary arithmetic expressions

use crate::ast::types::{ArithExpr, ArithNumberNode};

/// Skip whitespace in arithmetic expression input.
/// Also handles line continuations (backslash followed by newline).
pub fn skip_arith_whitespace(input: &str, mut pos: usize) -> usize {
    let chars: Vec<char> = input.chars().collect();
    while pos < chars.len() {
        // Skip line continuations (backslash followed by newline)
        if chars[pos] == '\\' && chars.get(pos + 1) == Some(&'\n') {
            pos += 2;
            continue;
        }
        // Skip regular whitespace
        if chars[pos].is_whitespace() {
            pos += 1;
            continue;
        }
        break;
    }
    pos
}

/// Assignment operators in arithmetic expressions
pub const ARITH_ASSIGN_OPS: &[&str] = &[
    "=", "+=", "-=", "*=", "/=", "%=",
    "<<=", ">>=", "&=", "|=", "^=",
];

/// Parse a number string with various bases (decimal, hex, octal, base#num)
/// Returns None for invalid numbers.
/// Note: Negative numbers are not handled here - the minus sign is a unary operator.
pub fn parse_arith_number(s: &str) -> Option<i64> {
    // Reject negative numbers - minus is handled as unary operator
    if s.starts_with('-') {
        return None;
    }

    // Handle base#num format
    // Bash supports bases 2-64 with digits: 0-9, a-z (10-35), A-Z (36-61), @ (62), _ (63)
    if s.contains('#') {
        let parts: Vec<&str> = s.splitn(2, '#').collect();
        if parts.len() != 2 {
            return None;
        }
        let base: u32 = parts[0].parse().ok()?;
        if base < 2 || base > 64 {
            return None;
        }
        let num_str = parts[1];
        
        // For bases <= 36, we can use i64::from_str_radix
        if base <= 36 {
            return i64::from_str_radix(num_str, base).ok();
        }
        
        // For bases 37-64, manually calculate
        let mut result: i64 = 0;
        for ch in num_str.chars() {
            let digit_value = match ch {
                '0'..='9' => ch as i64 - '0' as i64,
                'a'..='z' => ch as i64 - 'a' as i64 + 10,
                'A'..='Z' => ch as i64 - 'A' as i64 + 36,
                '@' => 62,
                '_' => 63,
                _ => return None,
            };
            if digit_value >= base as i64 {
                return None;
            }
            result = result * base as i64 + digit_value;
        }
        return Some(result);
    }
    
    // Handle hex (0x or 0X prefix)
    if s.starts_with("0x") || s.starts_with("0X") {
        return i64::from_str_radix(&s[2..], 16).ok();
    }
    
    // Handle octal (leading 0, but not just "0")
    if s.starts_with('0') && s.len() > 1 && s.chars().all(|c| c.is_ascii_digit()) {
        // If it looks like octal but has 8 or 9, it's an error
        if s.chars().any(|c| c == '8' || c == '9') {
            return None;
        }
        return i64::from_str_radix(s, 8).ok();
    }
    
    // Decimal
    s.parse().ok()
}

/// Result type for nested arithmetic parsing
pub struct NestedArithResult {
    pub expr: ArithExpr,
    pub pos: usize,
}

/// Parse nested arithmetic expression: $((expr))
pub fn parse_nested_arithmetic<F>(
    parse_fn: F,
    input: &str,
    current_pos: usize,
) -> Option<NestedArithResult>
where
    F: Fn(&str, usize) -> Option<(ArithExpr, usize)>,
{
    let chars: Vec<char> = input.chars().collect();
    if current_pos + 3 > chars.len() {
        return None;
    }
    
    // Check for $((
    if chars[current_pos] != '$' 
        || chars.get(current_pos + 1) != Some(&'(')
        || chars.get(current_pos + 2) != Some(&'(')
    {
        return None;
    }
    
    let mut pos = current_pos + 3;
    let mut depth = 1;
    let expr_start = pos;
    
    while pos < chars.len() - 1 && depth > 0 {
        if chars[pos] == '(' && chars.get(pos + 1) == Some(&'(') {
            depth += 1;
            pos += 2;
        } else if chars[pos] == ')' && chars.get(pos + 1) == Some(&')') {
            depth -= 1;
            if depth > 0 {
                pos += 2;
            }
        } else {
            pos += 1;
        }
    }
    
    let nested_expr: String = chars[expr_start..pos].iter().collect();
    let (expr, _) = parse_fn(&nested_expr, 0)?;
    pos += 2; // Skip ))
    
    Some(NestedArithResult {
        expr: ArithExpr::Nested(Box::new(crate::ast::types::ArithNestedNode {
            expression: expr,
        })),
        pos,
    })
}

/// Result type for ANSI-C quoting parsing
pub struct AnsiCResult {
    pub expr: ArithExpr,
    pub pos: usize,
}

/// Parse ANSI-C quoting: $'...'
/// Returns the numeric value of the string content
pub fn parse_ansi_c_quoting(input: &str, current_pos: usize) -> Option<AnsiCResult> {
    let chars: Vec<char> = input.chars().collect();
    if current_pos + 2 > chars.len() {
        return None;
    }
    
    // Check for $'
    if chars[current_pos] != '$' || chars.get(current_pos + 1) != Some(&'\'') {
        return None;
    }
    
    let mut pos = current_pos + 2; // Skip $'
    let mut content = String::new();
    
    while pos < chars.len() && chars[pos] != '\'' {
        if chars[pos] == '\\' && pos + 1 < chars.len() {
            let next_char = chars[pos + 1];
            match next_char {
                'n' => content.push('\n'),
                't' => content.push('\t'),
                'r' => content.push('\r'),
                '\\' => content.push('\\'),
                '\'' => content.push('\''),
                _ => content.push(next_char),
            }
            pos += 2;
        } else {
            content.push(chars[pos]);
            pos += 1;
        }
    }
    
    if pos < chars.len() && chars[pos] == '\'' {
        pos += 1; // Skip closing '
    }
    
    let num_value = content.parse::<i64>().unwrap_or(0);
    
    Some(AnsiCResult {
        expr: ArithExpr::Number(ArithNumberNode { value: num_value }),
        pos,
    })
}

/// Result type for localization quoting parsing
pub struct LocalizationResult {
    pub expr: ArithExpr,
    pub pos: usize,
}

/// Parse localization quoting: $"..."
/// Returns the numeric value of the string content
pub fn parse_localization_quoting(input: &str, current_pos: usize) -> Option<LocalizationResult> {
    let chars: Vec<char> = input.chars().collect();
    if current_pos + 2 > chars.len() {
        return None;
    }
    
    // Check for $"
    if chars[current_pos] != '$' || chars.get(current_pos + 1) != Some(&'"') {
        return None;
    }
    
    let mut pos = current_pos + 2; // Skip $"
    let mut content = String::new();
    
    while pos < chars.len() && chars[pos] != '"' {
        if chars[pos] == '\\' && pos + 1 < chars.len() {
            content.push(chars[pos + 1]);
            pos += 2;
        } else {
            content.push(chars[pos]);
            pos += 1;
        }
    }
    
    if pos < chars.len() && chars[pos] == '"' {
        pos += 1; // Skip closing "
    }
    
    let num_value = content.parse::<i64>().unwrap_or(0);
    
    Some(LocalizationResult {
        expr: ArithExpr::Number(ArithNumberNode { value: num_value }),
        pos,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_decimal() {
        assert_eq!(parse_arith_number("42"), Some(42));
        assert_eq!(parse_arith_number("0"), Some(0));
        assert_eq!(parse_arith_number("-1"), None); // Negative handled elsewhere
    }

    #[test]
    fn test_parse_hex() {
        assert_eq!(parse_arith_number("0x10"), Some(16));
        assert_eq!(parse_arith_number("0XFF"), Some(255));
        assert_eq!(parse_arith_number("0xAB"), Some(171));
    }

    #[test]
    fn test_parse_octal() {
        assert_eq!(parse_arith_number("010"), Some(8));
        assert_eq!(parse_arith_number("077"), Some(63));
        assert_eq!(parse_arith_number("089"), None); // Invalid octal
    }

    #[test]
    fn test_parse_base_notation() {
        assert_eq!(parse_arith_number("2#1010"), Some(10));
        assert_eq!(parse_arith_number("16#FF"), Some(255));
        assert_eq!(parse_arith_number("8#77"), Some(63));
    }

    #[test]
    fn test_skip_whitespace() {
        assert_eq!(skip_arith_whitespace("  abc", 0), 2);
        assert_eq!(skip_arith_whitespace("abc", 0), 0);
        assert_eq!(skip_arith_whitespace("\\\nabc", 0), 2);
    }
}
