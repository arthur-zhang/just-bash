/// Parsed URL components
struct ParsedUrl {
    origin: String,   // e.g., "https://api.example.com" or "http://localhost:3000"
    pathname: String,  // e.g., "/v1/users" or "/"
}

/// Parse a URL string into components. Returns None if invalid.
/// Supports http:// and https:// URLs.
/// Origin = scheme + "://" + host + optional_port
/// Pathname = path portion (defaults to "/" if none)
fn parse_url(url_string: &str) -> Option<ParsedUrl> {
    // Find scheme
    let scheme_end = url_string.find("://")?;
    let scheme = &url_string[..scheme_end];
    if scheme != "http" && scheme != "https" {
        // Still parse it, but validate_allow_list will reject non-http(s)
    }

    let after_scheme = &url_string[scheme_end + 3..];

    // Find host+port (up to first / or end of string)
    let (authority, pathname) = if let Some(slash_pos) = after_scheme.find('/') {
        (&after_scheme[..slash_pos], &after_scheme[slash_pos..])
    } else {
        (after_scheme, "/")
    };

    if authority.is_empty() {
        return None;
    }

    // Strip query string and fragment from pathname
    let pathname = pathname.split('?').next().unwrap_or("/");
    let pathname = pathname.split('#').next().unwrap_or("/");

    Some(ParsedUrl {
        origin: format!("{}://{}", scheme, authority),
        pathname: pathname.to_string(),
    })
}

/// Check if a URL matches an allow-list entry.
/// Rules:
/// 1. Origins must match exactly (case-sensitive for scheme and host)
/// 2. URL path must start with entry's path
/// 3. If entry has no path (or just "/"), all paths allowed
pub fn matches_allow_list_entry(url: &str, allowed_entry: &str) -> bool {
    let parsed_url = match parse_url(url) {
        Some(p) => p,
        None => return false,
    };
    let parsed_entry = match parse_url(allowed_entry) {
        Some(p) => p,
        None => return false,
    };

    // Origins must match exactly
    if parsed_url.origin != parsed_entry.origin {
        return false;
    }

    // If entry path is "/" or empty, allow all paths
    if parsed_entry.pathname == "/" || parsed_entry.pathname.is_empty() {
        return true;
    }

    // URL path must start with entry's path prefix
    parsed_url.pathname.starts_with(&parsed_entry.pathname)
}

/// Check if a URL is allowed by any entry in the allow-list.
pub fn is_url_allowed(url: &str, allowed_url_prefixes: &[String]) -> bool {
    if allowed_url_prefixes.is_empty() {
        return false;
    }
    allowed_url_prefixes
        .iter()
        .any(|entry| matches_allow_list_entry(url, entry))
}

/// Validate allow-list configuration. Returns error messages for invalid entries.
pub fn validate_allow_list(allowed_url_prefixes: &[String]) -> Vec<String> {
    let mut errors = Vec::new();
    for entry in allowed_url_prefixes {
        let parsed = parse_url(entry);
        if parsed.is_none() {
            errors.push(format!(
                "Invalid URL in allow-list: \"{}\" - must be a valid URL with scheme and host",
                entry
            ));
            continue;
        }

        // Check scheme
        let scheme = entry.split("://").next().unwrap_or("");
        if scheme != "http" && scheme != "https" {
            errors.push(format!(
                "Only http and https URLs are allowed in allow-list: \"{}\"",
                entry
            ));
            continue;
        }

        // Check for hostname
        let after_scheme = &entry[scheme.len() + 3..];
        let authority = after_scheme.split('/').next().unwrap_or("");
        if authority.is_empty() {
            errors.push(format!(
                "Allow-list entry must include a hostname: \"{}\"",
                entry
            ));
            continue;
        }

        // Warn about query strings and fragments
        if entry.contains('?') || entry.contains('#') {
            errors.push(format!(
                "Query strings and fragments are ignored in allow-list entries: \"{}\"",
                entry
            ));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    // parse_url tests
    #[test]
    fn test_parse_url_basic() {
        let p = parse_url("https://api.example.com/v1").unwrap();
        assert_eq!(p.origin, "https://api.example.com");
        assert_eq!(p.pathname, "/v1");
    }

    #[test]
    fn test_parse_url_with_port() {
        let p = parse_url("http://localhost:3000/api").unwrap();
        assert_eq!(p.origin, "http://localhost:3000");
        assert_eq!(p.pathname, "/api");
    }

    #[test]
    fn test_parse_url_no_path() {
        let p = parse_url("https://example.com").unwrap();
        assert_eq!(p.origin, "https://example.com");
        assert_eq!(p.pathname, "/");
    }

    #[test]
    fn test_parse_url_invalid() {
        assert!(parse_url("not-a-url").is_none());
        assert!(parse_url("").is_none());
    }

    #[test]
    fn test_parse_url_strips_query_and_fragment() {
        let p = parse_url("https://example.com/path?q=1#frag").unwrap();
        assert_eq!(p.pathname, "/path");
    }

    // matches_allow_list_entry tests
    #[test]
    fn test_matches_origin_all_paths() {
        assert!(matches_allow_list_entry(
            "https://api.example.com/v1/users",
            "https://api.example.com"
        ));
    }

    #[test]
    fn test_matches_path_prefix() {
        assert!(matches_allow_list_entry(
            "https://api.example.com/v1/users",
            "https://api.example.com/v1"
        ));
    }

    #[test]
    fn test_rejects_wrong_path() {
        assert!(!matches_allow_list_entry(
            "https://api.example.com/v2/users",
            "https://api.example.com/v1"
        ));
    }

    #[test]
    fn test_rejects_wrong_origin() {
        assert!(!matches_allow_list_entry(
            "https://other.com/v1",
            "https://api.example.com/v1"
        ));
    }

    #[test]
    fn test_trailing_slash_matters() {
        // /v1 does NOT start with /v1/ (trailing slash in entry)
        assert!(!matches_allow_list_entry(
            "https://api.example.com/v1",
            "https://api.example.com/v1/"
        ));
        // But /v1/foo does start with /v1/
        assert!(matches_allow_list_entry(
            "https://api.example.com/v1/foo",
            "https://api.example.com/v1/"
        ));
    }

    // is_url_allowed tests
    #[test]
    fn test_url_allowed_by_first_entry() {
        let prefixes = vec!["https://api.example.com".to_string()];
        assert!(is_url_allowed("https://api.example.com/v1", &prefixes));
    }

    #[test]
    fn test_url_allowed_by_second_entry() {
        let prefixes = vec![
            "https://other.com".to_string(),
            "https://api.example.com".to_string(),
        ];
        assert!(is_url_allowed("https://api.example.com/v1", &prefixes));
    }

    #[test]
    fn test_url_not_allowed() {
        let prefixes = vec!["https://api.example.com".to_string()];
        assert!(!is_url_allowed("https://evil.com/hack", &prefixes));
    }

    #[test]
    fn test_empty_allow_list() {
        assert!(!is_url_allowed("https://example.com", &[]));
    }

    // validate_allow_list tests
    #[test]
    fn test_validate_valid_entries() {
        let errors = validate_allow_list(&vec!["https://example.com".to_string()]);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_invalid_url() {
        let errors = validate_allow_list(&vec!["not-a-url".to_string()]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Invalid URL"));
    }

    #[test]
    fn test_validate_non_http_scheme() {
        let errors = validate_allow_list(&vec!["ftp://example.com".to_string()]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Only http and https"));
    }

    #[test]
    fn test_validate_query_string_warning() {
        let errors = validate_allow_list(&vec!["https://example.com?q=1".to_string()]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Query strings"));
    }
}