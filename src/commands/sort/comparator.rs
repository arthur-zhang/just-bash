// src/commands/sort/comparator.rs
use std::cmp::Ordering;

/// Comparison mode for sorting
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompareMode {
    String,
    Numeric,
    HumanNumeric,
    Version,
    Month,
}

/// Per-key sort options (modifiers that can appear on a key spec)
#[derive(Debug, Clone)]
pub struct KeyOptions {
    pub ignore_leading_blanks: bool,
    pub dictionary_order: bool,
    pub ignore_case: bool,
    pub human_numeric: bool,
    pub month_sort: bool,
    pub numeric: bool,
    pub reverse: bool,
    pub version_sort: bool,
}

impl Default for KeyOptions {
    fn default() -> Self {
        Self {
            ignore_leading_blanks: false,
            dictionary_order: false,
            ignore_case: false,
            human_numeric: false,
            month_sort: false,
            numeric: false,
            reverse: false,
            version_sort: false,
        }
    }
}

/// A key specification parsed from -k KEYDEF
#[derive(Debug, Clone)]
pub struct KeySpec {
    pub start_field: usize,  // 1-indexed
    pub start_char: usize,   // 1-indexed, 0 means whole field
    pub end_field: usize,    // 1-indexed, 0 means end of line
    pub end_char: usize,     // 1-indexed, 0 means end of field
    pub options: KeyOptions,
}

/// Global sort options
#[derive(Debug, Clone)]
pub struct SortOptions {
    pub reverse: bool,
    pub numeric: bool,
    pub unique: bool,
    pub ignore_case: bool,
    pub human_numeric: bool,
    pub version_sort: bool,
    pub dictionary_order: bool,
    pub month_sort: bool,
    pub ignore_leading_blanks: bool,
    pub stable: bool,
    pub check: bool,
    pub output_file: Option<String>,
    pub keys: Vec<KeySpec>,
    pub field_separator: Option<char>,
}

impl Default for SortOptions {
    fn default() -> Self {
        Self {
            reverse: false,
            numeric: false,
            unique: false,
            ignore_case: false,
            human_numeric: false,
            version_sort: false,
            dictionary_order: false,
            month_sort: false,
            ignore_leading_blanks: false,
            stable: false,
            check: false,
            output_file: None,
            keys: Vec::new(),
            field_separator: None,
        }
    }
}

/// Parse a key position part like "2" or "2.3" or "2n" or "2.3nr"
/// Returns (field, char, options_chars)
fn parse_key_position(s: &str) -> (usize, usize, String) {
    let mut num_str = String::new();
    let mut char_str = String::new();
    let mut opts = String::new();
    let mut in_char = false;
    let mut past_nums = false;

    for c in s.chars() {
        if c == '.' && !in_char && !past_nums {
            in_char = true;
        } else if c.is_ascii_digit() && !past_nums {
            if in_char {
                char_str.push(c);
            } else {
                num_str.push(c);
            }
        } else {
            past_nums = true;
            opts.push(c);
        }
    }

    let field = num_str.parse::<usize>().unwrap_or(0);
    let char_pos = if char_str.is_empty() {
        0
    } else {
        char_str.parse::<usize>().unwrap_or(0)
    };

    (field, char_pos, opts)
}

/// Parse option characters into KeyOptions
fn parse_key_options(opts: &str) -> KeyOptions {
    let mut key_opts = KeyOptions::default();
    for c in opts.chars() {
        match c {
            'b' => key_opts.ignore_leading_blanks = true,
            'd' => key_opts.dictionary_order = true,
            'f' => key_opts.ignore_case = true,
            'h' => key_opts.human_numeric = true,
            'M' => key_opts.month_sort = true,
            'n' => key_opts.numeric = true,
            'r' => key_opts.reverse = true,
            'V' => key_opts.version_sort = true,
            _ => {}
        }
    }
    key_opts
}

/// Parse a KEYDEF string like "2", "2,2", "2n", "1.2,3.4nr"
pub fn parse_key_spec(keydef: &str) -> KeySpec {
    let parts: Vec<&str> = keydef.splitn(2, ',').collect();

    let (start_field, start_char, start_opts) = parse_key_position(parts[0]);

    let (end_field, end_char, end_opts) = if parts.len() > 1 {
        parse_key_position(parts[1])
    } else {
        (0, 0, String::new())
    };

    // Merge options: start opts take precedence, then end opts
    let combined_opts = format!("{}{}", start_opts, end_opts);
    let options = parse_key_options(&combined_opts);

    KeySpec {
        start_field,
        start_char,
        end_field,
        end_char,
        options,
    }
}

/// Split a line into fields using the given separator
fn split_fields<'a>(line: &'a str, separator: Option<char>) -> Vec<&'a str> {
    match separator {
        Some(sep) => line.split(sep).collect(),
        None => line.split_whitespace().collect(),
    }
}

/// Extract the key substring from a line based on a KeySpec
pub fn extract_key(line: &str, key: &KeySpec, separator: Option<char>) -> String {
    let fields = split_fields(line, separator);

    if fields.is_empty() || key.start_field == 0 {
        return line.to_string();
    }

    let start_idx = key.start_field.saturating_sub(1);
    if start_idx >= fields.len() {
        return String::new();
    }

    let end_idx = if key.end_field == 0 {
        fields.len() - 1
    } else {
        (key.end_field.saturating_sub(1)).min(fields.len() - 1)
    };

    if start_idx > end_idx {
        return String::new();
    }

    // Simple case: just join the fields in range
    if key.start_char == 0 && key.end_char == 0 {
        let result: Vec<&str> = fields[start_idx..=end_idx].to_vec();
        return result.join(" ");
    }

    // Handle character positions within fields
    let mut result = String::new();
    for (i, &field) in fields[start_idx..=end_idx].iter().enumerate() {
        let actual_idx = start_idx + i;
        let start_c = if actual_idx == start_idx && key.start_char > 0 {
            (key.start_char - 1).min(field.len())
        } else {
            0
        };
        let end_c = if actual_idx == end_idx && key.end_char > 0 {
            key.end_char.min(field.len())
        } else {
            field.len()
        };

        if start_c < end_c {
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(&field[start_c..end_c]);
        }
    }

    result
}

/// Determine the comparison mode from key options or global options
fn get_compare_mode(key_opts: &KeyOptions, global: &SortOptions) -> CompareMode {
    // Key-level options override global
    if key_opts.numeric {
        CompareMode::Numeric
    } else if key_opts.human_numeric {
        CompareMode::HumanNumeric
    } else if key_opts.version_sort {
        CompareMode::Version
    } else if key_opts.month_sort {
        CompareMode::Month
    } else if global.numeric {
        CompareMode::Numeric
    } else if global.human_numeric {
        CompareMode::HumanNumeric
    } else if global.version_sort {
        CompareMode::Version
    } else if global.month_sort {
        CompareMode::Month
    } else {
        CompareMode::String
    }
}

/// Check if case should be ignored
fn should_ignore_case(key_opts: &KeyOptions, global: &SortOptions) -> bool {
    key_opts.ignore_case || global.ignore_case
}

/// Check if result should be reversed
fn should_reverse(key_opts: &KeyOptions, global: &SortOptions) -> bool {
    // Key-level reverse XOR global reverse
    key_opts.reverse ^ global.reverse
}

/// Check if dictionary order applies
fn should_dictionary_order(key_opts: &KeyOptions, global: &SortOptions) -> bool {
    key_opts.dictionary_order || global.dictionary_order
}

/// Parse a month abbreviation to a number (0 for unknown)
pub fn compare_months(a: &str, b: &str) -> Ordering {
    fn month_num(s: &str) -> u32 {
        let trimmed = s.trim().to_uppercase();
        let prefix = if trimmed.len() >= 3 {
            &trimmed[..3]
        } else {
            &trimmed
        };
        match prefix {
            "JAN" => 1,
            "FEB" => 2,
            "MAR" => 3,
            "APR" => 4,
            "MAY" => 5,
            "JUN" => 6,
            "JUL" => 7,
            "AUG" => 8,
            "SEP" => 9,
            "OCT" => 10,
            "NOV" => 11,
            "DEC" => 12,
            _ => 0,
        }
    }
    month_num(a).cmp(&month_num(b))
}

/// Parse a human-readable size string like "1K", "2M", "3G"
fn parse_human_size(s: &str) -> f64 {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return 0.0;
    }

    let last = trimmed.chars().last().unwrap();
    let multiplier = match last.to_ascii_uppercase() {
        'K' => 1024.0,
        'M' => 1024.0 * 1024.0,
        'G' => 1024.0 * 1024.0 * 1024.0,
        'T' => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => {
            return trimmed.parse::<f64>().unwrap_or(0.0);
        }
    };

    let num_part = &trimmed[..trimmed.len() - 1];
    num_part.parse::<f64>().unwrap_or(0.0) * multiplier
}

/// Compare human-readable sizes
pub fn compare_human_sizes(a: &str, b: &str) -> Ordering {
    let va = parse_human_size(a);
    let vb = parse_human_size(b);
    va.partial_cmp(&vb).unwrap_or(Ordering::Equal)
}

/// Compare version strings (natural sort)
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts_a = split_version(a);
    let parts_b = split_version(b);

    for (pa, pb) in parts_a.iter().zip(parts_b.iter()) {
        let ord = match (pa, pb) {
            (VersionPart::Num(na), VersionPart::Num(nb)) => na.cmp(nb),
            (VersionPart::Str(sa), VersionPart::Str(sb)) => sa.cmp(sb),
            (VersionPart::Num(_), VersionPart::Str(_)) => Ordering::Less,
            (VersionPart::Str(_), VersionPart::Num(_)) => Ordering::Greater,
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }

    parts_a.len().cmp(&parts_b.len())
}

#[derive(Debug)]
enum VersionPart {
    Num(u64),
    Str(String),
}

fn split_version(s: &str) -> Vec<VersionPart> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_num = false;

    for c in s.chars() {
        let is_digit = c.is_ascii_digit();
        if current.is_empty() {
            in_num = is_digit;
            current.push(c);
        } else if is_digit == in_num {
            current.push(c);
        } else {
            if in_num {
                parts.push(VersionPart::Num(
                    current.parse().unwrap_or(0),
                ));
            } else {
                parts.push(VersionPart::Str(current.clone()));
            }
            current.clear();
            current.push(c);
            in_num = is_digit;
        }
    }

    if !current.is_empty() {
        if in_num {
            parts.push(VersionPart::Num(
                current.parse().unwrap_or(0),
            ));
        } else {
            parts.push(VersionPart::Str(current));
        }
    }

    parts
}

/// Apply dictionary order filter: keep only blanks and alphanumeric
fn dictionary_filter(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect()
}

/// Compare two values based on the comparison mode
pub fn compare_values(a: &str, b: &str, mode: CompareMode, ignore_case: bool, dict_order: bool) -> Ordering {
    let a_val = if dict_order { dictionary_filter(a) } else { a.to_string() };
    let b_val = if dict_order { dictionary_filter(b) } else { b.to_string() };

    match mode {
        CompareMode::String => {
            if ignore_case {
                a_val.to_lowercase().cmp(&b_val.to_lowercase())
            } else {
                a_val.cmp(&b_val)
            }
        }
        CompareMode::Numeric => {
            let na = a_val.trim().parse::<f64>().unwrap_or(0.0);
            let nb = b_val.trim().parse::<f64>().unwrap_or(0.0);
            na.partial_cmp(&nb).unwrap_or(Ordering::Equal)
        }
        CompareMode::HumanNumeric => {
            compare_human_sizes(&a_val, &b_val)
        }
        CompareMode::Version => {
            compare_versions(&a_val, &b_val)
        }
        CompareMode::Month => {
            compare_months(&a_val, &b_val)
        }
    }
}

/// Create a comparator function that compares two lines based on sort options
pub fn create_comparator(opts: &SortOptions) -> Box<dyn Fn(&str, &str) -> Ordering + '_> {
    Box::new(move |a: &str, b: &str| {
        if !opts.keys.is_empty() {
            // Compare using key specifications
            for key in &opts.keys {
                let key_a = extract_key(a, key, opts.field_separator);
                let key_b = extract_key(b, key, opts.field_separator);

                let mode = get_compare_mode(&key.options, opts);
                let ic = should_ignore_case(&key.options, opts);
                let dict = should_dictionary_order(&key.options, opts);

                let mut ord = compare_values(&key_a, &key_b, mode, ic, dict);

                if should_reverse(&key.options, opts) {
                    ord = ord.reverse();
                }

                if ord != Ordering::Equal {
                    return ord;
                }
            }

            // If all keys are equal and not stable, fall back to whole-line
            if opts.stable {
                return Ordering::Equal;
            }

            // Whole-line fallback comparison (no reverse applied)
            a.cmp(b)
        } else {
            // No keys: compare whole lines
            let mode = if opts.numeric {
                CompareMode::Numeric
            } else if opts.human_numeric {
                CompareMode::HumanNumeric
            } else if opts.version_sort {
                CompareMode::Version
            } else if opts.month_sort {
                CompareMode::Month
            } else {
                CompareMode::String
            };

            let mut ord = compare_values(
                a,
                b,
                mode,
                opts.ignore_case,
                opts.dictionary_order,
            );

            if opts.reverse {
                ord = ord.reverse();
            }

            ord
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_spec_simple() {
        let key = parse_key_spec("2");
        assert_eq!(key.start_field, 2);
        assert_eq!(key.end_field, 0);
    }

    #[test]
    fn test_parse_key_spec_range() {
        let key = parse_key_spec("2,4");
        assert_eq!(key.start_field, 2);
        assert_eq!(key.end_field, 4);
    }

    #[test]
    fn test_parse_key_spec_with_options() {
        let key = parse_key_spec("2n,3r");
        assert_eq!(key.start_field, 2);
        assert_eq!(key.end_field, 3);
        assert!(key.options.numeric);
        assert!(key.options.reverse);
    }

    #[test]
    fn test_parse_key_spec_with_char() {
        let key = parse_key_spec("1.2,3.4");
        assert_eq!(key.start_field, 1);
        assert_eq!(key.start_char, 2);
        assert_eq!(key.end_field, 3);
        assert_eq!(key.end_char, 4);
    }

    #[test]
    fn test_extract_key_simple() {
        let key = parse_key_spec("2");
        let result = extract_key("a b c", &key, None);
        assert_eq!(result, "b c");
    }

    #[test]
    fn test_extract_key_with_separator() {
        let key = parse_key_spec("2,2");
        let result = extract_key("a:b:c", &key, Some(':'));
        assert_eq!(result, "b");
    }

    #[test]
    fn test_compare_months() {
        assert_eq!(compare_months("Jan", "Feb"), Ordering::Less);
        assert_eq!(compare_months("DEC", "jan"), Ordering::Greater);
        assert_eq!(compare_months("mar", "MAR"), Ordering::Equal);
    }

    #[test]
    fn test_compare_human_sizes() {
        assert_eq!(compare_human_sizes("1K", "1M"), Ordering::Less);
        assert_eq!(compare_human_sizes("2G", "1G"), Ordering::Greater);
        assert_eq!(compare_human_sizes("100", "100"), Ordering::Equal);
    }

    #[test]
    fn test_compare_versions() {
        assert_eq!(compare_versions("1.0", "1.1"), Ordering::Less);
        assert_eq!(compare_versions("2.0", "1.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.10", "1.9"), Ordering::Greater);
    }

    #[test]
    fn test_compare_values_string() {
        assert_eq!(compare_values("abc", "abd", CompareMode::String, false, false), Ordering::Less);
        assert_eq!(compare_values("ABC", "abc", CompareMode::String, true, false), Ordering::Equal);
    }

    #[test]
    fn test_compare_values_numeric() {
        assert_eq!(compare_values("10", "9", CompareMode::Numeric, false, false), Ordering::Greater);
        assert_eq!(compare_values("2.5", "2.5", CompareMode::Numeric, false, false), Ordering::Equal);
    }
}
