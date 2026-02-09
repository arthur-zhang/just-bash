/// Types for curl command

#[derive(Debug, Clone)]
pub struct FormField {
    pub name: String,
    pub value: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CurlOptions {
    pub url: Option<String>,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub data: Option<String>,
    pub data_binary: bool,
    pub form_fields: Vec<FormField>,
    pub upload_file: Option<String>,
    pub output_file: Option<String>,
    pub use_remote_name: bool,
    pub head_only: bool,
    pub include_headers: bool,
    pub follow_redirects: bool,
    pub fail_silently: bool,
    pub silent: bool,
    pub show_error: bool,
    pub verbose: bool,
    pub user: Option<String>,
    pub cookie_jar: Option<String>,
    pub write_out: Option<String>,
    pub timeout_ms: Option<u64>,
}

impl Default for CurlOptions {
    fn default() -> Self {
        Self {
            url: None,
            method: "GET".to_string(),
            headers: Vec::new(),
            data: None,
            data_binary: false,
            form_fields: Vec::new(),
            upload_file: None,
            output_file: None,
            use_remote_name: false,
            head_only: false,
            include_headers: false,
            follow_redirects: true,
            fail_silently: false,
            silent: false,
            show_error: false,
            verbose: false,
            user: None,
            cookie_jar: None,
            write_out: None,
            timeout_ms: None,
        }
    }
}
