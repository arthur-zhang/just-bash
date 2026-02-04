//! Prompt expansion
//!
//! Handles prompt escape sequences for ${var@P} transformation and PS1/PS2/PS3/PS4.

use crate::interpreter::InterpreterState;
use chrono::{Datelike, Local, Timelike};

/// Simple strftime implementation for prompt \D{format}
/// Only supports common format specifiers
fn simple_strftime(format: &str, now: &chrono::DateTime<Local>) -> String {
    // If format is empty, use locale default time format (like %X)
    if format.is_empty() {
        return format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());
    }

    let mut result = String::new();
    let chars: Vec<char> = format.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '%' {
            if i + 1 >= chars.len() {
                result.push('%');
                i += 1;
                continue;
            }
            let spec = chars[i + 1];
            match spec {
                'H' => result.push_str(&format!("{:02}", now.hour())),
                'M' => result.push_str(&format!("{:02}", now.minute())),
                'S' => result.push_str(&format!("{:02}", now.second())),
                'd' => result.push_str(&format!("{:02}", now.day())),
                'm' => result.push_str(&format!("{:02}", now.month())),
                'Y' => result.push_str(&format!("{}", now.year())),
                'y' => result.push_str(&format!("{:02}", now.year() % 100)),
                'I' => {
                    let mut h = now.hour() % 12;
                    if h == 0 {
                        h = 12;
                    }
                    result.push_str(&format!("{:02}", h));
                }
                'p' => result.push_str(if now.hour() < 12 { "AM" } else { "PM" }),
                'P' => result.push_str(if now.hour() < 12 { "am" } else { "pm" }),
                '%' => result.push('%'),
                'a' => {
                    let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
                    result.push_str(days[now.weekday().num_days_from_sunday() as usize]);
                }
                'b' => {
                    let months = [
                        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct",
                        "Nov", "Dec",
                    ];
                    result.push_str(months[(now.month() - 1) as usize]);
                }
                _ => {
                    // Unknown specifier - pass through
                    result.push('%');
                    result.push(spec);
                }
            }
            i += 2;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Expand prompt escape sequences (${var@P} transformation)
/// Interprets backslash escapes used in PS1, PS2, PS3, PS4 prompt strings.
///
/// Supported escapes:
/// - \a - bell (ASCII 07)
/// - \e - escape (ASCII 033)
/// - \n - newline
/// - \r - carriage return
/// - \\ - literal backslash
/// - \$ - $ for regular user, # for root (always $ here)
/// - \[ and \] - non-printing sequence delimiters (removed)
/// - \u - username
/// - \h - short hostname (up to first .)
/// - \H - full hostname
/// - \w - current working directory
/// - \W - basename of current working directory
/// - \d - date (Weekday Month Day format)
/// - \t - time HH:MM:SS (24-hour)
/// - \T - time HH:MM:SS (12-hour)
/// - \@ - time HH:MM AM/PM (12-hour)
/// - \A - time HH:MM (24-hour)
/// - \D{format} - strftime format
/// - \s - shell name
/// - \v - bash version (major.minor)
/// - \V - bash version (major.minor.patch)
/// - \j - number of jobs
/// - \l - terminal device basename
/// - \# - command number
/// - \! - history number
/// - \NNN - octal character code
pub fn expand_prompt(state: &InterpreterState, value: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;

    // Get environment values for prompt escapes
    let user = state
        .env
        .get("USER")
        .or_else(|| state.env.get("LOGNAME"))
        .map(|s| s.as_str())
        .unwrap_or("user");
    let hostname = state
        .env
        .get("HOSTNAME")
        .map(|s| s.as_str())
        .unwrap_or("localhost");
    let short_host = hostname.split('.').next().unwrap_or(hostname);
    let pwd = state.env.get("PWD").map(|s| s.as_str()).unwrap_or("/");
    let home = state.env.get("HOME").map(|s| s.as_str()).unwrap_or("/");

    // Replace $HOME with ~ in pwd for \w
    let tilde_expanded = if pwd.starts_with(home) && !home.is_empty() {
        format!("~{}", &pwd[home.len()..])
    } else {
        pwd.to_string()
    };
    let pwd_basename = pwd.rsplit('/').next().unwrap_or(pwd);

    // Get date/time values
    let now = Local::now();
    let weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    // Command number (we'll use a simple counter from the state if available)
    let cmd_num = state
        .env
        .get("__COMMAND_NUMBER")
        .map(|s| s.as_str())
        .unwrap_or("1");

    while i < chars.len() {
        let c = chars[i];

        if c == '\\' {
            if i + 1 >= chars.len() {
                // Trailing backslash
                result.push('\\');
                i += 1;
                continue;
            }

            let next = chars[i + 1];

            // Check for octal escape \NNN (1-3 digits)
            if next >= '0' && next <= '7' {
                let mut octal_str = String::new();
                let mut j = i + 1;
                while j < chars.len() && j < i + 4 && chars[j] >= '0' && chars[j] <= '7' {
                    octal_str.push(chars[j]);
                    j += 1;
                }
                // Parse octal, wrap around at 256
                let code = u32::from_str_radix(&octal_str, 8).unwrap_or(0) % 256;
                if let Some(ch) = char::from_u32(code) {
                    result.push(ch);
                }
                i = j;
                continue;
            }

            match next {
                '\\' => {
                    result.push('\\');
                    i += 2;
                }
                'a' => {
                    result.push('\x07'); // Bell
                    i += 2;
                }
                'e' => {
                    result.push('\x1b'); // Escape
                    i += 2;
                }
                'n' => {
                    result.push('\n');
                    i += 2;
                }
                'r' => {
                    result.push('\r');
                    i += 2;
                }
                '$' => {
                    // $ for regular user, # for root - we always use $ since we're not running as root
                    result.push('$');
                    i += 2;
                }
                '[' | ']' => {
                    // Non-printing sequence delimiters - just remove them
                    i += 2;
                }
                'u' => {
                    result.push_str(user);
                    i += 2;
                }
                'h' => {
                    result.push_str(short_host);
                    i += 2;
                }
                'H' => {
                    result.push_str(hostname);
                    i += 2;
                }
                'w' => {
                    result.push_str(&tilde_expanded);
                    i += 2;
                }
                'W' => {
                    result.push_str(pwd_basename);
                    i += 2;
                }
                'd' => {
                    // Date: Weekday Month Day
                    let day_str = format!("{:2}", now.day());
                    result.push_str(&format!(
                        "{} {} {}",
                        weekdays[now.weekday().num_days_from_sunday() as usize],
                        months[(now.month() - 1) as usize],
                        day_str
                    ));
                    i += 2;
                }
                't' => {
                    // Time: HH:MM:SS (24-hour)
                    result.push_str(&format!(
                        "{:02}:{:02}:{:02}",
                        now.hour(),
                        now.minute(),
                        now.second()
                    ));
                    i += 2;
                }
                'T' => {
                    // Time: HH:MM:SS (12-hour)
                    let mut h = now.hour() % 12;
                    if h == 0 {
                        h = 12;
                    }
                    result.push_str(&format!("{:02}:{:02}:{:02}", h, now.minute(), now.second()));
                    i += 2;
                }
                '@' => {
                    // Time: HH:MM AM/PM (12-hour)
                    let mut h = now.hour() % 12;
                    if h == 0 {
                        h = 12;
                    }
                    let ampm = if now.hour() < 12 { "AM" } else { "PM" };
                    result.push_str(&format!("{:02}:{:02} {}", h, now.minute(), ampm));
                    i += 2;
                }
                'A' => {
                    // Time: HH:MM (24-hour)
                    result.push_str(&format!("{:02}:{:02}", now.hour(), now.minute()));
                    i += 2;
                }
                'D' => {
                    // strftime format: \D{format}
                    if i + 2 < chars.len() && chars[i + 2] == '{' {
                        let rest: String = chars[i + 3..].iter().collect();
                        if let Some(close_idx) = rest.find('}') {
                            let format: String = chars[i + 3..i + 3 + close_idx].iter().collect();
                            result.push_str(&simple_strftime(&format, &now));
                            i = i + 3 + close_idx + 1;
                        } else {
                            // No closing brace - treat literally
                            result.push_str("\\D");
                            i += 2;
                        }
                    } else {
                        result.push_str("\\D");
                        i += 2;
                    }
                }
                's' => {
                    // Shell name
                    result.push_str("bash");
                    i += 2;
                }
                'v' => {
                    // Version: major.minor
                    result.push_str("5.0");
                    i += 2;
                }
                'V' => {
                    // Version: major.minor.patch
                    result.push_str("5.0.0");
                    i += 2;
                }
                'j' => {
                    // Number of jobs - we don't track jobs, so return 0
                    result.push('0');
                    i += 2;
                }
                'l' => {
                    // Terminal device basename - we're not in a real terminal
                    result.push_str("tty");
                    i += 2;
                }
                '#' => {
                    // Command number
                    result.push_str(cmd_num);
                    i += 2;
                }
                '!' => {
                    // History number - same as command number
                    result.push_str(cmd_num);
                    i += 2;
                }
                'x' => {
                    // \xNN hex literals are NOT supported in bash prompt expansion
                    // Just pass through as literal
                    result.push_str("\\x");
                    i += 2;
                }
                _ => {
                    // Unknown escape - pass through as literal
                    result.push('\\');
                    result.push(next);
                    i += 2;
                }
            }
        } else {
            result.push(c);
            i += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_state() -> InterpreterState {
        let mut env = HashMap::new();
        env.insert("USER".to_string(), "testuser".to_string());
        env.insert("HOSTNAME".to_string(), "myhost.example.com".to_string());
        env.insert("PWD".to_string(), "/home/testuser/projects".to_string());
        env.insert("HOME".to_string(), "/home/testuser".to_string());
        InterpreterState {
            env,
            ..Default::default()
        }
    }

    #[test]
    fn test_simple_escapes() {
        let state = make_state();
        assert_eq!(expand_prompt(&state, "\\n"), "\n");
        assert_eq!(expand_prompt(&state, "\\r"), "\r");
        assert_eq!(expand_prompt(&state, "\\\\"), "\\");
        assert_eq!(expand_prompt(&state, "\\$"), "$");
    }

    #[test]
    fn test_user_and_host() {
        let state = make_state();
        assert_eq!(expand_prompt(&state, "\\u"), "testuser");
        assert_eq!(expand_prompt(&state, "\\h"), "myhost");
        assert_eq!(expand_prompt(&state, "\\H"), "myhost.example.com");
    }

    #[test]
    fn test_directory() {
        let state = make_state();
        assert_eq!(expand_prompt(&state, "\\w"), "~/projects");
        assert_eq!(expand_prompt(&state, "\\W"), "projects");
    }

    #[test]
    fn test_shell_info() {
        let state = make_state();
        assert_eq!(expand_prompt(&state, "\\s"), "bash");
        assert_eq!(expand_prompt(&state, "\\v"), "5.0");
        assert_eq!(expand_prompt(&state, "\\V"), "5.0.0");
    }

    #[test]
    fn test_octal_escape() {
        let state = make_state();
        assert_eq!(expand_prompt(&state, "\\101"), "A"); // 101 octal = 65 decimal = 'A'
        assert_eq!(expand_prompt(&state, "\\141"), "a"); // 141 octal = 97 decimal = 'a'
    }

    #[test]
    fn test_non_printing_delimiters() {
        let state = make_state();
        assert_eq!(expand_prompt(&state, "\\[\\e[32m\\]test\\[\\e[0m\\]"), "\x1b[32mtest\x1b[0m");
    }
}
