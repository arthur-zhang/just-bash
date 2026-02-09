/// Response formatting utilities for curl command

use std::collections::HashMap;

/// Format response headers as "Name: Value\r\n"
pub fn format_headers(headers: &HashMap<String, String>) -> String {
    headers
        .iter()
        .map(|(name, value)| format!("{}: {}", name, value))
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// Extract filename from URL path (for -O flag)
pub fn extract_filename(url: &str) -> String {
    // Try to parse as URL-like string
    // Find the path portion after the host
    if let Some(after_scheme) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
        if let Some(slash_pos) = after_scheme.find('/') {
            let path = &after_scheme[slash_pos..];
            // Remove query string
            let path = path.split('?').next().unwrap_or(path);
            // Remove fragment
            let path = path.split('#').next().unwrap_or(path);
            let filename = path.rsplit('/').next().unwrap_or("");
            if !filename.is_empty() {
                return filename.to_string();
            }
        }
    }
    "index.html".to_string()
}

/// Apply write-out format string substitution
pub fn apply_write_out(
    format: &str,
    status: u16,
    headers: &HashMap<String, String>,
    url: &str,
    body_len: usize,
) -> String {
    let mut output = format.to_string();
    output = output.replace("%{http_code}", &status.to_string());
    output = output.replace(
        "%{content_type}",
        headers.get("content-type").map(|s| s.as_str()).unwrap_or(""),
    );
    output = output.replace("%{url_effective}", url);
    output = output.replace("%{size_download}", &body_len.to_string());
    output = output.replace("\\n", "\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_headers() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "text/html".to_string());
        let formatted = format_headers(&headers);
        assert!(formatted.contains("content-type: text/html"));
    }

    #[test]
    fn test_extract_filename_with_path() {
        assert_eq!(extract_filename("https://example.com/path/file.txt"), "file.txt");
    }

    #[test]
    fn test_extract_filename_root() {
        assert_eq!(extract_filename("https://example.com/"), "index.html");
    }

    #[test]
    fn test_extract_filename_no_path() {
        assert_eq!(extract_filename("https://example.com"), "index.html");
    }

    #[test]
    fn test_extract_filename_with_query() {
        assert_eq!(extract_filename("https://example.com/file.zip?v=1"), "file.zip");
    }

    #[test]
    fn test_apply_write_out_http_code() {
        let headers = HashMap::new();
        let result = apply_write_out("%{http_code}", 200, &headers, "https://example.com", 100);
        assert_eq!(result, "200");
    }

    #[test]
    fn test_apply_write_out_content_type() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        let result = apply_write_out("%{content_type}", 200, &headers, "https://example.com", 100);
        assert_eq!(result, "application/json");
    }

    #[test]
    fn test_apply_write_out_url_effective() {
        let headers = HashMap::new();
        let result = apply_write_out("%{url_effective}", 200, &headers, "https://example.com", 100);
        assert_eq!(result, "https://example.com");
    }

    #[test]
    fn test_apply_write_out_size_download() {
        let headers = HashMap::new();
        let result = apply_write_out("%{size_download}", 200, &headers, "https://example.com", 42);
        assert_eq!(result, "42");
    }

    #[test]
    fn test_apply_write_out_newline() {
        let headers = HashMap::new();
        let result = apply_write_out("%{http_code}\\n", 200, &headers, "https://example.com", 0);
        assert_eq!(result, "200\n");
    }

    #[test]
    fn test_apply_write_out_combined() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "text/plain".to_string());
        let result = apply_write_out(
            "code=%{http_code} type=%{content_type}\\n",
            404,
            &headers,
            "https://example.com",
            0,
        );
        assert_eq!(result, "code=404 type=text/plain\n");
    }
}
