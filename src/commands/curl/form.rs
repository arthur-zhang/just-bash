/// Form data handling for curl command

use std::collections::HashMap;
use super::types::FormField;

/// URL-encode a single character
fn url_encode_char(c: char) -> String {
    if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
        c.to_string()
    } else {
        let mut buf = [0u8; 4];
        let encoded = c.encode_utf8(&mut buf);
        encoded.bytes().map(|b| format!("%{:02X}", b)).collect()
    }
}

/// URL-encode a string
fn url_encode(s: &str) -> String {
    s.chars().map(url_encode_char).collect()
}

/// URL-encode form data for --data-urlencode
/// Supports: name=content, =content, plain content
pub fn encode_form_data(input: &str) -> String {
    if let Some(eq_index) = input.find('=') {
        let name = &input[..eq_index];
        let value = &input[eq_index + 1..];
        if !name.is_empty() {
            format!("{}={}", url_encode(name), url_encode(value))
        } else {
            url_encode(value)
        }
    } else {
        url_encode(input)
    }
}

/// Parse -F field specification: "name=value", "name=@file", "name=<file"
pub fn parse_form_field(spec: &str) -> Option<FormField> {
    let eq_index = spec.find('=')?;
    if eq_index == 0 && spec.len() == 1 {
        return None;
    }

    let name = spec[..eq_index].to_string();
    let mut value = spec[eq_index + 1..].to_string();
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;

    // Check for ;type= suffix
    if let Some(type_pos) = value.rfind(";type=") {
        content_type = Some(value[type_pos + 6..].to_string());
        value = value[..type_pos].to_string();
    }

    // Check for ;filename= suffix
    if let Some(fn_pos) = value.find(";filename=") {
        let end = value[fn_pos + 10..].find(';').map(|p| fn_pos + 10 + p).unwrap_or(value.len());
        filename = Some(value[fn_pos + 10..end].to_string());
        value = format!("{}{}", &value[..fn_pos], &value[end..]);
    }

    // @ means file upload, < means file content
    if value.starts_with('@') || value.starts_with('<') {
        if filename.is_none() {
            let file_path = &value[1..];
            filename = Some(
                file_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(file_path)
                    .to_string(),
            );
        }
    }

    Some(FormField {
        name,
        value,
        filename,
        content_type,
    })
}

/// Generate multipart/form-data body
/// Returns (body_string, content_type_with_boundary)
pub fn generate_multipart_body(
    fields: &[FormField],
    file_contents: &HashMap<String, String>,
) -> (String, String) {
    let boundary = "----CurlFormBoundary7ma4d";
    let mut parts = String::new();

    for field in fields {
        let mut value = field.value.clone();

        // Replace file references with content
        if value.starts_with('@') || value.starts_with('<') {
            let file_path = &value[1..];
            value = file_contents.get(file_path).cloned().unwrap_or_default();
        }

        parts.push_str(&format!("--{}\r\n", boundary));
        if let Some(ref fname) = field.filename {
            parts.push_str(&format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                field.name, fname
            ));
            if let Some(ref ct) = field.content_type {
                parts.push_str(&format!("Content-Type: {}\r\n", ct));
            }
        } else {
            parts.push_str(&format!(
                "Content-Disposition: form-data; name=\"{}\"\r\n",
                field.name
            ));
        }
        parts.push_str(&format!("\r\n{}\r\n", value));
    }

    parts.push_str(&format!("--{}--\r\n", boundary));

    let content_type = format!("multipart/form-data; boundary={}", boundary);
    (parts, content_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_form_data_name_value() {
        assert_eq!(encode_form_data("name=hello world"), "name=hello%20world");
    }

    #[test]
    fn test_encode_form_data_value_only() {
        assert_eq!(encode_form_data("=hello world"), "hello%20world");
    }

    #[test]
    fn test_encode_form_data_plain() {
        assert_eq!(encode_form_data("hello world"), "hello%20world");
    }

    #[test]
    fn test_parse_form_field_simple() {
        let field = parse_form_field("name=value").unwrap();
        assert_eq!(field.name, "name");
        assert_eq!(field.value, "value");
        assert!(field.filename.is_none());
        assert!(field.content_type.is_none());
    }

    #[test]
    fn test_parse_form_field_file_upload() {
        let field = parse_form_field("file=@upload.txt").unwrap();
        assert_eq!(field.name, "file");
        assert_eq!(field.value, "@upload.txt");
        assert_eq!(field.filename.as_deref(), Some("upload.txt"));
    }

    #[test]
    fn test_parse_form_field_with_type() {
        let field = parse_form_field("file=@photo.jpg;type=image/jpeg").unwrap();
        assert_eq!(field.name, "file");
        assert_eq!(field.value, "@photo.jpg");
        assert_eq!(field.content_type.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn test_parse_form_field_with_filename() {
        let field = parse_form_field("file=@data.bin;filename=custom.bin").unwrap();
        assert_eq!(field.name, "file");
        assert_eq!(field.filename.as_deref(), Some("custom.bin"));
    }

    #[test]
    fn test_generate_multipart_body() {
        let fields = vec![FormField {
            name: "field1".to_string(),
            value: "value1".to_string(),
            filename: None,
            content_type: None,
        }];
        let file_contents = HashMap::new();
        let (body, ct) = generate_multipart_body(&fields, &file_contents);
        assert!(ct.starts_with("multipart/form-data; boundary="));
        assert!(body.contains("Content-Disposition: form-data; name=\"field1\""));
        assert!(body.contains("value1"));
    }

    #[test]
    fn test_generate_multipart_body_with_file() {
        let fields = vec![FormField {
            name: "file".to_string(),
            value: "@test.txt".to_string(),
            filename: Some("test.txt".to_string()),
            content_type: None,
        }];
        let mut file_contents = HashMap::new();
        file_contents.insert("test.txt".to_string(), "file content here".to_string());
        let (body, _ct) = generate_multipart_body(&fields, &file_contents);
        assert!(body.contains("filename=\"test.txt\""));
        assert!(body.contains("file content here"));
    }

    #[test]
    fn test_url_encoding_special_chars() {
        assert_eq!(encode_form_data("key=a&b=c"), "key=a%26b%3Dc");
    }
}