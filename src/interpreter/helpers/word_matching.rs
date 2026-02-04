//! Word matching utility functions.
//!
//! Standalone helper functions used by the interpreter.

use crate::{WordNode, WordPart, LiteralPart};

/// Check if a WordNode is a literal match for any of the given strings.
/// Returns true only if the word is a single literal (no expansions, no quoting)
/// that matches one of the target strings.
///
/// This is used to detect assignment builtins at "parse time" - bash determines
/// whether a command is export/declare/etc based on the literal token, not the
/// runtime value after expansion.
pub fn is_word_literal_match(word: &WordNode, targets: &[&str]) -> bool {
    // Must be a single part
    if word.parts.len() != 1 {
        return false;
    }
    let part = &word.parts[0];
    // Must be a simple literal (not quoted, not an expansion)
    match part {
        WordPart::Literal(LiteralPart { value }) => targets.contains(&value.as_str()),
        _ => false,
    }
}

/// Parsed read-write file descriptor content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RwFdContent {
    pub path: String,
    pub position: usize,
    pub content: String,
}

/// Parse the content of a read-write file descriptor.
/// Format: __rw__:pathLength:path:position:content
/// Returns the parsed components, or None if format is invalid.
pub fn parse_rw_fd_content(fd_content: &str) -> Option<RwFdContent> {
    let after_prefix = fd_content.strip_prefix("__rw__:")?;

    // Parse pathLength
    let first_colon_idx = after_prefix.find(':')?;
    let path_length: usize = after_prefix[..first_colon_idx].parse().ok()?;

    // Extract path using length
    let path_start = first_colon_idx + 1;
    if path_start + path_length > after_prefix.len() {
        return None;
    }
    let path = after_prefix[path_start..path_start + path_length].to_string();

    // Parse position (after path and colon)
    let position_start = path_start + path_length + 1; // +1 for ":"
    if position_start >= after_prefix.len() {
        return None;
    }
    let remaining = &after_prefix[position_start..];
    let pos_colon_idx = remaining.find(':')?;
    let position: usize = remaining[..pos_colon_idx].parse().ok()?;

    // Extract content (after position and colon)
    let content = remaining[pos_colon_idx + 1..].to_string();

    Some(RwFdContent { path, position, content })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_word_literal_match() {
        let word = WordNode {
            parts: vec![WordPart::Literal(LiteralPart { value: "export".to_string() })],
        };
        assert!(is_word_literal_match(&word, &["export", "declare", "local"]));
        assert!(!is_word_literal_match(&word, &["declare", "local"]));
    }

    #[test]
    fn test_is_word_literal_match_multiple_parts() {
        let word = WordNode {
            parts: vec![
                WordPart::Literal(LiteralPart { value: "ex".to_string() }),
                WordPart::Literal(LiteralPart { value: "port".to_string() }),
            ],
        };
        assert!(!is_word_literal_match(&word, &["export"]));
    }

    #[test]
    fn test_parse_rw_fd_content() {
        let content = "__rw__:9:/tmp/file:0:hello world";
        let parsed = parse_rw_fd_content(content).unwrap();
        assert_eq!(parsed.path, "/tmp/file");
        assert_eq!(parsed.position, 0);
        assert_eq!(parsed.content, "hello world");
    }

    #[test]
    fn test_parse_rw_fd_content_with_position() {
        let content = "__rw__:9:/tmp/file:5:world";
        let parsed = parse_rw_fd_content(content).unwrap();
        assert_eq!(parsed.path, "/tmp/file");
        assert_eq!(parsed.position, 5);
        assert_eq!(parsed.content, "world");
    }

    #[test]
    fn test_parse_rw_fd_content_invalid() {
        assert!(parse_rw_fd_content("not_rw_content").is_none());
        assert!(parse_rw_fd_content("__rw__:").is_none());
        assert!(parse_rw_fd_content("__rw__:abc:path:0:content").is_none());
    }

    #[test]
    fn test_parse_rw_fd_content_path_with_colon() {
        // Path: /tmp:file (contains colon, length is 10)
        let content = "__rw__:10:/tmp:file/:0:content";
        let parsed = parse_rw_fd_content(content).unwrap();
        assert_eq!(parsed.path, "/tmp:file/");
        assert_eq!(parsed.position, 0);
        assert_eq!(parsed.content, "content");
    }
}
