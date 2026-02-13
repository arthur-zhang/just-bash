/// curl - Transfer data from or to a server

pub mod types;
pub mod parse;
pub mod form;
pub mod response_formatting;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use std::collections::HashMap;
use crate::commands::{Command, CommandContext, CommandResult};
use self::form::generate_multipart_body;
use self::parse::parse_options;
use self::response_formatting::{apply_write_out, extract_filename, format_headers};

pub struct CurlCommand;

/// Prepare request headers from options, including auth
fn prepare_headers(
    options: &types::CurlOptions,
    content_type: Option<&str>,
) -> HashMap<String, String> {
    let mut headers = HashMap::new();

    // Copy user-specified headers
    for (name, value) in &options.headers {
        headers.insert(name.clone(), value.clone());
    }

    // Add authentication header
    if let Some(ref user) = options.user {
        let encoded = STANDARD.encode(user.as_bytes());
        headers.insert("Authorization".to_string(), format!("Basic {}", encoded));
    }

    // Set content type if needed and not already set
    if let Some(ct) = content_type {
        if !headers.contains_key("Content-Type") {
            headers.insert("Content-Type".to_string(), ct.to_string());
        }
    }

    headers
}

/// Build output string from response
fn build_output(
    options: &types::CurlOptions,
    status: u16,
    resp_headers: &HashMap<String, String>,
    body: &str,
    request_url: &str,
    response_url: &str,
) -> String {
    let mut output = String::new();
    let status_text = default_status_text(status);

    // Verbose output
    if options.verbose {
        output.push_str(&format!("> {} {}\n", options.method, request_url));
        for (name, value) in &options.headers {
            output.push_str(&format!("> {}: {}\n", name, value));
        }
        output.push_str(">\n");
        output.push_str(&format!("< HTTP/1.1 {} {}\n", status, status_text));
        for (name, value) in resp_headers {
            output.push_str(&format!("< {}: {}\n", name, value));
        }
        output.push_str("<\n");
    }

    // Include headers with -i/--include
    if options.include_headers && !options.verbose {
        output.push_str(&format!("HTTP/1.1 {} {}\r\n", status, status_text));
        output.push_str(&format_headers(resp_headers));
        output.push_str("\r\n\r\n");
    }

    // Add body (unless head-only mode)
    if !options.head_only {
        output.push_str(body);
    } else if options.include_headers || options.verbose {
        // For HEAD, we already showed headers
    } else {
        // HEAD without -i shows headers
        output.push_str(&format!("HTTP/1.1 {} {}\r\n", status, status_text));
        output.push_str(&format_headers(resp_headers));
        output.push_str("\r\n");
    }

    // Write-out format
    if let Some(ref write_out) = options.write_out {
        output.push_str(&apply_write_out(
            write_out,
            status,
            resp_headers,
            response_url,
            body.len(),
        ));
    }

    output
}

fn default_status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

#[async_trait]
impl Command for CurlCommand {
    fn name(&self) -> &'static str {
        "curl"
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        // Parse options
        let options = match parse_options(&ctx.args) {
            Ok(opts) => opts,
            Err(e) => {
                return CommandResult::with_exit_code(
                    String::new(),
                    format!("{}\n", e),
                    2,
                );
            }
        };

        // Check for URL
        let url_str = match options.url {
            Some(ref u) => u.clone(),
            None => {
                return CommandResult::with_exit_code(
                    String::new(),
                    "curl: no URL specified\n".to_string(),
                    2,
                );
            }
        };

        // Check for fetch_fn
        let fetch_fn = match ctx.fetch_fn {
            Some(ref f) => f.clone(),
            None => {
                return CommandResult::with_exit_code(
                    String::new(),
                    "curl: (6) Could not resolve host (network not available)\n".to_string(),
                    6,
                );
            }
        };

        // Normalize URL - add https:// if no protocol
        let url = if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
            format!("https://{}", url_str)
        } else {
            url_str
        };

        // Prepare body
        let mut body: Option<String> = None;
        let mut content_type: Option<String> = None;

        // Handle -T/--upload-file
        if let Some(ref upload_file) = options.upload_file {
            let file_path = ctx.fs.resolve_path(&ctx.cwd, upload_file);
            match ctx.fs.read_file(&file_path).await {
                Ok(content) => body = Some(content),
                Err(_) => {
                    return CommandResult::with_exit_code(
                        String::new(),
                        format!("curl: (26) Failed to open/read file: {}\n", upload_file),
                        26,
                    );
                }
            }
        }

        // Handle -F/--form multipart data
        if !options.form_fields.is_empty() {
            let mut file_contents = HashMap::new();

            for field in &options.form_fields {
                if field.value.starts_with('@') || field.value.starts_with('<') {
                    let file_path = ctx.fs.resolve_path(&ctx.cwd, &field.value[1..]);
                    match ctx.fs.read_file(&file_path).await {
                        Ok(content) => {
                            file_contents.insert(field.value[1..].to_string(), content);
                        }
                        Err(_) => {
                            file_contents.insert(field.value[1..].to_string(), String::new());
                        }
                    }
                }
            }

            let (multipart_body, multipart_ct) =
                generate_multipart_body(&options.form_fields, &file_contents);
            body = Some(multipart_body);
            content_type = Some(multipart_ct);
        }

        // Handle -d/--data variants
        if body.is_none() {
            if let Some(ref data) = options.data {
                body = Some(data.clone());
            }
        }

        // Prepare headers
        let headers = prepare_headers(&options, content_type.as_deref());

        // Make the request
        match fetch_fn(url.clone(), options.method.clone(), headers, body).await {
            Ok(response) => {
                // Save cookies if requested
                if let Some(ref cookie_jar) = options.cookie_jar {
                    if let Some(set_cookie) = response.headers.get("set-cookie") {
                        let jar_path = ctx.fs.resolve_path(&ctx.cwd, cookie_jar);
                        let _ = ctx.fs.write_file(&jar_path, set_cookie.as_bytes()).await;
                    }
                }

                // Check for HTTP errors with -f/--fail
                if options.fail_silently && response.status >= 400 {
                    let stderr = if options.show_error || !options.silent {
                        format!(
                            "curl: (22) The requested URL returned error: {}\n",
                            response.status
                        )
                    } else {
                        String::new()
                    };
                    return CommandResult::with_exit_code(String::new(), stderr, 22);
                }

                let mut output = build_output(
                    &options,
                    response.status,
                    &response.headers,
                    &response.body,
                    &url,
                    &response.url,
                );

                // Write to file
                if options.output_file.is_some() || options.use_remote_name {
                    let filename = options
                        .output_file
                        .clone()
                        .unwrap_or_else(|| extract_filename(&url));
                    let file_path = ctx.fs.resolve_path(&ctx.cwd, &filename);
                    let file_body = if options.head_only {
                        ""
                    } else {
                        &response.body
                    };
                    let _ = ctx.fs.write_file(&file_path, file_body.as_bytes()).await;

                    // When writing to file, don't output body to stdout unless verbose
                    if !options.verbose {
                        output = String::new();
                    }

                    // Add write-out after file write
                    if let Some(ref write_out) = options.write_out {
                        output = apply_write_out(
                            write_out,
                            response.status,
                            &response.headers,
                            &response.url,
                            response.body.len(),
                        );
                    }
                }

                CommandResult::with_exit_code(output, String::new(), 0)
            }
            Err(message) => {
                let mut exit_code = 1;
                if message.contains("Network access denied") {
                    exit_code = 7;
                } else if message.contains("HTTP method") && message.contains("not allowed") {
                    exit_code = 3;
                } else if message.contains("Redirect target not in allow-list") {
                    exit_code = 47;
                } else if message.contains("Too many redirects") {
                    exit_code = 47;
                } else if message.contains("aborted") {
                    exit_code = 28;
                }

                let show_err = !options.silent || options.show_error;
                let stderr = if show_err {
                    format!("curl: ({}) {}\n", exit_code, message)
                } else {
                    String::new()
                };

                CommandResult::with_exit_code(String::new(), stderr, exit_code)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::{FetchFn, FetchResponse};
    use crate::fs::{FileSystem, InMemoryFs};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    fn make_fetch_fn(
        status: u16,
        body: &str,
        resp_headers: HashMap<String, String>,
    ) -> FetchFn {
        let body = body.to_string();
        Arc::new(move |url: String, _method: String, _headers: HashMap<String, String>, _body: Option<String>| {
            let body = body.clone();
            let resp_headers = resp_headers.clone();
            Box::pin(async move {
                Ok(FetchResponse {
                    status,
                    headers: resp_headers,
                    body,
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        })
    }

    fn make_ctx(args: Vec<&str>, fetch_fn: Option<FetchFn>) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
            exec_fn: None,
            fetch_fn,
        }
    }

    fn make_ctx_with_fs(args: Vec<&str>, fetch_fn: Option<FetchFn>, fs: Arc<InMemoryFs>) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs,
            exec_fn: None,
            fetch_fn,
        }
    }

    fn default_headers() -> HashMap<String, String> {
        let mut h = HashMap::new();
        h.insert("content-type".to_string(), "text/plain".to_string());
        h
    }

    #[tokio::test]
    async fn test_get_request() {
        let fetch = make_fetch_fn(200, "response body", default_headers());
        let ctx = make_ctx(vec!["https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "response body");
    }

    #[tokio::test]
    async fn test_post_with_data() {
        let fetch: FetchFn = Arc::new(|url, method, _headers, body| {
            Box::pin(async move {
                assert_eq!(method, "POST");
                assert_eq!(body.as_deref(), Some("key=value"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-d", "key=value", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "ok");
    }

    #[tokio::test]
    async fn test_headers_included() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "text/html".to_string());
        let fetch = make_fetch_fn(200, "body", headers);
        let ctx = make_ctx(vec!["-i", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("HTTP/1.1 200"));
        assert!(result.stdout.contains("content-type: text/html"));
        assert!(result.stdout.contains("body"));
    }

    #[tokio::test]
    async fn test_output_to_file() {
        let fetch = make_fetch_fn(200, "file content", default_headers());
        let fs = Arc::new(InMemoryFs::new());
        let ctx = make_ctx_with_fs(
            vec!["-o", "output.txt", "https://example.com"],
            Some(fetch),
            fs.clone(),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // stdout should be empty when writing to file
        assert_eq!(result.stdout, "");
        // File should contain the body
        let content = fs.read_file("/output.txt").await.unwrap();
        assert_eq!(content, "file content");
    }

    #[tokio::test]
    async fn test_fail_on_http_error() {
        let fetch = make_fetch_fn(404, "not found", default_headers());
        let ctx = make_ctx(vec!["-f", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 22);
        assert!(result.stderr.contains("404"));
    }

    #[tokio::test]
    async fn test_without_fetch_fn() {
        let ctx = make_ctx(vec!["https://example.com"], None);
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 6);
        assert!(result.stderr.contains("Could not resolve host"));
    }

    #[tokio::test]
    async fn test_verbose_output() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "text/plain".to_string());
        let fetch = make_fetch_fn(200, "body", headers);
        let ctx = make_ctx(vec!["-v", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("> GET https://example.com"));
        assert!(result.stdout.contains("< HTTP/1.1 200"));
        assert!(result.stdout.contains("body"));
    }

    #[tokio::test]
    async fn test_basic_auth() {
        let fetch: FetchFn = Arc::new(|url, _method, headers, _body| {
            Box::pin(async move {
                let auth = headers.get("Authorization").cloned().unwrap_or_default();
                assert!(auth.starts_with("Basic "));
                // "user:pass" in base64 is "dXNlcjpwYXNz"
                assert_eq!(auth, "Basic dXNlcjpwYXNz");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "authenticated".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-u", "user:pass", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "authenticated");
    }

    #[tokio::test]
    async fn test_url_normalization() {
        let fetch: FetchFn = Arc::new(|url, _method, _headers, _body| {
            Box::pin(async move {
                assert_eq!(url, "https://example.com");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_no_url_specified() {
        let fetch = make_fetch_fn(200, "", HashMap::new());
        let ctx = make_ctx(vec!["-s"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 2);
        assert!(result.stderr.contains("no URL specified"));
    }

    #[tokio::test]
    async fn test_write_out_format() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        let fetch = make_fetch_fn(200, "{}", headers);
        let ctx = make_ctx(
            vec!["-w", "%{http_code}\\n", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("{}"));
        assert!(result.stdout.ends_with("200\n"));
    }

    #[tokio::test]
    async fn test_remote_name_output() {
        let fetch = make_fetch_fn(200, "downloaded", default_headers());
        let fs = Arc::new(InMemoryFs::new());
        let ctx = make_ctx_with_fs(
            vec!["-O", "https://example.com/path/file.zip"],
            Some(fetch),
            fs.clone(),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/file.zip").await.unwrap();
        assert_eq!(content, "downloaded");
    }

    #[tokio::test]
    async fn test_cookie_jar() {
        let mut headers = HashMap::new();
        headers.insert("set-cookie".to_string(), "session=abc123".to_string());
        let fetch = make_fetch_fn(200, "ok", headers);
        let fs = Arc::new(InMemoryFs::new());
        let ctx = make_ctx_with_fs(
            vec!["-c", "cookies.txt", "https://example.com"],
            Some(fetch),
            fs.clone(),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let cookies = fs.read_file("/cookies.txt").await.unwrap();
        assert_eq!(cookies, "session=abc123");
    }

    #[tokio::test]
    async fn test_silent_mode_suppresses_errors() {
        let fetch: FetchFn = Arc::new(|_url, _method, _headers, _body| {
            Box::pin(async move {
                Err("connection refused".to_string())
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-s", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
        // Silent mode suppresses error output
        assert_eq!(result.stderr, "");
    }

    #[tokio::test]
    async fn test_silent_with_show_error() {
        let fetch: FetchFn = Arc::new(|_url, _method, _headers, _body| {
            Box::pin(async move {
                Err("connection refused".to_string())
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-sS", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_ne!(result.exit_code, 0);
        // -S overrides -s for errors
        assert!(!result.stderr.is_empty());
    }

    #[tokio::test]
    async fn test_head_request() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "text/html".to_string());
        let fetch = make_fetch_fn(200, "", headers);
        let ctx = make_ctx(vec!["-I", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("HTTP/1.1 200"));
        assert!(result.stdout.contains("content-type"));
    }

    #[tokio::test]
    async fn test_custom_header_sent() {
        let fetch: FetchFn = Arc::new(|url, _method, headers, _body| {
            Box::pin(async move {
                let ct = headers.get("Content-Type").cloned().unwrap_or_default();
                assert_eq!(ct, "application/json");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(
            vec!["-H", "Content-Type: application/json", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_put_method() {
        let fetch: FetchFn = Arc::new(|url, method, _headers, _body| {
            Box::pin(async move {
                assert_eq!(method, "PUT");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "updated".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-X", "PUT", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "updated");
    }

    #[tokio::test]
    async fn test_delete_method() {
        let fetch: FetchFn = Arc::new(|url, method, _headers, _body| {
            Box::pin(async move {
                assert_eq!(method, "DELETE");
                Ok(FetchResponse {
                    status: 204,
                    headers: HashMap::new(),
                    body: String::new(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-X", "DELETE", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_patch_method() {
        let fetch: FetchFn = Arc::new(|url, method, _headers, _body| {
            Box::pin(async move {
                assert_eq!(method, "PATCH");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "patched".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-X", "PATCH", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "patched");
    }

    #[tokio::test]
    async fn test_multiple_headers() {
        let fetch: FetchFn = Arc::new(|url, _method, headers, _body| {
            Box::pin(async move {
                assert_eq!(headers.get("Accept").map(|s| s.as_str()), Some("application/json"));
                assert_eq!(headers.get("X-Custom").map(|s| s.as_str()), Some("value"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(
            vec!["-H", "Accept: application/json", "-H", "X-Custom: value", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_data_raw() {
        let fetch: FetchFn = Arc::new(|url, method, _headers, body| {
            Box::pin(async move {
                assert_eq!(method, "POST");
                assert_eq!(body.as_deref(), Some("{\"key\":\"value\"}"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["--data-raw", "{\"key\":\"value\"}", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_fail_on_500_error() {
        let fetch = make_fetch_fn(500, "server error", default_headers());
        let ctx = make_ctx(vec!["-f", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 22);
        assert!(result.stderr.contains("500"));
    }

    #[tokio::test]
    async fn test_fail_on_401_error() {
        let fetch = make_fetch_fn(401, "unauthorized", default_headers());
        let ctx = make_ctx(vec!["-f", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 22);
        assert!(result.stderr.contains("401"));
    }

    #[tokio::test]
    async fn test_success_with_fail_on_2xx() {
        let fetch = make_fetch_fn(201, "created", default_headers());
        let ctx = make_ctx(vec!["-f", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "created");
    }

    #[tokio::test]
    async fn test_success_with_fail_on_3xx() {
        let fetch = make_fetch_fn(301, "moved", default_headers());
        let ctx = make_ctx(vec!["-f", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_head_request_with_verbose() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "text/html".to_string());
        let fetch = make_fetch_fn(200, "", headers);
        let ctx = make_ctx(vec!["-I", "-v", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("> HEAD"));
        assert!(result.stdout.contains("< HTTP/1.1 200"));
    }

    #[tokio::test]
    async fn test_include_headers_with_body() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        headers.insert("x-custom".to_string(), "test".to_string());
        let fetch = make_fetch_fn(200, "{\"result\":\"ok\"}", headers);
        let ctx = make_ctx(vec!["-i", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("HTTP/1.1 200"));
        assert!(result.stdout.contains("content-type: application/json"));
        assert!(result.stdout.contains("x-custom: test"));
        assert!(result.stdout.contains("{\"result\":\"ok\"}"));
    }

    #[tokio::test]
    async fn test_verbose_shows_request_headers() {
        let fetch: FetchFn = Arc::new(|url, _method, _headers, _body| {
            Box::pin(async move {
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(
            vec!["-v", "-H", "Accept: application/json", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("> Accept: application/json"));
    }

    #[tokio::test]
    async fn test_verbose_post_request() {
        let fetch: FetchFn = Arc::new(|url, method, _headers, _body| {
            Box::pin(async move {
                assert_eq!(method, "POST");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "posted".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-v", "-d", "data=test", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("> POST"));
    }

    #[tokio::test]
    async fn test_write_out_with_url() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "text/plain".to_string());
        let fetch = make_fetch_fn(200, "body", headers);
        let ctx = make_ctx(
            vec!["-w", "%{url_effective}\\n", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("https://example.com"));
    }

    #[tokio::test]
    async fn test_write_out_with_content_type() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        let fetch = make_fetch_fn(200, "{}", headers);
        let ctx = make_ctx(
            vec!["-w", "%{content_type}\\n", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("application/json"));
    }

    #[tokio::test]
    async fn test_write_out_with_size() {
        let fetch = make_fetch_fn(200, "test body content", default_headers());
        let ctx = make_ctx(
            vec!["-w", "%{size_download}\\n", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("17")); // length of "test body content"
    }

    #[tokio::test]
    async fn test_output_file_with_verbose() {
        let fetch = make_fetch_fn(200, "file data", default_headers());
        let fs = Arc::new(InMemoryFs::new());
        let ctx = make_ctx_with_fs(
            vec!["-v", "-o", "output.txt", "https://example.com"],
            Some(fetch),
            fs.clone(),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Verbose output should still be shown
        assert!(result.stdout.contains("> GET"));
        let content = fs.read_file("/output.txt").await.unwrap();
        assert_eq!(content, "file data");
    }

    #[tokio::test]
    async fn test_output_file_with_write_out() {
        let fetch = make_fetch_fn(200, "saved", default_headers());
        let fs = Arc::new(InMemoryFs::new());
        let ctx = make_ctx_with_fs(
            vec!["-o", "out.txt", "-w", "%{http_code}\\n", "https://example.com"],
            Some(fetch),
            fs.clone(),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "200\n");
        let content = fs.read_file("/out.txt").await.unwrap();
        assert_eq!(content, "saved");
    }

    #[tokio::test]
    async fn test_remote_name_from_url_path() {
        let fetch = make_fetch_fn(200, "downloaded", default_headers());
        let fs = Arc::new(InMemoryFs::new());
        let ctx = make_ctx_with_fs(
            vec!["-O", "https://example.com/downloads/archive.tar.gz"],
            Some(fetch),
            fs.clone(),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/archive.tar.gz").await.unwrap();
        assert_eq!(content, "downloaded");
    }

    #[tokio::test]
    async fn test_head_only_no_body_in_file() {
        let fetch = make_fetch_fn(200, "body content", default_headers());
        let fs = Arc::new(InMemoryFs::new());
        let ctx = make_ctx_with_fs(
            vec!["-I", "-o", "headers.txt", "https://example.com"],
            Some(fetch),
            fs.clone(),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/headers.txt").await.unwrap();
        assert_eq!(content, ""); // HEAD request should not write body
    }

    #[tokio::test]
    async fn test_upload_file_with_t_option() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/upload.txt", b"file to upload").await.unwrap();

        let fetch: FetchFn = Arc::new(|url, method, _headers, body| {
            Box::pin(async move {
                assert_eq!(method, "PUT");
                assert_eq!(body.as_deref(), Some("file to upload"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "uploaded".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });

        let ctx = make_ctx_with_fs(
            vec!["-T", "/upload.txt", "https://example.com/upload"],
            Some(fetch),
            fs,
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "uploaded");
    }

    #[tokio::test]
    async fn test_upload_nonexistent_file() {
        let fs = Arc::new(InMemoryFs::new());
        let fetch = make_fetch_fn(200, "ok", default_headers());
        let ctx = make_ctx_with_fs(
            vec!["-T", "/nonexistent.txt", "https://example.com/upload"],
            Some(fetch),
            fs,
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 26);
        assert!(result.stderr.contains("Failed to open/read file"));
    }

    #[tokio::test]
    async fn test_form_field_simple() {
        let fetch: FetchFn = Arc::new(|url, method, headers, body| {
            Box::pin(async move {
                assert_eq!(method, "POST");
                let ct = headers.get("Content-Type").cloned().unwrap_or_default();
                assert!(ct.starts_with("multipart/form-data"));
                let body_str = body.unwrap_or_default();
                assert!(body_str.contains("name=\"field1\""));
                assert!(body_str.contains("value1"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-F", "field1=value1", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_form_multiple_fields() {
        let fetch: FetchFn = Arc::new(|url, _method, _headers, body| {
            Box::pin(async move {
                let body_str = body.unwrap_or_default();
                assert!(body_str.contains("name=\"first\""));
                assert!(body_str.contains("John"));
                assert!(body_str.contains("name=\"last\""));
                assert!(body_str.contains("Doe"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(
            vec!["-F", "first=John", "-F", "last=Doe", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_form_file_upload() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/data.txt", b"file contents").await.unwrap();

        let fetch: FetchFn = Arc::new(|url, _method, _headers, body| {
            Box::pin(async move {
                let body_str = body.unwrap_or_default();
                assert!(body_str.contains("name=\"file\""));
                assert!(body_str.contains("filename=\"data.txt\""));
                assert!(body_str.contains("file contents"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "uploaded".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });

        let ctx = make_ctx_with_fs(
            vec!["-F", "file=@/data.txt", "https://example.com/upload"],
            Some(fetch),
            fs,
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_network_error_exit_code() {
        let fetch: FetchFn = Arc::new(|_url, _method, _headers, _body| {
            Box::pin(async move {
                Err("Network access denied".to_string())
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 7);
        assert!(result.stderr.contains("Network access denied"));
    }

    #[tokio::test]
    async fn test_method_not_allowed_exit_code() {
        let fetch: FetchFn = Arc::new(|_url, _method, _headers, _body| {
            Box::pin(async move {
                Err("HTTP method POST not allowed".to_string())
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-d", "data", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 3);
    }

    #[tokio::test]
    async fn test_redirect_error_exit_code() {
        let fetch: FetchFn = Arc::new(|_url, _method, _headers, _body| {
            Box::pin(async move {
                Err("Redirect target not in allow-list".to_string())
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 47);
    }

    #[tokio::test]
    async fn test_too_many_redirects_exit_code() {
        let fetch: FetchFn = Arc::new(|_url, _method, _headers, _body| {
            Box::pin(async move {
                Err("Too many redirects".to_string())
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 47);
    }

    #[tokio::test]
    async fn test_timeout_exit_code() {
        let fetch: FetchFn = Arc::new(|_url, _method, _headers, _body| {
            Box::pin(async move {
                Err("Request aborted".to_string())
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 28);
    }

    #[tokio::test]
    async fn test_silent_and_fail_together() {
        let fetch = make_fetch_fn(404, "not found", default_headers());
        let ctx = make_ctx(vec!["-s", "-f", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 22);
        assert_eq!(result.stderr, ""); // Silent mode suppresses error
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_silent_show_error_and_fail() {
        let fetch = make_fetch_fn(404, "not found", default_headers());
        let ctx = make_ctx(vec!["-sS", "-f", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 22);
        assert!(!result.stderr.is_empty()); // -S shows errors
    }

    #[tokio::test]
    async fn test_post_with_empty_data() {
        let fetch: FetchFn = Arc::new(|url, method, _headers, body| {
            Box::pin(async move {
                assert_eq!(method, "POST");
                assert_eq!(body.as_deref(), Some(""));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-d", "", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_http_protocol_preserved() {
        let fetch: FetchFn = Arc::new(|url, _method, _headers, _body| {
            Box::pin(async move {
                assert_eq!(url, "http://example.com");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["http://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_https_protocol_preserved() {
        let fetch: FetchFn = Arc::new(|url, _method, _headers, _body| {
            Box::pin(async move {
                assert_eq!(url, "https://example.com");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_cookie_jar_saves_cookies() {
        let mut headers = HashMap::new();
        headers.insert("set-cookie".to_string(), "session=xyz789; Path=/".to_string());
        let fetch = make_fetch_fn(200, "ok", headers);
        let fs = Arc::new(InMemoryFs::new());
        let ctx = make_ctx_with_fs(
            vec!["-c", "jar.txt", "https://example.com"],
            Some(fetch),
            fs.clone(),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let cookies = fs.read_file("/jar.txt").await.unwrap();
        assert!(cookies.contains("session=xyz789"));
    }

    #[tokio::test]
    async fn test_multiple_data_options() {
        let fetch: FetchFn = Arc::new(|url, _method, _headers, body| {
            Box::pin(async move {
                // Multiple -d options are concatenated with &
                assert_eq!(body.as_deref(), Some("first&second"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-d", "first", "-d", "second", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_status_text_200() {
        assert_eq!(default_status_text(200), "OK");
    }

    #[tokio::test]
    async fn test_status_text_404() {
        assert_eq!(default_status_text(404), "Not Found");
    }

    #[tokio::test]
    async fn test_status_text_500() {
        assert_eq!(default_status_text(500), "Internal Server Error");
    }

    #[tokio::test]
    async fn test_status_text_unknown() {
        assert_eq!(default_status_text(999), "Unknown");
    }

    #[tokio::test]
    async fn test_status_text_201() {
        assert_eq!(default_status_text(201), "Created");
    }

    #[tokio::test]
    async fn test_status_text_204() {
        assert_eq!(default_status_text(204), "No Content");
    }

    #[tokio::test]
    async fn test_status_text_301() {
        assert_eq!(default_status_text(301), "Moved Permanently");
    }

    #[tokio::test]
    async fn test_status_text_302() {
        assert_eq!(default_status_text(302), "Found");
    }

    #[tokio::test]
    async fn test_status_text_304() {
        assert_eq!(default_status_text(304), "Not Modified");
    }

    #[tokio::test]
    async fn test_status_text_400() {
        assert_eq!(default_status_text(400), "Bad Request");
    }

    #[tokio::test]
    async fn test_status_text_401() {
        assert_eq!(default_status_text(401), "Unauthorized");
    }

    #[tokio::test]
    async fn test_status_text_403() {
        assert_eq!(default_status_text(403), "Forbidden");
    }

    #[tokio::test]
    async fn test_status_text_502() {
        assert_eq!(default_status_text(502), "Bad Gateway");
    }

    #[tokio::test]
    async fn test_status_text_503() {
        assert_eq!(default_status_text(503), "Service Unavailable");
    }

    #[tokio::test]
    async fn test_request_method() {
        let fetch: FetchFn = Arc::new(|url, method, _headers, _body| {
            Box::pin(async move {
                assert_eq!(method, "OPTIONS");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: String::new(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-X", "OPTIONS", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_post_with_json_data() {
        let fetch: FetchFn = Arc::new(|url, method, headers, body| {
            Box::pin(async move {
                assert_eq!(method, "POST");
                assert_eq!(body.as_deref(), Some("{\"name\":\"test\"}"));
                Ok(FetchResponse {
                    status: 201,
                    headers: HashMap::new(),
                    body: "{\"id\":123}".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(
            vec!["-d", "{\"name\":\"test\"}", "-H", "Content-Type: application/json", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "{\"id\":123}");
    }

    #[tokio::test]
    async fn test_verbose_with_multiple_headers() {
        let fetch: FetchFn = Arc::new(|url, _method, _headers, _body| {
            Box::pin(async move {
                let mut resp_headers = HashMap::new();
                resp_headers.insert("x-custom-1".to_string(), "value1".to_string());
                resp_headers.insert("x-custom-2".to_string(), "value2".to_string());
                Ok(FetchResponse {
                    status: 200,
                    headers: resp_headers,
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-v", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("< x-custom-1: value1"));
        assert!(result.stdout.contains("< x-custom-2: value2"));
    }

    #[tokio::test]
    async fn test_include_with_empty_body() {
        let mut headers = HashMap::new();
        headers.insert("content-length".to_string(), "0".to_string());
        let fetch = make_fetch_fn(204, "", headers);
        let ctx = make_ctx(vec!["-i", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("HTTP/1.1 204"));
        assert!(result.stdout.contains("content-length: 0"));
    }

    #[tokio::test]
    async fn test_head_with_include() {
        let mut headers = HashMap::new();
        headers.insert("server".to_string(), "test-server".to_string());
        let fetch = make_fetch_fn(200, "", headers);
        let ctx = make_ctx(vec!["-I", "-i", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("HTTP/1.1 200"));
        assert!(result.stdout.contains("server: test-server"));
    }

    #[tokio::test]
    async fn test_auth_header_encoding() {
        let fetch: FetchFn = Arc::new(|url, _method, headers, _body| {
            Box::pin(async move {
                let auth = headers.get("Authorization").cloned().unwrap_or_default();
                assert!(auth.starts_with("Basic "));
                // Verify it's base64 encoded
                let encoded = auth.strip_prefix("Basic ").unwrap();
                assert!(!encoded.is_empty());
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-u", "admin:secret123", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_form_with_special_characters() {
        let fetch: FetchFn = Arc::new(|url, _method, _headers, body| {
            Box::pin(async move {
                let body_str = body.unwrap_or_default();
                assert!(body_str.contains("name=\"field\""));
                assert!(body_str.contains("value with spaces"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-F", "field=value with spaces", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_output_to_subdirectory() {
        let fetch = make_fetch_fn(200, "content", default_headers());
        let fs = Arc::new(InMemoryFs::new());
        fs.mkdir("/subdir", &crate::fs::types::MkdirOptions { recursive: false }).await.unwrap();
        let ctx = make_ctx_with_fs(
            vec!["-o", "/subdir/file.txt", "https://example.com"],
            Some(fetch),
            fs.clone(),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let content = fs.read_file("/subdir/file.txt").await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_cookie_jar_with_multiple_cookies() {
        let mut headers = HashMap::new();
        headers.insert("set-cookie".to_string(), "cookie1=value1; cookie2=value2".to_string());
        let fetch = make_fetch_fn(200, "ok", headers);
        let fs = Arc::new(InMemoryFs::new());
        let ctx = make_ctx_with_fs(
            vec!["-c", "cookies.txt", "https://example.com"],
            Some(fetch),
            fs.clone(),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        let cookies = fs.read_file("/cookies.txt").await.unwrap();
        assert!(cookies.contains("cookie1=value1"));
    }

    #[tokio::test]
    async fn test_empty_response_body() {
        let fetch = make_fetch_fn(204, "", default_headers());
        let ctx = make_ctx(vec!["https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
    }

    #[tokio::test]
    async fn test_large_response_body() {
        let large_body = "x".repeat(10000);
        let fetch = make_fetch_fn(200, &large_body, default_headers());
        let ctx = make_ctx(vec!["https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.len(), 10000);
    }

    #[tokio::test]
    async fn test_write_out_multiple_variables() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "text/html".to_string());
        let fetch = make_fetch_fn(200, "test", headers);
        let ctx = make_ctx(
            vec!["-w", "code=%{http_code} type=%{content_type}", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("code=200"));
        assert!(result.stdout.contains("type=text/html"));
    }

    #[tokio::test]
    async fn test_silent_mode_with_success() {
        let fetch = make_fetch_fn(200, "response", default_headers());
        let ctx = make_ctx(vec!["-s", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "response");
        assert_eq!(result.stderr, "");
    }

    #[tokio::test]
    async fn test_verbose_with_silent() {
        let fetch = make_fetch_fn(200, "body", default_headers());
        let ctx = make_ctx(vec!["-s", "-v", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        // Verbose should still show output even with silent
        assert!(result.stdout.contains("> GET"));
    }

    #[tokio::test]
    async fn test_fail_with_403() {
        let fetch = make_fetch_fn(403, "forbidden", default_headers());
        let ctx = make_ctx(vec!["-f", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 22);
        assert!(result.stderr.contains("403"));
    }

    #[tokio::test]
    async fn test_fail_with_502() {
        let fetch = make_fetch_fn(502, "bad gateway", default_headers());
        let ctx = make_ctx(vec!["-f", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 22);
        assert!(result.stderr.contains("502"));
    }

    #[tokio::test]
    async fn test_post_switches_method_automatically() {
        let fetch: FetchFn = Arc::new(|url, method, _headers, body| {
            Box::pin(async move {
                assert_eq!(method, "POST");
                assert_eq!(body.as_deref(), Some("data"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-d", "data", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_explicit_get_with_data() {
        let fetch: FetchFn = Arc::new(|url, method, _headers, body| {
            Box::pin(async move {
                // -d with -X GET still switches to POST
                assert_eq!(method, "POST");
                assert_eq!(body.as_deref(), Some("data"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-X", "GET", "-d", "data", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_upload_switches_to_put() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/data.bin", b"binary data").await.unwrap();

        let fetch: FetchFn = Arc::new(|url, method, _headers, _body| {
            Box::pin(async move {
                assert_eq!(method, "PUT");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "uploaded".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });

        let ctx = make_ctx_with_fs(
            vec!["-T", "/data.bin", "https://example.com"],
            Some(fetch),
            fs,
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_form_switches_to_post() {
        let fetch: FetchFn = Arc::new(|url, method, _headers, _body| {
            Box::pin(async move {
                assert_eq!(method, "POST");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-F", "field=value", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_content_type_not_set_by_default() {
        let fetch: FetchFn = Arc::new(|url, _method, headers, _body| {
            Box::pin(async move {
                // Content-Type is not automatically set for -d
                let ct = headers.get("Content-Type").cloned().unwrap_or_default();
                assert_eq!(ct, "");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["-d", "key=value", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_custom_content_type_overrides_default() {
        let fetch: FetchFn = Arc::new(|url, _method, headers, _body| {
            Box::pin(async move {
                let ct = headers.get("Content-Type").cloned().unwrap_or_default();
                assert_eq!(ct, "text/plain");
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(
            vec!["-H", "Content-Type: text/plain", "-d", "data", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_response_with_special_characters() {
        let body = "Response with special chars: <>&\"'";
        let fetch = make_fetch_fn(200, body, default_headers());
        let ctx = make_ctx(vec!["https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, body);
    }

    #[tokio::test]
    async fn test_response_with_unicode() {
        let body = "Unicode: 你好世界 🌍";
        let fetch = make_fetch_fn(200, body, default_headers());
        let ctx = make_ctx(vec!["https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, body);
    }

    #[tokio::test]
    async fn test_write_out_size_zero() {
        let fetch = make_fetch_fn(204, "", default_headers());
        let ctx = make_ctx(
            vec!["-w", "%{size_download}", "https://example.com"],
            Some(fetch),
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("0"));
    }

    #[tokio::test]
    async fn test_multiple_form_fields_with_files() {
        let fs = Arc::new(InMemoryFs::new());
        fs.write_file("/file1.txt", b"content1").await.unwrap();
        fs.write_file("/file2.txt", b"content2").await.unwrap();

        let fetch: FetchFn = Arc::new(|url, _method, _headers, body| {
            Box::pin(async move {
                let body_str = body.unwrap_or_default();
                assert!(body_str.contains("filename=\"file1.txt\""));
                assert!(body_str.contains("content1"));
                assert!(body_str.contains("filename=\"file2.txt\""));
                assert!(body_str.contains("content2"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });

        let ctx = make_ctx_with_fs(
            vec!["-F", "file1=@/file1.txt", "-F", "file2=@/file2.txt", "https://example.com"],
            Some(fetch),
            fs,
        );
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_head_request_no_body_output() {
        let fetch = make_fetch_fn(200, "this should not appear", default_headers());
        let ctx = make_ctx(vec!["-I", "https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(!result.stdout.contains("this should not appear"));
        assert!(result.stdout.contains("HTTP/1.1 200"));
    }

    #[tokio::test]
    async fn test_generic_error_exit_code() {
        let fetch: FetchFn = Arc::new(|_url, _method, _headers, _body| {
            Box::pin(async move {
                Err("Generic error".to_string())
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["https://example.com"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 1);
    }

    #[tokio::test]
    async fn test_url_with_query_string() {
        let fetch: FetchFn = Arc::new(|url, _method, _headers, _body| {
            Box::pin(async move {
                assert!(url.contains("?key=value"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["https://example.com/path?key=value"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn test_url_with_fragment() {
        let fetch: FetchFn = Arc::new(|url, _method, _headers, _body| {
            Box::pin(async move {
                assert!(url.contains("#section"));
                Ok(FetchResponse {
                    status: 200,
                    headers: HashMap::new(),
                    body: "ok".to_string(),
                    url,
                })
            }) as Pin<Box<dyn Future<Output = Result<FetchResponse, String>> + Send>>
        });
        let ctx = make_ctx(vec!["https://example.com/page#section"], Some(fetch));
        let result = CurlCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
    }
}
