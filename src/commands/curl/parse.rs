/// Option parsing for curl command

use super::form::{encode_form_data, parse_form_field};
use super::types::CurlOptions;

fn set_post_if_get(options: &mut CurlOptions) {
    if options.method == "GET" {
        options.method = "POST".to_string();
    }
}

fn parse_header_str(header: &str, options: &mut CurlOptions) {
    if let Some(colon_idx) = header.find(':') {
        if colon_idx > 0 {
            let name = header[..colon_idx].trim().to_string();
            let value = header[colon_idx + 1..].trim().to_string();
            options.headers.push((name, value));
        }
    }
}

fn accumulate_data(options: &mut CurlOptions, new_data: &str) {
    if let Some(ref existing) = options.data {
        options.data = Some(format!("{}&{}", existing, new_data));
    } else {
        options.data = Some(new_data.to_string());
    }
}

/// Parse curl command-line arguments
pub fn parse_options(args: &[String]) -> Result<CurlOptions, String> {
    let mut options = CurlOptions::default();
    let mut i = 0;

    while i < args.len() {
        let arg = args[i].clone();

        if arg == "-X" || arg == "--request" {
            i += 1;
            options.method = args.get(i).cloned().unwrap_or_else(|| "GET".to_string());
        } else if arg.starts_with("-X") && arg.len() > 2 && !arg.starts_with("--") {
            options.method = arg[2..].to_string();
        } else if let Some(val) = arg.strip_prefix("--request=") {
            options.method = val.to_string();
        } else if arg == "-H" || arg == "--header" {
            i += 1;
            if let Some(header) = args.get(i) {
                parse_header_str(header, &mut options);
            }
        } else if let Some(val) = arg.strip_prefix("--header=") {
            parse_header_str(val, &mut options);
        } else if arg == "-d" || arg == "--data" || arg == "--data-raw" {
            i += 1;
            let val = args.get(i).cloned().unwrap_or_default();
            accumulate_data(&mut options, &val);
            set_post_if_get(&mut options);
        } else if arg.starts_with("-d") && arg.len() > 2 && !arg.starts_with("--") {
            accumulate_data(&mut options, &arg[2..]);
            set_post_if_get(&mut options);
        } else if let Some(val) = arg.strip_prefix("--data=") {
            accumulate_data(&mut options, val);
            set_post_if_get(&mut options);
        } else if let Some(val) = arg.strip_prefix("--data-raw=") {
            accumulate_data(&mut options, val);
            set_post_if_get(&mut options);
        } else if arg == "--data-binary" {
            i += 1;
            let val = args.get(i).cloned().unwrap_or_default();
            accumulate_data(&mut options, &val);
            options.data_binary = true;
            set_post_if_get(&mut options);
        } else if let Some(val) = arg.strip_prefix("--data-binary=") {
            accumulate_data(&mut options, val);
            options.data_binary = true;
            set_post_if_get(&mut options);
        } else if arg == "--data-urlencode" {
            i += 1;
            let val = args.get(i).cloned().unwrap_or_default();
            let encoded = encode_form_data(&val);
            accumulate_data(&mut options, &encoded);
            set_post_if_get(&mut options);
        } else if let Some(val) = arg.strip_prefix("--data-urlencode=") {
            let encoded = encode_form_data(val);
            accumulate_data(&mut options, &encoded);
            set_post_if_get(&mut options);
        } else if arg == "-F" || arg == "--form" {
            i += 1;
            let form_data = args.get(i).cloned().unwrap_or_default();
            if let Some(field) = parse_form_field(&form_data) {
                options.form_fields.push(field);
            }
            set_post_if_get(&mut options);
        } else if let Some(val) = arg.strip_prefix("--form=") {
            if let Some(field) = parse_form_field(val) {
                options.form_fields.push(field);
            }
            set_post_if_get(&mut options);
        } else if arg == "-u" || arg == "--user" {
            i += 1;
            options.user = args.get(i).cloned();
        } else if arg.starts_with("-u") && arg.len() > 2 && !arg.starts_with("--") {
            options.user = Some(arg[2..].to_string());
        } else if let Some(val) = arg.strip_prefix("--user=") {
            options.user = Some(val.to_string());
        } else if arg == "-A" || arg == "--user-agent" {
            i += 1;
            let val = args.get(i).cloned().unwrap_or_default();
            options.headers.push(("User-Agent".to_string(), val));
        } else if arg.starts_with("-A") && arg.len() > 2 && !arg.starts_with("--") {
            options.headers.push(("User-Agent".to_string(), arg[2..].to_string()));
        } else if let Some(val) = arg.strip_prefix("--user-agent=") {
            options.headers.push(("User-Agent".to_string(), val.to_string()));
        } else if arg == "-e" || arg == "--referer" {
            i += 1;
            let val = args.get(i).cloned().unwrap_or_default();
            options.headers.push(("Referer".to_string(), val));
        } else if arg.starts_with("-e") && arg.len() > 2 && !arg.starts_with("--") {
            options.headers.push(("Referer".to_string(), arg[2..].to_string()));
        } else if let Some(val) = arg.strip_prefix("--referer=") {
            options.headers.push(("Referer".to_string(), val.to_string()));
        } else if arg == "-b" || arg == "--cookie" {
            i += 1;
            let val = args.get(i).cloned().unwrap_or_default();
            options.headers.push(("Cookie".to_string(), val));
        } else if arg.starts_with("-b") && arg.len() > 2 && !arg.starts_with("--") {
            options.headers.push(("Cookie".to_string(), arg[2..].to_string()));
        } else if let Some(val) = arg.strip_prefix("--cookie=") {
            options.headers.push(("Cookie".to_string(), val.to_string()));
        } else if arg == "-c" || arg == "--cookie-jar" {
            i += 1;
            options.cookie_jar = args.get(i).cloned();
        } else if let Some(val) = arg.strip_prefix("--cookie-jar=") {
            options.cookie_jar = Some(val.to_string());
        } else if arg == "-T" || arg == "--upload-file" {
            i += 1;
            options.upload_file = args.get(i).cloned();
            if options.method == "GET" {
                options.method = "PUT".to_string();
            }
        } else if let Some(val) = arg.strip_prefix("--upload-file=") {
            options.upload_file = Some(val.to_string());
            if options.method == "GET" {
                options.method = "PUT".to_string();
            }
        } else if arg == "-m" || arg == "--max-time" {
            i += 1;
            if let Some(val) = args.get(i) {
                if let Ok(secs) = val.parse::<f64>() {
                    if secs > 0.0 {
                        options.timeout_ms = Some((secs * 1000.0) as u64);
                    }
                }
            }
        } else if let Some(val) = arg.strip_prefix("--max-time=") {
            if let Ok(secs) = val.parse::<f64>() {
                if secs > 0.0 {
                    options.timeout_ms = Some((secs * 1000.0) as u64);
                }
            }
        } else if arg == "--connect-timeout" {
            i += 1;
            if let Some(val) = args.get(i) {
                if let Ok(secs) = val.parse::<f64>() {
                    if secs > 0.0 && options.timeout_ms.is_none() {
                        options.timeout_ms = Some((secs * 1000.0) as u64);
                    }
                }
            }
        } else if let Some(val) = arg.strip_prefix("--connect-timeout=") {
            if let Ok(secs) = val.parse::<f64>() {
                if secs > 0.0 && options.timeout_ms.is_none() {
                    options.timeout_ms = Some((secs * 1000.0) as u64);
                }
            }
        } else if arg == "-o" || arg == "--output" {
            i += 1;
            options.output_file = args.get(i).cloned();
        } else if let Some(val) = arg.strip_prefix("--output=") {
            options.output_file = Some(val.to_string());
        } else if arg == "-O" || arg == "--remote-name" {
            options.use_remote_name = true;
        } else if arg == "-I" || arg == "--head" {
            options.head_only = true;
            options.method = "HEAD".to_string();
        } else if arg == "-i" || arg == "--include" {
            options.include_headers = true;
        } else if arg == "-s" || arg == "--silent" {
            options.silent = true;
        } else if arg == "-S" || arg == "--show-error" {
            options.show_error = true;
        } else if arg == "-f" || arg == "--fail" {
            options.fail_silently = true;
        } else if arg == "-L" || arg == "--location" {
            options.follow_redirects = true;
        } else if arg == "--max-redirs" {
            i += 1; // skip value
        } else if arg.starts_with("--max-redirs=") {
            // skip
        } else if arg == "-w" || arg == "--write-out" {
            i += 1;
            options.write_out = args.get(i).cloned();
        } else if let Some(val) = arg.strip_prefix("--write-out=") {
            options.write_out = Some(val.to_string());
        } else if arg == "-v" || arg == "--verbose" {
            options.verbose = true;
        } else if arg.starts_with("--") && arg != "--" {
            return Err(format!("curl: unknown option: {}", arg));
        } else if arg.starts_with('-') && arg != "-" && arg.len() > 1 {
            // Handle combined short options like -sSf
            for c in arg[1..].chars() {
                match c {
                    's' => options.silent = true,
                    'S' => options.show_error = true,
                    'f' => options.fail_silently = true,
                    'L' => options.follow_redirects = true,
                    'I' => {
                        options.head_only = true;
                        options.method = "HEAD".to_string();
                    }
                    'i' => options.include_headers = true,
                    'O' => options.use_remote_name = true,
                    'v' => options.verbose = true,
                    _ => return Err(format!("curl: unknown option: -{}", c)),
                }
            }
        } else if !arg.starts_with('-') {
            options.url = Some(arg);
        }

        i += 1;
    }

    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn test_parse_simple_url() {
        let opts = parse_options(&args(&["https://example.com"])).unwrap();
        assert_eq!(opts.url.as_deref(), Some("https://example.com"));
        assert_eq!(opts.method, "GET");
    }

    #[test]
    fn test_parse_post_with_data() {
        let opts = parse_options(&args(&["-X", "POST", "-d", "data", "https://example.com"])).unwrap();
        assert_eq!(opts.method, "POST");
        assert_eq!(opts.data.as_deref(), Some("data"));
    }

    #[test]
    fn test_parse_header() {
        let opts = parse_options(&args(&["-H", "Content-Type: application/json", "https://example.com"])).unwrap();
        assert_eq!(opts.headers.len(), 1);
        assert_eq!(opts.headers[0].0, "Content-Type");
        assert_eq!(opts.headers[0].1, "application/json");
    }

    #[test]
    fn test_parse_combined_flags() {
        let opts = parse_options(&args(&["-sSf", "https://example.com"])).unwrap();
        assert!(opts.silent);
        assert!(opts.show_error);
        assert!(opts.fail_silently);
    }

    #[test]
    fn test_parse_output_file() {
        let opts = parse_options(&args(&["-o", "output.txt", "https://example.com"])).unwrap();
        assert_eq!(opts.output_file.as_deref(), Some("output.txt"));
    }

    #[test]
    fn test_parse_form_field() {
        let opts = parse_options(&args(&["-F", "file=@upload.txt", "https://example.com"])).unwrap();
        assert_eq!(opts.form_fields.len(), 1);
        assert_eq!(opts.form_fields[0].name, "file");
        assert_eq!(opts.method, "POST");
    }

    #[test]
    fn test_data_switches_to_post() {
        let opts = parse_options(&args(&["-d", "key=value", "https://example.com"])).unwrap();
        assert_eq!(opts.method, "POST");
        assert_eq!(opts.data.as_deref(), Some("key=value"));
    }

    #[test]
    fn test_multiple_data_flags_accumulate() {
        let opts = parse_options(&args(&["-d", "a=1", "-d", "b=2", "https://example.com"])).unwrap();
        assert_eq!(opts.data.as_deref(), Some("a=1&b=2"));
    }

    #[test]
    fn test_upload_file_switches_to_put() {
        let opts = parse_options(&args(&["-T", "file.txt", "https://example.com"])).unwrap();
        assert_eq!(opts.method, "PUT");
        assert_eq!(opts.upload_file.as_deref(), Some("file.txt"));
    }

    #[test]
    fn test_head_flag() {
        let opts = parse_options(&args(&["-I", "https://example.com"])).unwrap();
        assert!(opts.head_only);
        assert_eq!(opts.method, "HEAD");
    }

    #[test]
    fn test_long_option_with_equals() {
        let opts = parse_options(&args(&["--data=hello", "https://example.com"])).unwrap();
        assert_eq!(opts.data.as_deref(), Some("hello"));
        assert_eq!(opts.method, "POST");
    }

    #[test]
    fn test_max_time() {
        let opts = parse_options(&args(&["--max-time", "30", "https://example.com"])).unwrap();
        assert_eq!(opts.timeout_ms, Some(30000));
    }

    #[test]
    fn test_user_auth() {
        let opts = parse_options(&args(&["-u", "user:pass", "https://example.com"])).unwrap();
        assert_eq!(opts.user.as_deref(), Some("user:pass"));
    }

    #[test]
    fn test_data_urlencode() {
        let opts = parse_options(&args(&["--data-urlencode", "name=hello world", "https://example.com"])).unwrap();
        assert_eq!(opts.data.as_deref(), Some("name=hello%20world"));
        assert_eq!(opts.method, "POST");
    }

    #[test]
    fn test_unknown_long_option() {
        let result = parse_options(&args(&["--unknown-flag", "https://example.com"]));
        assert!(result.is_err());
    }

    #[test]
    fn test_write_out() {
        let opts = parse_options(&args(&["-w", "%{http_code}", "https://example.com"])).unwrap();
        assert_eq!(opts.write_out.as_deref(), Some("%{http_code}"));
    }
}
