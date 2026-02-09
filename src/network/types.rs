use std::collections::HashMap;
use std::fmt;

/// HTTP methods that can be allowed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Patch,
    Options,
}

impl HttpMethod {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "HEAD" => Some(Self::Head),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "DELETE" => Some(Self::Delete),
            "PATCH" => Some(Self::Patch),
            "OPTIONS" => Some(Self::Options),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Options => "OPTIONS",
        }
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Configuration for network access
#[derive(Debug, Clone, Default)]
pub struct NetworkConfig {
    /// List of allowed URL prefixes (origin + optional path)
    pub allowed_url_prefixes: Vec<String>,
    /// Allowed HTTP methods (default: GET, HEAD)
    pub allowed_methods: Option<Vec<HttpMethod>>,
    /// Bypass allow-list (DANGEROUS)
    pub dangerously_allow_full_internet_access: bool,
    /// Max redirects (default: 20)
    pub max_redirects: Option<usize>,
    /// Request timeout in ms (default: 30000)
    pub timeout_ms: Option<u64>,
}

/// Result of a network fetch operation
#[derive(Debug, Clone)]
pub struct FetchResult {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub url: String,
}

/// Network error types
#[derive(Debug, Clone)]
pub enum NetworkError {
    /// URL not in allow-list
    AccessDenied { url: String },
    /// Too many redirects
    TooManyRedirects { max: usize },
    /// Redirect target not in allow-list
    RedirectNotAllowed { url: String },
    /// HTTP method not allowed
    MethodNotAllowed { method: String, allowed: Vec<String> },
    /// Fetch operation failed
    FetchError { message: String },
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccessDenied { url } => {
                write!(f, "Network access denied: URL not in allow-list: {}", url)
            }
            Self::TooManyRedirects { max } => {
                write!(f, "Too many redirects (max: {})", max)
            }
            Self::RedirectNotAllowed { url } => {
                write!(f, "Redirect target not in allow-list: {}", url)
            }
            Self::MethodNotAllowed { method, allowed } => {
                write!(
                    f,
                    "HTTP method '{}' not allowed. Allowed methods: {}",
                    method,
                    allowed.join(", ")
                )
            }
            Self::FetchError { message } => write!(f, "Fetch error: {}", message),
        }
    }
}

impl std::error::Error for NetworkError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_method_from_str() {
        assert_eq!(HttpMethod::from_str("GET"), Some(HttpMethod::Get));
        assert_eq!(HttpMethod::from_str("get"), Some(HttpMethod::Get));
        assert_eq!(HttpMethod::from_str("Post"), Some(HttpMethod::Post));
        assert_eq!(HttpMethod::from_str("DELETE"), Some(HttpMethod::Delete));
        assert_eq!(HttpMethod::from_str("PATCH"), Some(HttpMethod::Patch));
        assert_eq!(HttpMethod::from_str("OPTIONS"), Some(HttpMethod::Options));
        assert_eq!(HttpMethod::from_str("invalid"), None);
        assert_eq!(HttpMethod::from_str(""), None);
    }

    #[test]
    fn test_http_method_as_str() {
        assert_eq!(HttpMethod::Get.as_str(), "GET");
        assert_eq!(HttpMethod::Post.as_str(), "POST");
        assert_eq!(HttpMethod::Head.as_str(), "HEAD");
    }

    #[test]
    fn test_http_method_display() {
        assert_eq!(format!("{}", HttpMethod::Get), "GET");
        assert_eq!(format!("{}", HttpMethod::Delete), "DELETE");
    }

    #[test]
    fn test_network_config_default() {
        let config = NetworkConfig::default();
        assert!(config.allowed_url_prefixes.is_empty());
        assert!(config.allowed_methods.is_none());
        assert!(!config.dangerously_allow_full_internet_access);
        assert!(config.max_redirects.is_none());
        assert!(config.timeout_ms.is_none());
    }

    #[test]
    fn test_network_error_display() {
        let err = NetworkError::AccessDenied {
            url: "https://evil.com".to_string(),
        };
        assert!(format!("{}", err).contains("https://evil.com"));

        let err = NetworkError::TooManyRedirects { max: 20 };
        assert!(format!("{}", err).contains("20"));

        let err = NetworkError::RedirectNotAllowed {
            url: "https://bad.com".to_string(),
        };
        assert!(format!("{}", err).contains("https://bad.com"));

        let err = NetworkError::MethodNotAllowed {
            method: "POST".to_string(),
            allowed: vec!["GET".to_string(), "HEAD".to_string()],
        };
        assert!(format!("{}", err).contains("POST"));
        assert!(format!("{}", err).contains("GET, HEAD"));

        let err = NetworkError::FetchError {
            message: "timeout".to_string(),
        };
        assert!(format!("{}", err).contains("timeout"));
    }

    #[test]
    fn test_fetch_result_construction() {
        let result = FetchResult {
            status: 200,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            body: "hello".to_string(),
            url: "https://example.com".to_string(),
        };
        assert_eq!(result.status, 200);
        assert_eq!(result.body, "hello");
    }
}
