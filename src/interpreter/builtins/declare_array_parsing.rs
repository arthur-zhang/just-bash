//! Array Parsing Functions for declare/typeset
//!
//! Handles parsing of array literal syntax for the declare builtin.

/// Parse array elements from content like "1 2 3" or "'a b' c d"
pub fn parse_array_elements(content: &str) -> Vec<String> {
    let mut elements: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;
    // Track whether we've seen content that should result in an element,
    // including empty quoted strings like '' or ""
    let mut has_content = false;

    for ch in content.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            has_content = true;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '\'' && !in_double_quote {
            // Entering or leaving single quotes - either way, this indicates an element exists
            if !in_single_quote {
                // Entering quotes - mark that we have content (even if empty)
                has_content = true;
            }
            in_single_quote = !in_single_quote;
            continue;
        }
        if ch == '"' && !in_single_quote {
            // Entering or leaving double quotes - either way, this indicates an element exists
            if !in_double_quote {
                // Entering quotes - mark that we have content (even if empty)
                has_content = true;
            }
            in_double_quote = !in_double_quote;
            continue;
        }
        if (ch == ' ' || ch == '\t' || ch == '\n') && !in_single_quote && !in_double_quote {
            if has_content {
                elements.push(current);
                current = String::new();
                has_content = false;
            }
            continue;
        }
        current.push(ch);
        has_content = true;
    }
    if has_content {
        elements.push(current);
    }
    elements
}

/// Parse associative array literal content like "['foo']=bar ['spam']=42"
/// Returns array of (key, value) pairs
pub fn parse_assoc_array_literal(content: &str) -> Vec<(String, String)> {
    let mut entries: Vec<(String, String)> = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let mut pos = 0;

    while pos < chars.len() {
        // Skip whitespace
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        if pos >= chars.len() {
            break;
        }

        // Expect [
        if chars[pos] != '[' {
            // Skip non-bracket content
            pos += 1;
            continue;
        }
        pos += 1; // skip [

        // Parse key (may be quoted)
        let mut key = String::new();
        if pos < chars.len() && (chars[pos] == '\'' || chars[pos] == '"') {
            let quote = chars[pos];
            pos += 1;
            while pos < chars.len() && chars[pos] != quote {
                key.push(chars[pos]);
                pos += 1;
            }
            if pos < chars.len() && chars[pos] == quote {
                pos += 1;
            }
        } else {
            while pos < chars.len() && chars[pos] != ']' && chars[pos] != '=' {
                key.push(chars[pos]);
                pos += 1;
            }
        }

        // Skip to ]
        while pos < chars.len() && chars[pos] != ']' {
            pos += 1;
        }
        if pos < chars.len() && chars[pos] == ']' {
            pos += 1;
        }

        // Expect =
        if pos >= chars.len() || chars[pos] != '=' {
            continue;
        }
        pos += 1;

        // Parse value (may be quoted)
        let mut value = String::new();
        if pos < chars.len() && (chars[pos] == '\'' || chars[pos] == '"') {
            let quote = chars[pos];
            pos += 1;
            while pos < chars.len() && chars[pos] != quote {
                if chars[pos] == '\\' && pos + 1 < chars.len() {
                    pos += 1;
                    value.push(chars[pos]);
                } else {
                    value.push(chars[pos]);
                }
                pos += 1;
            }
            if pos < chars.len() && chars[pos] == quote {
                pos += 1;
            }
        } else {
            while pos < chars.len() && !chars[pos].is_whitespace() {
                value.push(chars[pos]);
                pos += 1;
            }
        }

        entries.push((key, value));
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_array_elements_simple() {
        let result = parse_array_elements("1 2 3");
        assert_eq!(result, vec!["1", "2", "3"]);
    }

    #[test]
    fn test_parse_array_elements_quoted() {
        let result = parse_array_elements("'a b' c d");
        assert_eq!(result, vec!["a b", "c", "d"]);
    }

    #[test]
    fn test_parse_array_elements_double_quoted() {
        let result = parse_array_elements("\"hello world\" foo");
        assert_eq!(result, vec!["hello world", "foo"]);
    }

    #[test]
    fn test_parse_array_elements_empty_quoted() {
        let result = parse_array_elements("'' foo ''");
        assert_eq!(result, vec!["", "foo", ""]);
    }

    #[test]
    fn test_parse_array_elements_escaped() {
        let result = parse_array_elements("a\\ b c");
        assert_eq!(result, vec!["a b", "c"]);
    }

    #[test]
    fn test_parse_array_elements_empty() {
        let result = parse_array_elements("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_array_elements_whitespace_only() {
        let result = parse_array_elements("   \t\n  ");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_assoc_array_literal_simple() {
        let result = parse_assoc_array_literal("[foo]=bar [baz]=qux");
        assert_eq!(result, vec![
            ("foo".to_string(), "bar".to_string()),
            ("baz".to_string(), "qux".to_string()),
        ]);
    }

    #[test]
    fn test_parse_assoc_array_literal_quoted_keys() {
        let result = parse_assoc_array_literal("['foo']=bar [\"baz\"]=qux");
        assert_eq!(result, vec![
            ("foo".to_string(), "bar".to_string()),
            ("baz".to_string(), "qux".to_string()),
        ]);
    }

    #[test]
    fn test_parse_assoc_array_literal_quoted_values() {
        let result = parse_assoc_array_literal("[foo]='hello world' [bar]=\"test\"");
        assert_eq!(result, vec![
            ("foo".to_string(), "hello world".to_string()),
            ("bar".to_string(), "test".to_string()),
        ]);
    }

    #[test]
    fn test_parse_assoc_array_literal_escaped_in_value() {
        let result = parse_assoc_array_literal("[foo]='it\\'s'");
        assert_eq!(result, vec![
            ("foo".to_string(), "it's".to_string()),
        ]);
    }

    #[test]
    fn test_parse_assoc_array_literal_empty() {
        let result = parse_assoc_array_literal("");
        assert!(result.is_empty());
    }
}
