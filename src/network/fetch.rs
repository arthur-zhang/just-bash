// src/network/fetch.rs

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::future::Future;
use crate::commands::types::{FetchFn, FetchResponse};
use super::allow_list::is_url_allowed;
use super::types::{NetworkConfig, NetworkError};

const DEFAULT_MAX_REDIRECTS: usize = 20;
const BODYLESS_METHODS: &[&str] = &["GET", "HEAD", "OPTIONS"];
const REDIRECT_CODES: &[u16] = &[301, 302, 303, 307, 308];

/// Options for a single fetch request
#[derive(Debug, Clone, Default)]
pub struct SecureFetchOptions {
    pub method: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    pub follow_redirects: Option<bool>,
}

/// Perform a secure fetch with allow-list enforcement and redirect handling.
pub async fn secure_fetch(
    config: &NetworkConfig,
    raw_fetch: &FetchFn,
    url: &str,
    options: SecureFetchOptions,
) -> Result<FetchResponse, NetworkError> {
    let method = options.method.as_deref().unwrap_or("GET").to_uppercase();
    let max_redirects = config.max_redirects.unwrap_or(DEFAULT_MAX_REDIRECTS);
    let follow_redirects = options.follow_redirects.unwrap_or(true);

    // Check URL allowed
    check_url_allowed(config, url)?;
    // Check method allowed
    check_method_allowed(config, &method)?;

    let mut current_url = url.to_string();
    let mut redirect_count = 0;

    loop {
        let headers = options.headers.clone().unwrap_or_default();
        let body = if BODYLESS_METHODS.contains(&method.as_str()) {
            None
        } else {
            options.body.clone()
        };
        let response = raw_fetch(current_url.clone(), method.clone(), headers, body)
            .await
            .map_err(|e| NetworkError::FetchError { message: e })?;

        // Check for redirects
        if REDIRECT_CODES.contains(&response.status) && follow_redirects {
            if let Some(location) = response.headers.get("location") {
                let redirect_url = resolve_redirect_url(&current_url, location);

                // Check redirect target against allow-list
                if !config.dangerously_allow_full_internet_access {
                    if !is_url_allowed(&redirect_url, &config.allowed_url_prefixes) {
                        return Err(NetworkError::RedirectNotAllowed { url: redirect_url });
                    }
                }

                redirect_count += 1;
                if redirect_count > max_redirects {
                    return Err(NetworkError::TooManyRedirects { max: max_redirects });
                }

                current_url = redirect_url;
                continue;
            }
        }

        return Ok(response);
    }
}

/// Create a secure FetchFn that wraps a raw FetchFn with allow-list enforcement.
/// The returned FetchFn has the same signature as the raw one but adds security checks.
pub fn create_secure_fetch_fn(config: NetworkConfig, raw_fetch: FetchFn) -> FetchFn {
    Arc::new(move |url: String, method: String, headers: HashMap<String, String>, body: Option<String>| {
        let config = config.clone();
        let raw_fetch = raw_fetch.clone();
        Box::pin(async move {
            let options = SecureFetchOptions {
                method: Some(method),
                headers: Some(headers),
                body,
                follow_redirects: Some(true),
            };
            secure_fetch(&config, &raw_fetch, &url, options)
                .await
                .map_err(|e| e.to_string())
        }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
    })
}

fn check_url_allowed(config: &NetworkConfig, url: &str) -> Result<(), NetworkError> {
    if config.dangerously_allow_full_internet_access {
        return Ok(());
    }
    if !is_url_allowed(url, &config.allowed_url_prefixes) {
        return Err(NetworkError::AccessDenied { url: url.to_string() });
    }
    Ok(())
}

fn check_method_allowed(config: &NetworkConfig, method: &str) -> Result<(), NetworkError> {
    if config.dangerously_allow_full_internet_access {
        return Ok(());
    }
    let allowed = config.allowed_methods.as_ref().map(|methods| {
        methods.iter().map(|m| m.as_str().to_string()).collect::<Vec<_>>()
    }).unwrap_or_else(|| vec!["GET".to_string(), "HEAD".to_string()]);

    let upper_method = method.to_uppercase();
    if !allowed.iter().any(|m| m == &upper_method) {
        return Err(NetworkError::MethodNotAllowed {
            method: upper_method,
            allowed,
        });
    }
    Ok(())
}

/// Resolve a redirect URL (may be relative) against the current URL.
fn resolve_redirect_url(base_url: &str, location: &str) -> String {
    // If location is absolute, use it directly
    if location.starts_with("http://") || location.starts_with("https://") {
        return location.to_string();
    }

    // Relative URL - resolve against base
    if let Some(scheme_end) = base_url.find("://") {
        let after_scheme = &base_url[scheme_end + 3..];
        if let Some(first_slash) = after_scheme.find('/') {
            let origin = &base_url[..scheme_end + 3 + first_slash];
            if location.starts_with('/') {
                // Absolute path
                return format!("{}{}", origin, location);
            } else {
                // Relative path - resolve against current directory
                let base_path = &base_url[..base_url.rfind('/').unwrap_or(base_url.len())];
                return format!("{}/{}", base_path, location);
            }
        } else {
            // No path in base URL
            let origin = base_url;
            if location.starts_with('/') {
                return format!("{}{}", origin, location);
            } else {
                return format!("{}/{}", origin, location);
            }
        }
    }

    // Fallback
    location.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::HttpMethod;
    use std::sync::Arc;

    /// Create a mock FetchFn that returns a predefined response
    fn mock_fetch(status: u16, body: &str) -> FetchFn {
        let body = body.to_string();
        Arc::new(move |url: String, _method: String, _headers: HashMap<String, String>, _body: Option<String>| {
            let body = body.clone();
            Box::pin(async move {
                Ok(FetchResponse {
                    status,
                    headers: HashMap::new(),
                    body,
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        })
    }

    /// Create a mock FetchFn that returns a redirect
    fn mock_redirect_fetch(redirect_to: &str, final_body: &str) -> FetchFn {
        let redirect_to = redirect_to.to_string();
        let final_body = final_body.to_string();
        Arc::new(move |url: String, _method: String, _headers: HashMap<String, String>, _body: Option<String>| {
            let redirect_to = redirect_to.clone();
            let final_body = final_body.clone();
            Box::pin(async move {
                if !url.contains("redirected") {
                    let mut headers = HashMap::new();
                    headers.insert("location".to_string(), redirect_to);
                    Ok(FetchResponse {
                        status: 302,
                        headers,
                        body: String::new(),
                        url,
                    })
                } else {
                    Ok(FetchResponse {
                        status: 200,
                        headers: HashMap::new(),
                        body: final_body,
                        url,
                    })
                }
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        })
    }

    #[tokio::test]
    async fn test_secure_fetch_allowed_url() {
        let config = NetworkConfig {
            allowed_url_prefixes: vec!["https://api.example.com".to_string()],
            ..Default::default()
        };
        let fetch = mock_fetch(200, "ok");
        let result = secure_fetch(&config, &fetch, "https://api.example.com/data", SecureFetchOptions::default()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().body, "ok");
    }

    #[tokio::test]
    async fn test_secure_fetch_denied_url() {
        let config = NetworkConfig {
            allowed_url_prefixes: vec!["https://api.example.com".to_string()],
            ..Default::default()
        };
        let fetch = mock_fetch(200, "ok");
        let result = secure_fetch(&config, &fetch, "https://evil.com/hack", SecureFetchOptions::default()).await;
        assert!(matches!(result, Err(NetworkError::AccessDenied { .. })));
    }

    #[tokio::test]
    async fn test_secure_fetch_full_access() {
        let config = NetworkConfig {
            dangerously_allow_full_internet_access: true,
            ..Default::default()
        };
        let fetch = mock_fetch(200, "ok");
        let result = secure_fetch(&config, &fetch, "https://anything.com/whatever", SecureFetchOptions::default()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_secure_fetch_method_not_allowed() {
        let config = NetworkConfig {
            allowed_url_prefixes: vec!["https://api.example.com".to_string()],
            ..Default::default()
        };
        let fetch = mock_fetch(200, "ok");
        let options = SecureFetchOptions { method: Some("POST".to_string()), ..Default::default() };
        let result = secure_fetch(&config, &fetch, "https://api.example.com/data", options).await;
        assert!(matches!(result, Err(NetworkError::MethodNotAllowed { .. })));
    }

    #[tokio::test]
    async fn test_secure_fetch_method_allowed() {
        let config = NetworkConfig {
            allowed_url_prefixes: vec!["https://api.example.com".to_string()],
            allowed_methods: Some(vec![HttpMethod::Get, HttpMethod::Post]),
            ..Default::default()
        };
        let fetch = mock_fetch(200, "ok");
        let options = SecureFetchOptions { method: Some("POST".to_string()), ..Default::default() };
        let result = secure_fetch(&config, &fetch, "https://api.example.com/data", options).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_secure_fetch_redirect_allowed() {
        let config = NetworkConfig {
            allowed_url_prefixes: vec!["https://api.example.com".to_string()],
            ..Default::default()
        };
        let fetch = mock_redirect_fetch("https://api.example.com/redirected", "final");
        let result = secure_fetch(&config, &fetch, "https://api.example.com/start", SecureFetchOptions::default()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().body, "final");
    }

    #[tokio::test]
    async fn test_secure_fetch_redirect_denied() {
        let config = NetworkConfig {
            allowed_url_prefixes: vec!["https://api.example.com".to_string()],
            ..Default::default()
        };
        let fetch = mock_redirect_fetch("https://evil.com/hack", "bad");
        let result = secure_fetch(&config, &fetch, "https://api.example.com/start", SecureFetchOptions::default()).await;
        assert!(matches!(result, Err(NetworkError::RedirectNotAllowed { .. })));
    }

    #[tokio::test]
    async fn test_secure_fetch_too_many_redirects() {
        let config = NetworkConfig {
            allowed_url_prefixes: vec!["https://api.example.com".to_string()],
            max_redirects: Some(2),
            ..Default::default()
        };
        // Create a fetch that always redirects
        let fetch: FetchFn = Arc::new(|url: String, _: String, _: HashMap<String, String>, _: Option<String>| {
            Box::pin(async move {
                let mut headers = HashMap::new();
                headers.insert("location".to_string(), format!("{}/next", url));
                Ok(FetchResponse { status: 302, headers, body: String::new(), url })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let result = secure_fetch(&config, &fetch, "https://api.example.com/start", SecureFetchOptions::default()).await;
        assert!(matches!(result, Err(NetworkError::TooManyRedirects { max: 2 })));
    }

    #[tokio::test]
    async fn test_secure_fetch_no_follow_redirects() {
        let config = NetworkConfig {
            allowed_url_prefixes: vec!["https://api.example.com".to_string()],
            ..Default::default()
        };
        let fetch = mock_redirect_fetch("https://api.example.com/redirected", "final");
        let options = SecureFetchOptions { follow_redirects: Some(false), ..Default::default() };
        let result = secure_fetch(&config, &fetch, "https://api.example.com/start", options).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, 302); // Returns redirect response directly
    }

    #[test]
    fn test_resolve_redirect_url_absolute() {
        assert_eq!(
            resolve_redirect_url("https://example.com/path", "https://other.com/new"),
            "https://other.com/new"
        );
    }

    #[test]
    fn test_resolve_redirect_url_absolute_path() {
        assert_eq!(
            resolve_redirect_url("https://example.com/old/path", "/new/path"),
            "https://example.com/new/path"
        );
    }

    #[test]
    fn test_resolve_redirect_url_relative() {
        assert_eq!(
            resolve_redirect_url("https://example.com/old/path", "new"),
            "https://example.com/old/new"
        );
    }

    // test create_secure_fetch_fn
    #[tokio::test]
    async fn test_create_secure_fetch_fn() {
        let config = NetworkConfig {
            allowed_url_prefixes: vec!["https://api.example.com".to_string()],
            ..Default::default()
        };
        let raw = mock_fetch(200, "wrapped");
        let secure = create_secure_fetch_fn(config, raw);
        let result = secure("https://api.example.com/data".to_string(), "GET".to_string(), HashMap::new(), None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().body, "wrapped");
    }

    #[tokio::test]
    async fn test_create_secure_fetch_fn_denied() {
        let config = NetworkConfig {
            allowed_url_prefixes: vec!["https://api.example.com".to_string()],
            ..Default::default()
        };
        let raw = mock_fetch(200, "wrapped");
        let secure = create_secure_fetch_fn(config, raw);
        let result = secure("https://evil.com/hack".to_string(), "GET".to_string(), HashMap::new(), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not in allow-list"));
    }
}
