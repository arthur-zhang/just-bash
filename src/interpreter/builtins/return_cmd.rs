//! return - Return from a function with an exit code

use crate::interpreter::errors::{ReturnError, InterpreterError};
use crate::interpreter::types::InterpreterState;
use super::break_cmd::BuiltinResult;

/// Handle the return builtin command.
///
/// # Arguments
/// * `state` - The interpreter state
/// * `args` - Command arguments
///
/// # Returns
/// Ok(BuiltinResult) for error cases, Err(InterpreterError) for control flow
pub fn handle_return(state: &InterpreterState, args: &[String]) -> Result<BuiltinResult, InterpreterError> {
    // Check if we're in a function or sourced script
    if state.call_depth == 0 && state.source_depth == 0 {
        return Ok(BuiltinResult::failure(
            "bash: return: can only `return' from a function or sourced script\n",
            1,
        ));
    }

    let mut exit_code = state.last_exit_code;
    if !args.is_empty() {
        let arg = &args[0];
        // Empty string or non-numeric is an error
        if arg.is_empty() || !arg.chars().all(|c| c.is_ascii_digit() || c == '-') {
            return Ok(BuiltinResult::failure(
                &format!("bash: return: {}: numeric argument required\n", arg),
                2,
            ));
        }
        match arg.parse::<i32>() {
            Ok(n) => {
                // Bash uses modulo 256 for exit codes
                exit_code = ((n % 256) + 256) % 256;
            }
            Err(_) => {
                return Ok(BuiltinResult::failure(
                    &format!("bash: return: {}: numeric argument required\n", arg),
                    2,
                ));
            }
        }
    }

    Err(ReturnError::new(exit_code, String::new(), String::new()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> InterpreterState {
        InterpreterState::default()
    }

    #[test]
    fn test_return_outside_function() {
        let state = make_state();
        let result = handle_return(&state, &[]);
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.exit_code, 1);
        assert!(r.stderr.contains("can only `return'"));
    }

    #[test]
    fn test_return_in_function() {
        let mut state = make_state();
        state.call_depth = 1;
        let result = handle_return(&state, &[]);
        assert!(result.is_err());
        match result.unwrap_err() {
            InterpreterError::Return(e) => assert_eq!(e.exit_code, 0),
            _ => panic!("Expected ReturnError"),
        }
    }

    #[test]
    fn test_return_with_exit_code() {
        let mut state = make_state();
        state.call_depth = 1;
        let result = handle_return(&state, &["42".to_string()]);
        assert!(result.is_err());
        match result.unwrap_err() {
            InterpreterError::Return(e) => assert_eq!(e.exit_code, 42),
            _ => panic!("Expected ReturnError"),
        }
    }

    #[test]
    fn test_return_with_negative_exit_code() {
        let mut state = make_state();
        state.call_depth = 1;
        let result = handle_return(&state, &["-1".to_string()]);
        assert!(result.is_err());
        match result.unwrap_err() {
            InterpreterError::Return(e) => assert_eq!(e.exit_code, 255),
            _ => panic!("Expected ReturnError"),
        }
    }

    #[test]
    fn test_return_in_sourced_script() {
        let mut state = make_state();
        state.source_depth = 1;
        let result = handle_return(&state, &["5".to_string()]);
        assert!(result.is_err());
        match result.unwrap_err() {
            InterpreterError::Return(e) => assert_eq!(e.exit_code, 5),
            _ => panic!("Expected ReturnError"),
        }
    }
}
