//! shift - Shift positional parameters
//!
//! shift [n]
//!
//! Shifts positional parameters to the left by n (default 1).
//! $n+1 becomes $1, $n+2 becomes $2, etc.
//! $# is decremented by n.
//!
//! In POSIX mode (set -o posix), errors from shift (like shift count
//! exceeding available parameters) cause the script to exit immediately.

use crate::interpreter::errors::{InterpreterError, PosixFatalError};
use crate::interpreter::types::InterpreterState;
use super::break_cmd::BuiltinResult;

/// Handle the shift builtin command.
///
/// # Arguments
/// * `state` - The interpreter state (mutable for modifying env)
/// * `args` - Command arguments
///
/// # Returns
/// Ok(BuiltinResult) for success/failure, Err for POSIX fatal errors
pub fn handle_shift(state: &mut InterpreterState, args: &[String]) -> Result<BuiltinResult, InterpreterError> {
    // Default shift count is 1
    let mut n = 1i32;

    if !args.is_empty() {
        match args[0].parse::<i32>() {
            Ok(parsed) if parsed >= 0 => {
                n = parsed;
            }
            _ => {
                let error_msg = format!("bash: shift: {}: numeric argument required\n", args[0]);
                // In POSIX mode, this error is fatal
                if state.options.posix {
                    return Err(PosixFatalError::new(1, String::new(), error_msg).into());
                }
                return Ok(BuiltinResult::failure(&error_msg, 1));
            }
        }
    }

    // Get current positional parameter count
    let current_count: i32 = state.env.get("#")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Check if shift count exceeds available parameters
    if n > current_count {
        let error_msg = "bash: shift: shift count out of range\n".to_string();
        // In POSIX mode, this error is fatal
        if state.options.posix {
            return Err(PosixFatalError::new(1, String::new(), error_msg).into());
        }
        return Ok(BuiltinResult::failure(&error_msg, 1));
    }

    // If n is 0, do nothing
    if n == 0 {
        return Ok(BuiltinResult::ok());
    }

    // Get current positional parameters
    let mut params: Vec<String> = Vec::new();
    for i in 1..=current_count {
        params.push(state.env.get(&i.to_string()).cloned().unwrap_or_default());
    }

    // Remove first n parameters
    let new_params: Vec<String> = params.into_iter().skip(n as usize).collect();

    // Clear all old positional parameters
    for i in 1..=current_count {
        state.env.remove(&i.to_string());
    }

    // Set new positional parameters
    for (i, param) in new_params.iter().enumerate() {
        state.env.insert((i + 1).to_string(), param.clone());
    }

    // Update $# and $@
    state.env.insert("#".to_string(), new_params.len().to_string());
    state.env.insert("@".to_string(), new_params.join(" "));

    Ok(BuiltinResult::ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state_with_params(params: &[&str]) -> InterpreterState {
        let mut state = InterpreterState::default();
        state.env.insert("#".to_string(), params.len().to_string());
        for (i, param) in params.iter().enumerate() {
            state.env.insert((i + 1).to_string(), param.to_string());
        }
        state.env.insert("@".to_string(), params.join(" "));
        state
    }

    #[test]
    fn test_shift_default() {
        let mut state = make_state_with_params(&["a", "b", "c"]);
        let result = handle_shift(&mut state, &[]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("#").unwrap(), "2");
        assert_eq!(state.env.get("1").unwrap(), "b");
        assert_eq!(state.env.get("2").unwrap(), "c");
        assert!(state.env.get("3").is_none());
    }

    #[test]
    fn test_shift_by_n() {
        let mut state = make_state_with_params(&["a", "b", "c", "d"]);
        let result = handle_shift(&mut state, &["2".to_string()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("#").unwrap(), "2");
        assert_eq!(state.env.get("1").unwrap(), "c");
        assert_eq!(state.env.get("2").unwrap(), "d");
    }

    #[test]
    fn test_shift_zero() {
        let mut state = make_state_with_params(&["a", "b"]);
        let result = handle_shift(&mut state, &["0".to_string()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("#").unwrap(), "2");
        assert_eq!(state.env.get("1").unwrap(), "a");
    }

    #[test]
    fn test_shift_all() {
        let mut state = make_state_with_params(&["a", "b"]);
        let result = handle_shift(&mut state, &["2".to_string()]).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(state.env.get("#").unwrap(), "0");
        assert!(state.env.get("1").is_none());
    }

    #[test]
    fn test_shift_out_of_range() {
        let mut state = make_state_with_params(&["a", "b"]);
        let result = handle_shift(&mut state, &["5".to_string()]).unwrap();
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("out of range"));
    }

    #[test]
    fn test_shift_invalid_arg() {
        let mut state = make_state_with_params(&["a", "b"]);
        let result = handle_shift(&mut state, &["abc".to_string()]).unwrap();
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("numeric argument required"));
    }

    #[test]
    fn test_shift_negative() {
        let mut state = make_state_with_params(&["a", "b"]);
        let result = handle_shift(&mut state, &["-1".to_string()]).unwrap();
        assert_eq!(result.exit_code, 1);
        assert!(result.stderr.contains("numeric argument required"));
    }

    #[test]
    fn test_shift_posix_fatal() {
        let mut state = make_state_with_params(&["a"]);
        state.options.posix = true;
        let result = handle_shift(&mut state, &["5".to_string()]);
        assert!(result.is_err());
        match result.unwrap_err() {
            InterpreterError::PosixFatal(e) => {
                assert_eq!(e.exit_code, 1);
                assert!(e.stderr.contains("out of range"));
            }
            _ => panic!("Expected PosixFatalError"),
        }
    }
}
