//! let - Evaluate arithmetic expressions
//!
//! Usage:
//!   let expr [expr ...]
//!   let "x=1" "y=x+2"
//!
//! Each argument is evaluated as an arithmetic expression.
//! Returns 0 if the last expression evaluates to non-zero,
//! returns 1 if it evaluates to zero.
//!
//! Note: In bash, `let x=( 1 )` passes separate args ["x=(", "1", ")"]
//! when not quoted. The let builtin needs to handle this by joining
//! arguments that are part of the same expression.

use crate::interpreter::types::InterpreterState;
use crate::interpreter::arithmetic::evaluate_arithmetic;
use crate::parser::parse_arith_expr;

/// Result type for builtin commands
pub type BuiltinResult = (String, String, i32);

/// Parse arguments into expressions.
/// Handles cases like `let x=( 1 )` where parentheses cause splitting.
fn parse_let_args(args: &[String]) -> Vec<String> {
    let mut expressions = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0i32;

    for arg in args {
        // Count open and close parens in this arg
        for ch in arg.chars() {
            if ch == '(' {
                paren_depth += 1;
            } else if ch == ')' {
                paren_depth -= 1;
            }
        }

        if !current.is_empty() {
            current.push(' ');
            current.push_str(arg);
        } else {
            current = arg.clone();
        }

        // If parens are balanced, this is a complete expression
        if paren_depth == 0 {
            expressions.push(current);
            current = String::new();
        }
    }

    // Handle any remaining (unbalanced parens treated as single expression)
    if !current.is_empty() {
        expressions.push(current);
    }

    expressions
}

/// Handle the `let` builtin command.
///
/// Evaluates each argument as an arithmetic expression.
/// Returns 0 if the last expression evaluates to non-zero, 1 if zero.
pub fn handle_let(state: &mut InterpreterState, args: &[String]) -> BuiltinResult {
    use crate::interpreter::types::{InterpreterContext, ExecutionLimits};

    if args.is_empty() {
        return (String::new(), "bash: let: expression expected\n".to_string(), 1);
    }

    // Parse args into expressions (handling split parentheses)
    let expressions = parse_let_args(args);
    let mut last_result: i64 = 0;

    let limits = ExecutionLimits::default();
    let mut ctx = InterpreterContext::new(state, &limits);

    for expr in &expressions {
        // Parse the expression using the arithmetic parser
        let (arith_expr, pos) = parse_arith_expr(expr, 0);

        // Check for unparsed content (syntax error)
        if pos < expr.len() {
            let unparsed = &expr[pos..];
            let error_token = unparsed.split_whitespace().next().unwrap_or(unparsed);
            return (
                String::new(),
                format!("bash: let: {}: syntax error in expression (error token is \"{}\")\n", expr, error_token),
                1,
            );
        }

        // Evaluate the expression
        match evaluate_arithmetic(&mut ctx, &arith_expr, false, None) {
            Ok(result) => {
                last_result = result;
            }
            Err(err) => {
                return (
                    String::new(),
                    format!("bash: let: {}: {}\n", expr, err.message),
                    1,
                );
            }
        }
    }

    // Return 0 if last expression is non-zero, 1 if zero
    let exit_code = if last_result == 0 { 1 } else { 0 };
    (String::new(), String::new(), exit_code)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_let_args_simple() {
        let args = vec!["x=1".to_string(), "y=2".to_string()];
        let result = parse_let_args(&args);
        assert_eq!(result, vec!["x=1", "y=2"]);
    }

    #[test]
    fn test_parse_let_args_with_parens() {
        // Simulates: let x=( 1 )
        let args = vec!["x=(".to_string(), "1".to_string(), ")".to_string()];
        let result = parse_let_args(&args);
        assert_eq!(result, vec!["x=( 1 )"]);
    }

    #[test]
    fn test_parse_let_args_nested_parens() {
        let args = vec!["x=((1+2))".to_string()];
        let result = parse_let_args(&args);
        assert_eq!(result, vec!["x=((1+2))"]);
    }

    #[test]
    fn test_handle_let_no_args() {
        let mut state = InterpreterState::default();
        let (stdout, stderr, code) = handle_let(&mut state, &[]);
        assert_eq!(code, 1);
        assert!(stderr.contains("expression expected"));
        assert!(stdout.is_empty());
    }

    #[test]
    fn test_handle_let_simple_assignment() {
        let mut state = InterpreterState::default();
        let args = vec!["x=5".to_string()];
        let (stdout, stderr, code) = handle_let(&mut state, &args);
        assert_eq!(code, 0); // 5 is non-zero, so exit code is 0
        assert!(stderr.is_empty());
        assert!(stdout.is_empty());
        assert_eq!(state.env.get("x"), Some(&"5".to_string()));
    }

    #[test]
    fn test_handle_let_zero_result() {
        let mut state = InterpreterState::default();
        let args = vec!["x=0".to_string()];
        let (_, _, code) = handle_let(&mut state, &args);
        assert_eq!(code, 1); // 0 result means exit code 1
    }

    #[test]
    fn test_handle_let_arithmetic() {
        let mut state = InterpreterState::default();
        let args = vec!["x=2+3".to_string()];
        let (_, _, code) = handle_let(&mut state, &args);
        assert_eq!(code, 0);
        assert_eq!(state.env.get("x"), Some(&"5".to_string()));
    }

    #[test]
    fn test_handle_let_multiple_expressions() {
        let mut state = InterpreterState::default();
        let args = vec!["x=1".to_string(), "y=x+1".to_string()];
        let (_, _, code) = handle_let(&mut state, &args);
        assert_eq!(code, 0);
        assert_eq!(state.env.get("x"), Some(&"1".to_string()));
        assert_eq!(state.env.get("y"), Some(&"2".to_string()));
    }

    #[test]
    fn test_handle_let_comparison_true() {
        let mut state = InterpreterState::default();
        let args = vec!["5>3".to_string()];
        let (_, _, code) = handle_let(&mut state, &args);
        assert_eq!(code, 0); // 5>3 is true (1), so exit code is 0
    }

    #[test]
    fn test_handle_let_comparison_false() {
        let mut state = InterpreterState::default();
        let args = vec!["3>5".to_string()];
        let (_, _, code) = handle_let(&mut state, &args);
        assert_eq!(code, 1); // 3>5 is false (0), so exit code is 1
    }
}
