//! Brace Range Expansion
//!
//! Handles numeric {1..10} and character {a..z} range expansion.
//! These are pure functions with no external dependencies.

use crate::interpreter::BraceExpansionError;

/// Maximum iterations for range expansion to prevent infinite loops
const MAX_SAFE_RANGE_ITERATIONS: usize = 10000;

/// Safely expand a numeric range with step, preventing infinite loops.
/// Returns array of string values, or None if the range is invalid.
///
/// Bash behavior:
/// - When step is 0, treat it as 1
/// - When step direction is "wrong", use absolute value and go in natural direction
/// - Zero-padding: use the max width of start/end for padding
fn safe_expand_numeric_range(
    start: i64,
    end: i64,
    raw_step: Option<i64>,
    start_str: Option<&str>,
    end_str: Option<&str>,
) -> Option<Vec<String>> {
    // Step of 0 is treated as 1 in bash
    let step = raw_step.unwrap_or(1);
    let step = if step == 0 { 1 } else { step };

    // Use absolute value of step - bash ignores step sign and uses natural direction
    let abs_step = step.abs();

    let mut results = Vec::new();

    // Determine zero-padding width (max width of start or end if leading zeros)
    let mut pad_width = 0usize;

    // Check if start_str has leading zeros (like "01" or "-01")
    if let Some(s) = start_str {
        let without_minus = s.trim_start_matches('-');
        if without_minus.len() > 1 && without_minus.starts_with('0') {
            pad_width = pad_width.max(without_minus.len());
        }
    }

    // Check if end_str has leading zeros
    if let Some(s) = end_str {
        let without_minus = s.trim_start_matches('-');
        if without_minus.len() > 1 && without_minus.starts_with('0') {
            pad_width = pad_width.max(without_minus.len());
        }
    }

    let format_num = |n: i64| -> String {
        if pad_width > 0 {
            let neg = n < 0;
            let abs_str = format!("{:0>width$}", n.abs(), width = pad_width);
            if neg {
                format!("-{}", abs_str)
            } else {
                abs_str
            }
        } else {
            n.to_string()
        }
    };

    if start <= end {
        // Ascending range
        let mut i = start;
        let mut count = 0;
        while i <= end && count < MAX_SAFE_RANGE_ITERATIONS {
            results.push(format_num(i));
            i += abs_step;
            count += 1;
        }
    } else {
        // Descending range (start > end)
        let mut i = start;
        let mut count = 0;
        while i >= end && count < MAX_SAFE_RANGE_ITERATIONS {
            results.push(format_num(i));
            i -= abs_step;
            count += 1;
        }
    }

    Some(results)
}

/// Safely expand a character range with step, preventing infinite loops.
/// Returns array of string values, or None if the range is invalid.
/// Returns Err for mixed case ranges (e.g., {z..A}).
///
/// Bash behavior:
/// - When step is 0, treat it as 1
/// - When step direction is "wrong", use absolute value and go in natural direction
/// - Mixed case (e.g., {z..A}) is an error
fn safe_expand_char_range(
    start: char,
    end: char,
    raw_step: Option<i64>,
) -> Result<Option<Vec<String>>, BraceExpansionError> {
    // Step of 0 is treated as 1 in bash
    let step = raw_step.unwrap_or(1);
    let step = if step == 0 { 1 } else { step };

    let start_code = start as i64;
    let end_code = end as i64;

    // Use absolute value of step - bash ignores step sign and uses natural direction
    let abs_step = step.abs();

    // Check for mixed case (upper to lower or vice versa) - invalid in bash
    let start_is_upper = start >= 'A' && start <= 'Z';
    let start_is_lower = start >= 'a' && start <= 'z';
    let end_is_upper = end >= 'A' && end <= 'Z';
    let end_is_lower = end >= 'a' && end <= 'z';

    if (start_is_upper && end_is_lower) || (start_is_lower && end_is_upper) {
        // Mixed case is an error in bash (produces no output, exit code 1)
        let step_part = raw_step.map(|s| format!("..{}", s)).unwrap_or_default();
        return Err(BraceExpansionError::simple(format!(
            "{{{}..{}{}}}",
            start, end, step_part
        )));
    }

    let mut results = Vec::new();

    if start_code <= end_code {
        // Ascending range
        let mut i = start_code;
        let mut count = 0;
        while i <= end_code && count < MAX_SAFE_RANGE_ITERATIONS {
            if let Some(c) = char::from_u32(i as u32) {
                results.push(c.to_string());
            }
            i += abs_step;
            count += 1;
        }
    } else {
        // Descending range
        let mut i = start_code;
        let mut count = 0;
        while i >= end_code && count < MAX_SAFE_RANGE_ITERATIONS {
            if let Some(c) = char::from_u32(i as u32) {
                results.push(c.to_string());
            }
            i -= abs_step;
            count += 1;
        }
    }

    Ok(Some(results))
}

/// Result of a brace range expansion.
/// Either contains expanded values or a literal fallback for invalid ranges.
#[derive(Debug, Clone)]
pub struct BraceRangeResult {
    pub expanded: Option<Vec<String>>,
    pub literal: String,
}

/// Range value - either numeric or character
#[derive(Debug, Clone)]
pub enum RangeValue {
    Numeric(i64),
    Char(char),
}

/// Unified brace range expansion helper.
/// Handles both numeric and character ranges, returning either expanded values
/// or a literal string for invalid ranges.
pub fn expand_brace_range(
    start: RangeValue,
    end: RangeValue,
    step: Option<i64>,
    start_str: Option<&str>,
    end_str: Option<&str>,
) -> Result<BraceRangeResult, BraceExpansionError> {
    let step_part = step.map(|s| format!("..{}", s)).unwrap_or_default();

    match (&start, &end) {
        (RangeValue::Numeric(s), RangeValue::Numeric(e)) => {
            let expanded = safe_expand_numeric_range(*s, *e, step, start_str, end_str);
            Ok(BraceRangeResult {
                expanded,
                literal: format!("{{{}..{}{}}}", s, e, step_part),
            })
        }
        (RangeValue::Char(s), RangeValue::Char(e)) => {
            let expanded = safe_expand_char_range(*s, *e, step)?;
            Ok(BraceRangeResult {
                expanded,
                literal: format!("{{{}..{}{}}}", s, e, step_part),
            })
        }
        _ => {
            // Mismatched types - treat as invalid
            let start_str = match &start {
                RangeValue::Numeric(n) => n.to_string(),
                RangeValue::Char(c) => c.to_string(),
            };
            let end_str = match &end {
                RangeValue::Numeric(n) => n.to_string(),
                RangeValue::Char(c) => c.to_string(),
            };
            Ok(BraceRangeResult {
                expanded: None,
                literal: format!("{{{}..{}{}}}", start_str, end_str, step_part),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numeric_range_ascending() {
        let result = expand_brace_range(
            RangeValue::Numeric(1),
            RangeValue::Numeric(5),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            result.expanded,
            Some(vec!["1", "2", "3", "4", "5"].into_iter().map(String::from).collect())
        );
    }

    #[test]
    fn test_numeric_range_descending() {
        let result = expand_brace_range(
            RangeValue::Numeric(5),
            RangeValue::Numeric(1),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            result.expanded,
            Some(vec!["5", "4", "3", "2", "1"].into_iter().map(String::from).collect())
        );
    }

    #[test]
    fn test_numeric_range_with_step() {
        let result = expand_brace_range(
            RangeValue::Numeric(1),
            RangeValue::Numeric(10),
            Some(2),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            result.expanded,
            Some(vec!["1", "3", "5", "7", "9"].into_iter().map(String::from).collect())
        );
    }

    #[test]
    fn test_numeric_range_zero_padding() {
        let result = expand_brace_range(
            RangeValue::Numeric(1),
            RangeValue::Numeric(3),
            None,
            Some("01"),
            Some("03"),
        )
        .unwrap();
        assert_eq!(
            result.expanded,
            Some(vec!["01", "02", "03"].into_iter().map(String::from).collect())
        );
    }

    #[test]
    fn test_char_range_ascending() {
        let result = expand_brace_range(
            RangeValue::Char('a'),
            RangeValue::Char('e'),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            result.expanded,
            Some(vec!["a", "b", "c", "d", "e"].into_iter().map(String::from).collect())
        );
    }

    #[test]
    fn test_char_range_descending() {
        let result = expand_brace_range(
            RangeValue::Char('e'),
            RangeValue::Char('a'),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            result.expanded,
            Some(vec!["e", "d", "c", "b", "a"].into_iter().map(String::from).collect())
        );
    }

    #[test]
    fn test_char_range_mixed_case_error() {
        let result = expand_brace_range(
            RangeValue::Char('a'),
            RangeValue::Char('Z'),
            None,
            None,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_step_zero_treated_as_one() {
        let result = expand_brace_range(
            RangeValue::Numeric(1),
            RangeValue::Numeric(3),
            Some(0),
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            result.expanded,
            Some(vec!["1", "2", "3"].into_iter().map(String::from).collect())
        );
    }
}
