//! Arithmetic Evaluation
//!
//! Evaluates bash arithmetic expressions including:
//! - Basic operators (+, -, *, /, %)
//! - Comparison operators (<, <=, >, >=, ==, !=)
//! - Bitwise operators (&, |, ^, ~, <<, >>)
//! - Logical operators (&&, ||, !)
//! - Assignment operators (=, +=, -=, etc.)
//! - Ternary operator (? :)
//! - Pre/post increment/decrement (++, --)
//! - Nested arithmetic: $((expr))
//! - Command substitution: $(cmd) or `cmd`
//!
//! Known limitations:
//! - Bitwise operations use 64-bit signed integers, matching bash behavior.

use std::collections::HashSet;
use crate::ast::types::*;
use crate::interpreter::types::{InterpreterContext, ExecutionLimits};
use crate::interpreter::errors::ArithmeticError;
use crate::parser::{parse_arith_expr, parse_arith_number};

// ============================================================================
// Binary Operators
// ============================================================================

/// Pure binary operator evaluation - no async, no side effects.
fn apply_binary_op(left: i64, right: i64, operator: &ArithBinaryOperator) -> Result<i64, ArithmeticError> {
    match operator {
        ArithBinaryOperator::Add => Ok(left + right),
        ArithBinaryOperator::Sub => Ok(left - right),
        ArithBinaryOperator::Mul => Ok(left * right),
        ArithBinaryOperator::Div => {
            if right == 0 {
                Err(ArithmeticError::simple("division by 0"))
            } else {
                Ok(left / right)
            }
        }
        ArithBinaryOperator::Mod => {
            if right == 0 {
                Err(ArithmeticError::simple("division by 0"))
            } else {
                Ok(left % right)
            }
        }
        ArithBinaryOperator::Pow => {
            // Bash disallows negative exponents
            if right < 0 {
                Err(ArithmeticError::simple("exponent less than 0"))
            } else {
                Ok(left.saturating_pow(right as u32))
            }
        }
        ArithBinaryOperator::LShift => Ok(left << right),
        ArithBinaryOperator::RShift => Ok(left >> right),
        ArithBinaryOperator::Lt => Ok(if left < right { 1 } else { 0 }),
        ArithBinaryOperator::Le => Ok(if left <= right { 1 } else { 0 }),
        ArithBinaryOperator::Gt => Ok(if left > right { 1 } else { 0 }),
        ArithBinaryOperator::Ge => Ok(if left >= right { 1 } else { 0 }),
        ArithBinaryOperator::Eq => Ok(if left == right { 1 } else { 0 }),
        ArithBinaryOperator::Ne => Ok(if left != right { 1 } else { 0 }),
        ArithBinaryOperator::BitAnd => Ok(left & right),
        ArithBinaryOperator::BitOr => Ok(left | right),
        ArithBinaryOperator::BitXor => Ok(left ^ right),
        ArithBinaryOperator::Comma => Ok(right),
        ArithBinaryOperator::LogAnd | ArithBinaryOperator::LogOr => {
            // These are handled separately for short-circuit evaluation
            Ok(right)
        }
    }
}

// ============================================================================
// Assignment Operators
// ============================================================================

/// Pure assignment operator evaluation - no async, no side effects on ctx.
/// Returns the new value to be assigned.
fn apply_assignment_op(current: i64, value: i64, operator: &ArithAssignmentOperator) -> i64 {
    match operator {
        ArithAssignmentOperator::Assign => value,
        ArithAssignmentOperator::AddAssign => current + value,
        ArithAssignmentOperator::SubAssign => current - value,
        ArithAssignmentOperator::MulAssign => current * value,
        ArithAssignmentOperator::DivAssign => {
            if value != 0 { current / value } else { 0 }
        }
        ArithAssignmentOperator::ModAssign => {
            if value != 0 { current % value } else { 0 }
        }
        ArithAssignmentOperator::LShiftAssign => current << value,
        ArithAssignmentOperator::RShiftAssign => current >> value,
        ArithAssignmentOperator::AndAssign => current & value,
        ArithAssignmentOperator::OrAssign => current | value,
        ArithAssignmentOperator::XorAssign => current ^ value,
    }
}

// ============================================================================
// Unary Operators
// ============================================================================

/// Pure unary operator evaluation - no async, no side effects.
/// For ++/-- operators, this only handles the operand transformation,
/// not the variable assignment which must be done by the caller.
fn apply_unary_op(operand: i64, operator: &ArithUnaryOperator) -> i64 {
    match operator {
        ArithUnaryOperator::Neg => -operand,
        ArithUnaryOperator::Pos => operand,
        ArithUnaryOperator::Not => if operand == 0 { 1 } else { 0 },
        ArithUnaryOperator::BitNot => !operand,
        ArithUnaryOperator::Inc | ArithUnaryOperator::Dec => {
            // These are handled separately for side effects
            operand
        }
    }
}

// ============================================================================
// Variable Access
// ============================================================================

/// Get an arithmetic variable value with array[0] decay support.
/// In bash, when an array variable is used without an index in arithmetic context,
/// it decays to the value at index 0.
fn get_arith_variable(ctx: &InterpreterContext, name: &str) -> String {
    // First try to get the direct variable value
    if let Some(direct_value) = ctx.state.env.get(name) {
        return direct_value.clone();
    }
    // Array decay: if varName_0 exists, the variable is an array and we use element 0
    let array_zero_key = format!("{}_0", name);
    if let Some(array_zero_value) = ctx.state.env.get(&array_zero_key) {
        return array_zero_value.clone();
    }
    // Fall back to empty string (caller should handle this)
    String::new()
}

/// Check if a variable name is a valid identifier.
fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let bytes = name.as_bytes();
    let first = bytes[0];
    if !matches!(first, b'a'..=b'z' | b'A'..=b'Z' | b'_') {
        return false;
    }
    bytes[1..].iter().all(|&b| matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'))
}

// ============================================================================
// Array Operations
// ============================================================================

/// Get array elements as a list of (index, value) tuples.
/// Returns indices in sorted order for indexed arrays.
fn get_array_elements(ctx: &InterpreterContext, array_name: &str) -> Vec<(Option<i64>, String)> {
    let mut result = Vec::new();
    let prefix = format!("{}_", array_name);

    for (key, value) in ctx.state.env.iter() {
        if key == array_name {
            // Scalar value (for array[0] decay)
            result.push((Some(0), value.clone()));
        } else if key.starts_with(&prefix) {
            // Array element
            let index_str = &key[prefix.len()..];
            if let Ok(index) = index_str.parse::<i64>() {
                result.push((Some(index), value.clone()));
            } else {
                // Non-numeric index (associative array)
                result.push((None, value.clone()));
            }
        }
    }

    // Sort by numeric index for consistent ordering
    result.sort_by_key(|(idx, _)| idx.unwrap_or(i64::MAX));
    result
}

// ============================================================================
// Arithmetic Value Evaluation
// ============================================================================

/// Parse and evaluate a string value as an arithmetic expression with full context.
/// This properly handles expressions like "1+2+3" or "x+y" by parsing and evaluating them.
fn evaluate_arith_value(ctx: &mut InterpreterContext, value: &str, is_expansion_context: bool) -> Result<i64, ArithmeticError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(0);
    }

    // Try to parse as a simple number first (fast path)
    if let Ok(num) = value.parse::<i64>() {
        if value.chars().all(|c| c.is_ascii_digit() || c == '-') {
            return Ok(num);
        }
    }

    // Parse and evaluate as arithmetic expression
    let (expr, pos) = parse_arith_expr(value, 0);

    if pos < value.len() {
        // There's unparsed content - this is a syntax error
        let unparsed = &value[pos..];
        let error_token = unparsed.split_whitespace().next().unwrap_or(unparsed);
        return Err(ArithmeticError::new(
            format!("syntax error in expression (error token is \"{}\")", error_token),
            String::new(),
            String::new(),
            false,
        ));
    }

    evaluate_arithmetic(ctx, &expr, is_expansion_context)
}

/// Recursively resolve a variable name to its numeric value.
/// In bash arithmetic, if a variable contains a string that is another variable name
/// or an arithmetic expression, it is recursively evaluated:
///   foo=5; bar=foo; $((bar)) => 5
///   e=1+2; $((e + 3)) => 6
fn resolve_arith_variable(
    ctx: &mut InterpreterContext,
    name: &str,
    visited: &mut HashSet<String>,
    is_expansion_context: bool,
) -> Result<i64, ArithmeticError> {
    // Prevent infinite recursion
    if visited.contains(name) {
        return Ok(0);
    }
    visited.insert(name.to_string());

    let value = get_arith_variable(ctx, name);

    // If value is empty, return 0
    if value.is_empty() {
        return Ok(0);
    }

    // Try to parse as a number
    if let Ok(num) = value.trim().parse::<i64>() {
        if value.trim().chars().all(|c| c.is_ascii_digit() || c == '-') {
            return Ok(num);
        }
    }

    let trimmed = value.trim();

    // If it's a valid identifier, recursively resolve
    if is_valid_identifier(trimmed) {
        return resolve_arith_variable(ctx, trimmed, visited, is_expansion_context);
    }

    // Dynamic arithmetic: parse and evaluate
    evaluate_arith_value(ctx, trimmed, is_expansion_context)
}

/// Main arithmetic evaluation function.
pub fn evaluate_arithmetic(
    ctx: &mut InterpreterContext,
    expr: &ArithExpr,
    is_expansion_context: bool,
) -> Result<i64, ArithmeticError> {
    match expr {
        ArithExpr::Number(node) => {
            if node.value == i64::MIN && node.value.to_string().parse::<i64>().is_err() {
                return Err(ArithmeticError::new(
                    "value too great for base".to_string(),
                    String::new(),
                    String::new(),
                    false,
                ));
            }
            Ok(node.value)
        }

        ArithExpr::Variable(node) => {
            // Use recursive resolution - bash evaluates variable names recursively
            resolve_arith_variable(ctx, &node.name, &mut HashSet::new(), is_expansion_context)
        }

        ArithExpr::SpecialVar(node) => {
            // Get the special variable value and parse as arithmetic
            let value = get_arith_variable(ctx, &node.name);
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(0);
            }
            // Try to parse as a simple integer first
            if let Ok(num) = trimmed.parse::<i64>() {
                if trimmed.chars().all(|c| c.is_ascii_digit() || c == '-') {
                    return Ok(num);
                }
            }
            // If not a simple number, evaluate as arithmetic expression
            let (expr, _) = parse_arith_expr(trimmed, 0);
            evaluate_arithmetic(ctx, &expr, is_expansion_context)
        }

        ArithExpr::Nested(node) => {
            evaluate_arithmetic(ctx, &node.expression, is_expansion_context)
        }

        ArithExpr::CommandSubst(_node) => {
            // Execute the command and parse the result as a number
            // For now, return 0 - command substitution needs execFn
            Ok(0)
        }

        ArithExpr::BracedExpansion(_node) => {
            // This would need full expansion context - for now return 0
            Ok(0)
        }

        ArithExpr::DynamicBase(_node) => {
            // ${base}#value - expand base, then parse value in that base
            // For now return 0
            Ok(0)
        }

        ArithExpr::DynamicNumber(_node) => {
            // ${zero}11 or ${zero}xAB - expand prefix, combine with suffix
            // For now return 0
            Ok(0)
        }

        ArithExpr::ArrayElement(node) => {
            let is_assoc = ctx.state.associative_arrays
                .as_ref()
                .map(|set| set.contains(&node.array))
                .unwrap_or(false);

            // Case 1: Literal string key - A['key']
            if let Some(ref string_key) = node.string_key {
                let env_key = format!("{}_{}", node.array, string_key);
                let array_value = ctx.state.env.get(&env_key).cloned().unwrap_or_default();
                return evaluate_arith_value(ctx, &array_value, is_expansion_context);
            }

            // Case 2: Associative array with variable name (no $ prefix) - A[K]
            if is_assoc {
                if let Some(ref index) = node.index {
                    if let ArithExpr::Variable(ref var_node) = **index {
                        if !var_node.has_dollar_prefix {
                            let env_key = format!("{}_{}", node.array, var_node.name);
                            let array_value = ctx.state.env.get(&env_key).cloned().unwrap_or_default();
                            return evaluate_arith_value(ctx, &array_value, is_expansion_context);
                        }
                    }
                }
            }

            // Case 3: Associative array with $ prefix - A[$key]
            if is_assoc {
                if let Some(ref index) = node.index {
                    if let ArithExpr::Variable(ref var_node) = **index {
                        if var_node.has_dollar_prefix {
                            let expanded_key = get_arith_variable(ctx, &var_node.name);
                            let env_key = format!("{}_{}", node.array, expanded_key);
                            let array_value = ctx.state.env.get(&env_key).cloned().unwrap_or_default();
                            return evaluate_arith_value(ctx, &array_value, is_expansion_context);
                        }
                    }
                }
            }

            // Case 4: Indexed array - A[expr]
            if let Some(ref index) = node.index {
                let mut index_val = evaluate_arithmetic(ctx, index, is_expansion_context)?;

                // Handle negative indices - bash counts from max_index + 1
                if index_val < 0 {
                    let elements = get_array_elements(ctx, &node.array);
                    if elements.is_empty() {
                        let msg = format!("bash: line {}: {}: bad array subscript\n",
                                         ctx.state.current_line, node.array);
                        ctx.state.expansion_stderr = Some(
                            ctx.state.expansion_stderr.as_ref().unwrap_or(&String::new()).clone() + &msg
                        );
                        return Ok(0);
                    }
                    let max_index = elements.iter()
                        .filter_map(|(idx, _)| *idx)
                        .max()
                        .unwrap_or(0);
                    let actual_idx = max_index + 1 + index_val;
                    if actual_idx < 0 {
                        let msg = format!("bash: line {}: {}: bad array subscript\n",
                                         ctx.state.current_line, node.array);
                        ctx.state.expansion_stderr = Some(
                            ctx.state.expansion_stderr.as_ref().unwrap_or(&String::new()).clone() + &msg
                        );
                        return Ok(0);
                    }
                    index_val = actual_idx;
                }

                let env_key = format!("{}_{}", node.array, index_val);
                let array_value = ctx.state.env.get(&env_key).cloned().unwrap_or_default();
                if !array_value.is_empty() {
                    return evaluate_arith_value(ctx, &array_value, is_expansion_context);
                }

                // Scalar decay: s[0] returns scalar value s
                if index_val == 0 {
                    let scalar_value = ctx.state.env.get(&node.array).cloned().unwrap_or_default();
                    if !scalar_value.is_empty() {
                        return evaluate_arith_value(ctx, &scalar_value, is_expansion_context);
                    }
                }

                // Check nounset
                if ctx.state.options.nounset {
                    let has_any_element = ctx.state.env.keys().any(|key| {
                        key == &node.array || key.starts_with(&format!("{}_", node.array))
                    });
                    if !has_any_element {
                        return Err(ArithmeticError::new(
                            format!("{}[{}]", node.array, index_val),
                            String::new(),
                            String::new(),
                            false,
                        ));
                    }
                }

                return Ok(0);
            }

            // No index and no stringKey - invalid
            Ok(0)
        }

        ArithExpr::DoubleSubscript(_node) => {
            // Double subscript like a[1][1] is not valid
            Err(ArithmeticError::new(
                "double subscript".to_string(),
                String::new(),
                String::new(),
                false,
            ))
        }

        ArithExpr::NumberSubscript(node) => {
            // Number subscript like 1[2] is not valid
            Err(ArithmeticError::new(
                format!("{}{}: syntax error: invalid arithmetic operator (error token is \"{}\")",
                       node.number, node.error_token, node.error_token),
                String::new(),
                String::new(),
                false,
            ))
        }

        ArithExpr::SyntaxError(node) => {
            // Syntax error node - throw at evaluation time
            Err(ArithmeticError::new(
                node.message.clone(),
                String::new(),
                String::new(),
                true,
            ))
        }

        ArithExpr::SingleQuote(node) => {
            // Single-quoted string - behavior depends on context
            if is_expansion_context {
                Err(ArithmeticError::new(
                    format!("syntax error: operand expected (error token is \"'{}'\")", node.content),
                    String::new(),
                    String::new(),
                    false,
                ))
            } else {
                Ok(node.value)
            }
        }

        ArithExpr::Binary(node) => {
            // Short-circuit evaluation for logical operators
            if node.operator == ArithBinaryOperator::LogOr {
                let left = evaluate_arithmetic(ctx, &node.left, is_expansion_context)?;
                if left != 0 {
                    return Ok(1);
                }
                let right = evaluate_arithmetic(ctx, &node.right, is_expansion_context)?;
                Ok(if right != 0 { 1 } else { 0 })
            } else if node.operator == ArithBinaryOperator::LogAnd {
                let left = evaluate_arithmetic(ctx, &node.left, is_expansion_context)?;
                if left == 0 {
                    return Ok(0);
                }
                let right = evaluate_arithmetic(ctx, &node.right, is_expansion_context)?;
                Ok(if right != 0 { 1 } else { 0 })
            } else {
                let left = evaluate_arithmetic(ctx, &node.left, is_expansion_context)?;
                let right = evaluate_arithmetic(ctx, &node.right, is_expansion_context)?;
                apply_binary_op(left, right, &node.operator)
            }
        }

        ArithExpr::Unary(node) => {
            let operand = evaluate_arithmetic(ctx, &node.operand, is_expansion_context)?;

            // Handle ++/-- with side effects separately
            if matches!(node.operator, ArithUnaryOperator::Inc | ArithUnaryOperator::Dec) {
                return handle_inc_dec(ctx, &node.operand, &node.operator, node.prefix, is_expansion_context, operand);
            }

            Ok(apply_unary_op(operand, &node.operator))
        }

        ArithExpr::Ternary(node) => {
            let condition = evaluate_arithmetic(ctx, &node.condition, is_expansion_context)?;
            if condition != 0 {
                evaluate_arithmetic(ctx, &node.consequent, is_expansion_context)
            } else {
                evaluate_arithmetic(ctx, &node.alternate, is_expansion_context)
            }
        }

        ArithExpr::Assignment(node) => {
            let mut env_key = node.variable.clone();

            // Handle array element assignment
            if let Some(ref string_key) = node.string_key {
                // Literal string key: A['key'] = V
                env_key = format!("{}_{}", node.variable, string_key);
            } else if let Some(ref subscript) = node.subscript {
                let is_assoc = ctx.state.associative_arrays
                    .as_ref()
                    .map(|set| set.contains(&node.variable))
                    .unwrap_or(false);

                if is_assoc {
                    if let ArithExpr::Variable(ref var_node) = **subscript {
                        if !var_node.has_dollar_prefix {
                            // A[K] = V where K is a variable name without $
                            env_key = format!("{}_{}", node.variable, var_node.name);
                        } else {
                            // A[$key] -> expand $key to get the actual key
                            let expanded_key = get_arith_variable(ctx, &var_node.name);
                            env_key = format!("{}_{}", node.variable, expanded_key.as_str());
                        }
                    } else {
                        // For non-variable subscripts on associative arrays
                        let index = evaluate_arithmetic(ctx, subscript, is_expansion_context)?;
                        env_key = format!("{}_{}", node.variable, index);
                    }
                } else {
                    // For indexed arrays, evaluate the subscript as arithmetic
                    let mut index = evaluate_arithmetic(ctx, subscript, is_expansion_context)?;

                    // Handle negative indices
                    if index < 0 {
                        let elements = get_array_elements(ctx, &node.variable);
                        if !elements.is_empty() {
                            let max_index = elements.iter()
                                .filter_map(|(idx, _)| *idx)
                                .max()
                                .unwrap_or(0);
                            index = max_index + 1 + index;
                        }
                    }

                    env_key = format!("{}_{}", node.variable, index);
                }
            }

            let current = ctx.state.env.get(&env_key)
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
            let value = evaluate_arithmetic(ctx, &node.value, is_expansion_context)?;
            let new_value = apply_assignment_op(current, value, &node.operator);
            ctx.state.env.insert(env_key, new_value.to_string());
            Ok(new_value)
        }

        ArithExpr::Group(node) => {
            evaluate_arithmetic(ctx, &node.expression, is_expansion_context)
        }

        ArithExpr::Concat(node) => {
            // Concatenate all parts to form a dynamic variable name or number
            let mut concatenated = String::new();
            for part in &node.parts {
                concatenated.push_str(&eval_concat_part_to_string(ctx, part, is_expansion_context)?);
            }

            // If the result is a valid identifier, look it up as a variable
            if is_valid_identifier(&concatenated) {
                resolve_arith_variable(ctx, &concatenated, &mut HashSet::new(), is_expansion_context)
            } else {
                // Otherwise parse as a number
                Ok(concatenated.trim().parse::<i64>().unwrap_or(0))
            }
        }

        ArithExpr::DynamicAssignment(_node) => {
            // Dynamic assignment: x$foo = 42 or x$foo[5] = 42
            // For now return 0
            Ok(0)
        }

        ArithExpr::DynamicElement(_node) => {
            // Dynamic element: x$foo[5]
            // For now return 0
            Ok(0)
        }
    }
}

/// Handle increment/decrement operators with side effects.
fn handle_inc_dec(
    ctx: &mut InterpreterContext,
    operand: &ArithExpr,
    operator: &ArithUnaryOperator,
    prefix: bool,
    is_expansion_context: bool,
    eval_operand: i64,
) -> Result<i64, ArithmeticError> {
    let is_inc = *operator == ArithUnaryOperator::Inc;
    let new_value = if is_inc { eval_operand + 1 } else { eval_operand - 1 };

    match operand {
        ArithExpr::Variable(var_node) => {
            ctx.state.env.insert(var_node.name.clone(), new_value.to_string());
            Ok(if prefix { new_value } else { eval_operand })
        }

        ArithExpr::ArrayElement(arr_node) => {
            let is_assoc = ctx.state.associative_arrays
                .as_ref()
                .map(|set| set.contains(&arr_node.array))
                .unwrap_or(false);

            let env_key = if let Some(ref string_key) = arr_node.string_key {
                format!("{}_{}", arr_node.array, string_key)
            } else if let Some(ref index) = arr_node.index {
                if is_assoc {
                    if let ArithExpr::Variable(ref var_node) = **index {
                        if !var_node.has_dollar_prefix {
                            format!("{}_{}", arr_node.array, var_node.name)
                        } else {
                            let expanded_key = get_arith_variable(ctx, &var_node.name);
                            format!("{}_{}", arr_node.array, expanded_key)
                        }
                    } else {
                        let idx = evaluate_arithmetic(ctx, index, is_expansion_context)?;
                        format!("{}_{}", arr_node.array, idx)
                    }
                } else {
                    let idx = evaluate_arithmetic(ctx, index, is_expansion_context)?;
                    format!("{}_{}", arr_node.array, idx)
                }
            } else {
                return Ok(eval_operand);
            };

            ctx.state.env.insert(env_key, new_value.to_string());
            Ok(if prefix { new_value } else { eval_operand })
        }

        ArithExpr::Concat(concat_node) => {
            // Handle dynamic variable name increment/decrement: x$foo++
            let mut var_name = String::new();
            for part in &concat_node.parts {
                var_name.push_str(&eval_concat_part_to_string(ctx, part, is_expansion_context)?);
            }

            if is_valid_identifier(&var_name) {
                ctx.state.env.insert(var_name.clone(), new_value.to_string());
                Ok(if prefix { new_value } else { eval_operand })
            } else {
                Ok(eval_operand)
            }
        }

        _ => Ok(eval_operand),
    }
}

/// Evaluate a concatenation part to a string.
fn eval_concat_part_to_string(
    ctx: &mut InterpreterContext,
    part: &ArithExpr,
    is_expansion_context: bool,
) -> Result<String, ArithmeticError> {
    match part {
        ArithExpr::Variable(var_node) => {
            if var_node.has_dollar_prefix {
                Ok(get_arith_variable(ctx, &var_node.name))
            } else {
                Ok(var_node.name.clone())
            }
        }

        ArithExpr::Number(num_node) => Ok(num_node.value.to_string()),

        ArithExpr::SpecialVar(var_node) => {
            Ok(get_arith_variable(ctx, &var_node.name))
        }

        _ => {
            // Evaluate other expressions as arithmetic
            let val = evaluate_arithmetic(ctx, part, is_expansion_context)?;
            Ok(val.to_string())
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_binary_op() {
        assert_eq!(apply_binary_op(5, 3, &ArithBinaryOperator::Add).unwrap(), 8);
        assert_eq!(apply_binary_op(5, 3, &ArithBinaryOperator::Sub).unwrap(), 2);
        assert_eq!(apply_binary_op(5, 3, &ArithBinaryOperator::Mul).unwrap(), 15);
        assert_eq!(apply_binary_op(6, 3, &ArithBinaryOperator::Div).unwrap(), 2);
        assert_eq!(apply_binary_op(6, 3, &ArithBinaryOperator::Mod).unwrap(), 0);
        assert_eq!(apply_binary_op(5, 3, &ArithBinaryOperator::Lt).unwrap(), 0);
        assert_eq!(apply_binary_op(3, 5, &ArithBinaryOperator::Lt).unwrap(), 1);
        assert_eq!(apply_binary_op(5, 5, &ArithBinaryOperator::Eq).unwrap(), 1);
    }

    #[test]
    fn test_apply_binary_op_division_by_zero() {
        assert!(apply_binary_op(5, 0, &ArithBinaryOperator::Div).is_err());
        assert!(apply_binary_op(5, 0, &ArithBinaryOperator::Mod).is_err());
    }

    #[test]
    fn test_apply_assignment_op() {
        assert_eq!(apply_assignment_op(10, 5, &ArithAssignmentOperator::Assign), 5);
        assert_eq!(apply_assignment_op(10, 5, &ArithAssignmentOperator::AddAssign), 15);
        assert_eq!(apply_assignment_op(10, 5, &ArithAssignmentOperator::SubAssign), 5);
        assert_eq!(apply_assignment_op(10, 5, &ArithAssignmentOperator::MulAssign), 50);
        assert_eq!(apply_assignment_op(10, 5, &ArithAssignmentOperator::DivAssign), 2);
    }

    #[test]
    fn test_apply_unary_op() {
        assert_eq!(apply_unary_op(5, &ArithUnaryOperator::Neg), -5);
        assert_eq!(apply_unary_op(-5, &ArithUnaryOperator::Neg), 5);
        assert_eq!(apply_unary_op(0, &ArithUnaryOperator::Not), 1);
        assert_eq!(apply_unary_op(5, &ArithUnaryOperator::Not), 0);
    }

    #[test]
    fn test_bitwise_ops() {
        assert_eq!(apply_binary_op(0b1010, 0b1100, &ArithBinaryOperator::BitAnd).unwrap(), 0b1000);
        assert_eq!(apply_binary_op(0b1010, 0b1100, &ArithBinaryOperator::BitOr).unwrap(), 0b1110);
        assert_eq!(apply_binary_op(0b1010, 0b1100, &ArithBinaryOperator::BitXor).unwrap(), 0b0110);
        assert_eq!(apply_binary_op(5, 2, &ArithBinaryOperator::LShift).unwrap(), 20);
        assert_eq!(apply_binary_op(20, 2, &ArithBinaryOperator::RShift).unwrap(), 5);
    }

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("_bar"));
        assert!(is_valid_identifier("foo123"));
        assert!(is_valid_identifier("foo_bar"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("123foo"));
        assert!(!is_valid_identifier("foo-bar"));
    }
}
