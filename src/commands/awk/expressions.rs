/// AWK Expression Evaluator
///
/// Evaluates AWK expressions and returns their string values.
/// Handles all expression types including literals, operators,
/// function calls, assignments, and getline operations.

use regex_lite::Regex;

use crate::commands::awk::builtins::{
    call_builtin, builtin_gensub, builtin_gsub, builtin_split, builtin_sub, BuiltinResult,
};
use crate::commands::awk::coercion::{is_truthy, looks_like_number, to_number, to_string};
use crate::commands::awk::context::AwkContext;
use crate::commands::awk::fields::{get_field, set_current_line, set_field};
use crate::commands::awk::types::{AssignOp, AwkExpr, AwkFunctionDef, AwkProgram, AwkStmt, BinaryOp, UnaryOp};
use crate::commands::awk::variables::{
    get_array_element, get_variable, has_array_element, set_array_element, set_variable,
};

// ─── Main Expression Evaluator ───────────────────────────────────

/// Evaluate an AWK expression and return its string value.
pub fn eval_expr(ctx: &mut AwkContext, expr: &AwkExpr, program: &AwkProgram) -> String {
    match expr {
        AwkExpr::NumberLiteral(n) => to_string(*n),

        AwkExpr::StringLiteral(s) => s.clone(),

        AwkExpr::RegexLiteral(pattern) => {
            // Regex used as expression matches against $0
            match Regex::new(pattern) {
                Ok(re) => {
                    if re.is_match(&ctx.line) {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    }
                }
                Err(_) => "0".to_string(),
            }
        }

        AwkExpr::FieldRef(index_expr) => {
            let index_val = eval_expr(ctx, index_expr, program);
            let index = to_number(&index_val) as i64;
            get_field(ctx, index)
        }

        AwkExpr::Variable(name) => get_variable(ctx, name),

        AwkExpr::ArrayAccess { array, key } => {
            let key_str = eval_array_key(ctx, key, program);
            get_array_element(ctx, array, &key_str)
        }

        AwkExpr::BinaryOp {
            operator,
            left,
            right,
        } => eval_binary_op(ctx, operator, left, right, program),

        AwkExpr::UnaryOp { operator, operand } => eval_unary_op(ctx, operator, operand, program),

        AwkExpr::Concatenation { left, right } => {
            let left_val = eval_expr(ctx, left, program);
            let right_val = eval_expr(ctx, right, program);
            format!("{}{}", left_val, right_val)
        }

        AwkExpr::Ternary {
            condition,
            consequent,
            alternate,
        } => {
            let cond_val = eval_expr(ctx, condition, program);
            if is_truthy(&cond_val) {
                eval_expr(ctx, consequent, program)
            } else {
                eval_expr(ctx, alternate, program)
            }
        }

        AwkExpr::FunctionCall { name, args } => eval_function_call(ctx, name, args, program),

        AwkExpr::Assignment {
            operator,
            target,
            value,
        } => eval_assignment(ctx, operator, target, value, program),

        AwkExpr::PreIncrement(operand) => eval_pre_increment(ctx, operand, program),

        AwkExpr::PreDecrement(operand) => eval_pre_decrement(ctx, operand, program),

        AwkExpr::PostIncrement(operand) => eval_post_increment(ctx, operand, program),

        AwkExpr::PostDecrement(operand) => eval_post_decrement(ctx, operand, program),

        AwkExpr::InExpr { key, array } => {
            let key_str = eval_array_key(ctx, key, program);
            if has_array_element(ctx, array, &key_str) {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }

        AwkExpr::Getline {
            variable,
            file,
            command,
        } => eval_getline(ctx, variable.as_deref(), file.as_deref(), command.as_deref(), program),

        AwkExpr::Tuple(exprs) => eval_tuple(ctx, exprs, program),
    }
}

// ─── Array Key Evaluation ────────────────────────────────────────

/// Evaluate an expression as an array key, handling Tuple for multi-dimensional arrays.
fn eval_array_key(ctx: &mut AwkContext, key: &AwkExpr, program: &AwkProgram) -> String {
    if let AwkExpr::Tuple(exprs) = key {
        // Multi-dimensional key: join with SUBSEP
        let parts: Vec<String> = exprs.iter().map(|e| eval_expr(ctx, e, program)).collect();
        parts.join(&ctx.subsep)
    } else {
        eval_expr(ctx, key, program)
    }
}

// ─── Binary Operators ────────────────────────────────────────────

fn eval_binary_op(
    ctx: &mut AwkContext,
    op: &BinaryOp,
    left: &AwkExpr,
    right: &AwkExpr,
    program: &AwkProgram,
) -> String {
    // Short-circuit evaluation for logical operators
    match op {
        BinaryOp::Or => {
            let left_val = eval_expr(ctx, left, program);
            if is_truthy(&left_val) {
                return "1".to_string();
            }
            let right_val = eval_expr(ctx, right, program);
            if is_truthy(&right_val) {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }

        BinaryOp::And => {
            let left_val = eval_expr(ctx, left, program);
            if !is_truthy(&left_val) {
                return "0".to_string();
            }
            let right_val = eval_expr(ctx, right, program);
            if is_truthy(&right_val) {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }

        BinaryOp::MatchOp => {
            let left_val = eval_expr(ctx, left, program);
            let pattern = get_regex_pattern(ctx, right, program);
            match Regex::new(&pattern) {
                Ok(re) => {
                    if re.is_match(&left_val) {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    }
                }
                Err(_) => "0".to_string(),
            }
        }

        BinaryOp::NotMatchOp => {
            let left_val = eval_expr(ctx, left, program);
            let pattern = get_regex_pattern(ctx, right, program);
            match Regex::new(&pattern) {
                Ok(re) => {
                    if re.is_match(&left_val) {
                        "0".to_string()
                    } else {
                        "1".to_string()
                    }
                }
                Err(_) => "1".to_string(),
            }
        }

        // Comparison operators
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
            let left_val = eval_expr(ctx, left, program);
            let right_val = eval_expr(ctx, right, program);
            eval_comparison(&left_val, &right_val, op)
        }

        // Arithmetic operators
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
            let left_val = eval_expr(ctx, left, program);
            let right_val = eval_expr(ctx, right, program);
            let left_num = to_number(&left_val);
            let right_num = to_number(&right_val);
            let result = match op {
                BinaryOp::Add => left_num + right_num,
                BinaryOp::Sub => left_num - right_num,
                BinaryOp::Mul => left_num * right_num,
                BinaryOp::Div => {
                    if right_num != 0.0 {
                        left_num / right_num
                    } else {
                        f64::INFINITY
                    }
                }
                BinaryOp::Mod => {
                    if right_num != 0.0 {
                        left_num % right_num
                    } else {
                        f64::NAN
                    }
                }
                BinaryOp::Pow => left_num.powf(right_num),
                _ => unreachable!(),
            };
            to_string(result)
        }
    }
}

/// Get regex pattern from expression, handling RegexLiteral specially.
fn get_regex_pattern(ctx: &mut AwkContext, expr: &AwkExpr, program: &AwkProgram) -> String {
    if let AwkExpr::RegexLiteral(pattern) = expr {
        pattern.clone()
    } else {
        eval_expr(ctx, expr, program)
    }
}

/// Evaluate a comparison operation.
fn eval_comparison(left: &str, right: &str, op: &BinaryOp) -> String {
    let result = if looks_like_number(left) && looks_like_number(right) {
        // Numeric comparison
        let left_num = to_number(left);
        let right_num = to_number(right);
        match op {
            BinaryOp::Eq => left_num == right_num,
            BinaryOp::Ne => left_num != right_num,
            BinaryOp::Lt => left_num < right_num,
            BinaryOp::Gt => left_num > right_num,
            BinaryOp::Le => left_num <= right_num,
            BinaryOp::Ge => left_num >= right_num,
            _ => false,
        }
    } else {
        // String comparison
        match op {
            BinaryOp::Eq => left == right,
            BinaryOp::Ne => left != right,
            BinaryOp::Lt => left < right,
            BinaryOp::Gt => left > right,
            BinaryOp::Le => left <= right,
            BinaryOp::Ge => left >= right,
            _ => false,
        }
    };
    if result {
        "1".to_string()
    } else {
        "0".to_string()
    }
}

// ─── Unary Operators ─────────────────────────────────────────────

fn eval_unary_op(
    ctx: &mut AwkContext,
    op: &UnaryOp,
    operand: &AwkExpr,
    program: &AwkProgram,
) -> String {
    let val = eval_expr(ctx, operand, program);
    match op {
        UnaryOp::Not => {
            if is_truthy(&val) {
                "0".to_string()
            } else {
                "1".to_string()
            }
        }
        UnaryOp::Neg => {
            let num = to_number(&val);
            to_string(-num)
        }
        UnaryOp::Pos => {
            let num = to_number(&val);
            to_string(num)
        }
    }
}

// ─── Increment/Decrement ─────────────────────────────────────────

/// Apply increment/decrement to an operand.
/// Returns either the old or new value based on `return_new`.
fn apply_inc_dec(
    ctx: &mut AwkContext,
    operand: &AwkExpr,
    delta: f64,
    return_new: bool,
    program: &AwkProgram,
) -> String {
    match operand {
        AwkExpr::Variable(name) => {
            let old_val = to_number(&get_variable(ctx, name));
            let new_val = old_val + delta;
            set_variable(ctx, name, &to_string(new_val));
            if return_new {
                to_string(new_val)
            } else {
                to_string(old_val)
            }
        }
        AwkExpr::FieldRef(index_expr) => {
            let index = to_number(&eval_expr(ctx, index_expr, program)) as i64;
            let old_val = to_number(&get_field(ctx, index));
            let new_val = old_val + delta;
            set_field(ctx, index, &to_string(new_val));
            if return_new {
                to_string(new_val)
            } else {
                to_string(old_val)
            }
        }
        AwkExpr::ArrayAccess { array, key } => {
            let key_str = eval_array_key(ctx, key, program);
            let old_val = to_number(&get_array_element(ctx, array, &key_str));
            let new_val = old_val + delta;
            set_array_element(ctx, array, &key_str, &to_string(new_val));
            if return_new {
                to_string(new_val)
            } else {
                to_string(old_val)
            }
        }
        _ => {
            // For other expressions, just evaluate and return
            eval_expr(ctx, operand, program)
        }
    }
}

fn eval_pre_increment(ctx: &mut AwkContext, operand: &AwkExpr, program: &AwkProgram) -> String {
    apply_inc_dec(ctx, operand, 1.0, true, program)
}

fn eval_pre_decrement(ctx: &mut AwkContext, operand: &AwkExpr, program: &AwkProgram) -> String {
    apply_inc_dec(ctx, operand, -1.0, true, program)
}

fn eval_post_increment(ctx: &mut AwkContext, operand: &AwkExpr, program: &AwkProgram) -> String {
    apply_inc_dec(ctx, operand, 1.0, false, program)
}

fn eval_post_decrement(ctx: &mut AwkContext, operand: &AwkExpr, program: &AwkProgram) -> String {
    apply_inc_dec(ctx, operand, -1.0, false, program)
}

// ─── Assignment ──────────────────────────────────────────────────

fn eval_assignment(
    ctx: &mut AwkContext,
    op: &AssignOp,
    target: &AwkExpr,
    value: &AwkExpr,
    program: &AwkProgram,
) -> String {
    let value_str = eval_expr(ctx, value, program);

    let final_value = if *op == AssignOp::Assign {
        value_str
    } else {
        // Compound assignment - get current value
        let current = match target {
            AwkExpr::Variable(name) => get_variable(ctx, name),
            AwkExpr::FieldRef(index_expr) => {
                let index = to_number(&eval_expr(ctx, index_expr, program)) as i64;
                get_field(ctx, index)
            }
            AwkExpr::ArrayAccess { array, key } => {
                let key_str = eval_array_key(ctx, key, program);
                get_array_element(ctx, array, &key_str)
            }
            _ => String::new(),
        };

        let current_num = to_number(&current);
        let value_num = to_number(&value_str);

        let result = match op {
            AssignOp::AddAssign => current_num + value_num,
            AssignOp::SubAssign => current_num - value_num,
            AssignOp::MulAssign => current_num * value_num,
            AssignOp::DivAssign => {
                if value_num != 0.0 {
                    current_num / value_num
                } else {
                    f64::INFINITY
                }
            }
            AssignOp::ModAssign => {
                if value_num != 0.0 {
                    current_num % value_num
                } else {
                    f64::NAN
                }
            }
            AssignOp::PowAssign => current_num.powf(value_num),
            AssignOp::Assign => unreachable!(),
        };
        to_string(result)
    };

    // Assign to target
    match target {
        AwkExpr::Variable(name) => {
            set_variable(ctx, name, &final_value);
        }
        AwkExpr::FieldRef(index_expr) => {
            let index = to_number(&eval_expr(ctx, index_expr, program)) as i64;
            set_field(ctx, index, &final_value);
        }
        AwkExpr::ArrayAccess { array, key } => {
            let key_str = eval_array_key(ctx, key, program);
            set_array_element(ctx, array, &key_str, &final_value);
        }
        _ => {}
    }

    final_value
}

// ─── Function Calls ──────────────────────────────────────────────

fn eval_function_call(
    ctx: &mut AwkContext,
    name: &str,
    args: &[AwkExpr],
    program: &AwkProgram,
) -> String {
    // Handle special built-ins that need expression-level access
    match name {
        "split" => return eval_split(ctx, args, program),
        "sub" => return eval_sub(ctx, args, program),
        "gsub" => return eval_gsub(ctx, args, program),
        "gensub" => return eval_gensub(ctx, args, program),
        _ => {}
    }

    // Evaluate arguments for regular built-ins
    let evaluated_args: Vec<String> = args.iter().map(|a| eval_expr(ctx, a, program)).collect();

    // Check for built-in functions
    if let Some(result) = call_builtin(name, &evaluated_args, ctx) {
        return match result {
            BuiltinResult::Value(v) => v,
            BuiltinResult::ValueWithSideEffect {
                value,
                target_name,
                new_value,
            } => {
                set_variable(ctx, &target_name, &new_value);
                value
            }
            BuiltinResult::Error(_) => String::new(),
        };
    }

    // Check for user-defined function
    if let Some(func) = program.functions.iter().find(|f| f.name == name) {
        return call_user_function(ctx, func, args, program);
    }

    // Also check context.functions (for functions registered at runtime)
    if let Some(func) = ctx.functions.get(name).cloned() {
        return call_user_function(ctx, &func, args, program);
    }

    // Unknown function
    String::new()
}

/// Evaluate split(string, array, separator?)
fn eval_split(ctx: &mut AwkContext, args: &[AwkExpr], program: &AwkProgram) -> String {
    if args.len() < 2 {
        return "0".to_string();
    }

    let string = eval_expr(ctx, &args[0], program);
    let array_name = match &args[1] {
        AwkExpr::Variable(name) => name.clone(),
        AwkExpr::ArrayAccess { array, .. } => array.clone(),
        _ => return "0".to_string(),
    };

    let separator = if args.len() > 2 {
        Some(eval_expr(ctx, &args[2], program))
    } else {
        None
    };

    let (count, elements) = builtin_split(&string, separator.as_deref(), &ctx.fs);

    // Clear existing array and populate with new elements
    ctx.arrays.remove(&array_name);
    for (key, value) in elements {
        set_array_element(ctx, &array_name, &key, &value);
    }

    to_string(count as f64)
}

/// Evaluate sub(regex, replacement, target?)
fn eval_sub(ctx: &mut AwkContext, args: &[AwkExpr], program: &AwkProgram) -> String {
    if args.len() < 2 {
        return "0".to_string();
    }

    let pattern = get_regex_pattern(ctx, &args[0], program);
    let replacement = eval_expr(ctx, &args[1], program);

    // Determine target (default is $0)
    let (target_value, target_expr) = if args.len() > 2 {
        (eval_expr(ctx, &args[2], program), Some(&args[2]))
    } else {
        (ctx.line.clone(), None)
    };

    let (count, new_value) = builtin_sub(&pattern, &replacement, &target_value);

    // Update the target
    if let Some(expr) = target_expr {
        match expr {
            AwkExpr::Variable(name) => set_variable(ctx, name, &new_value),
            AwkExpr::FieldRef(index_expr) => {
                let index = to_number(&eval_expr(ctx, index_expr, program)) as i64;
                set_field(ctx, index, &new_value);
            }
            AwkExpr::ArrayAccess { array, key } => {
                let key_str = eval_array_key(ctx, key, program);
                set_array_element(ctx, array, &key_str, &new_value);
            }
            _ => {}
        }
    } else {
        // Default target is $0
        set_field(ctx, 0, &new_value);
    }

    count
}

/// Evaluate gsub(regex, replacement, target?)
fn eval_gsub(ctx: &mut AwkContext, args: &[AwkExpr], program: &AwkProgram) -> String {
    if args.len() < 2 {
        return "0".to_string();
    }

    let pattern = get_regex_pattern(ctx, &args[0], program);
    let replacement = eval_expr(ctx, &args[1], program);

    // Determine target (default is $0)
    let (target_value, target_expr) = if args.len() > 2 {
        (eval_expr(ctx, &args[2], program), Some(&args[2]))
    } else {
        (ctx.line.clone(), None)
    };

    let (count, new_value) = builtin_gsub(&pattern, &replacement, &target_value);

    // Update the target
    if let Some(expr) = target_expr {
        match expr {
            AwkExpr::Variable(name) => set_variable(ctx, name, &new_value),
            AwkExpr::FieldRef(index_expr) => {
                let index = to_number(&eval_expr(ctx, index_expr, program)) as i64;
                set_field(ctx, index, &new_value);
            }
            AwkExpr::ArrayAccess { array, key } => {
                let key_str = eval_array_key(ctx, key, program);
                set_array_element(ctx, array, &key_str, &new_value);
            }
            _ => {}
        }
    } else {
        // Default target is $0
        set_field(ctx, 0, &new_value);
    }

    count
}

/// Evaluate gensub(regex, replacement, how, target?)
fn eval_gensub(ctx: &mut AwkContext, args: &[AwkExpr], program: &AwkProgram) -> String {
    if args.len() < 3 {
        return String::new();
    }

    let pattern = get_regex_pattern(ctx, &args[0], program);
    let replacement = eval_expr(ctx, &args[1], program);
    let how = eval_expr(ctx, &args[2], program);

    let target = if args.len() > 3 {
        eval_expr(ctx, &args[3], program)
    } else {
        ctx.line.clone()
    };

    builtin_gensub(&pattern, &replacement, &how, &target)
}

// ─── User Function Calls ─────────────────────────────────────────

/// Call a user-defined function.
fn call_user_function(
    ctx: &mut AwkContext,
    func: &AwkFunctionDef,
    args: &[AwkExpr],
    program: &AwkProgram,
) -> String {
    // Check recursion depth limit
    ctx.current_recursion_depth += 1;
    if ctx.current_recursion_depth > ctx.max_recursion_depth {
        ctx.current_recursion_depth -= 1;
        return String::new(); // Recursion limit exceeded
    }

    // Save parameter variables (they become local in AWK)
    let mut saved_params: Vec<(String, Option<String>)> = Vec::new();
    for param in &func.params {
        saved_params.push((param.clone(), ctx.vars.get(param).cloned()));
    }

    // Track array aliases we create (to clean up later)
    let mut created_aliases: Vec<String> = Vec::new();

    // Set up parameters
    for (i, param) in func.params.iter().enumerate() {
        if i < args.len() {
            let arg = &args[i];
            // If argument is a simple variable, set up an array alias
            // This allows arrays to be passed by reference
            if let AwkExpr::Variable(var_name) = arg {
                ctx.array_aliases.insert(param.clone(), var_name.clone());
                created_aliases.push(param.clone());
            }
            let value = eval_expr(ctx, arg, program);
            ctx.vars.insert(param.clone(), value);
        } else {
            ctx.vars.insert(param.clone(), String::new());
        }
    }

    // Execute function body
    ctx.has_return = false;
    ctx.return_value = None;

    // Execute the function body statements
    execute_block(ctx, &func.body, program);

    let result = ctx.return_value.clone().unwrap_or_default();

    // Restore parameter variables
    for (param, saved_value) in saved_params {
        if let Some(value) = saved_value {
            ctx.vars.insert(param, value);
        } else {
            ctx.vars.remove(&param);
        }
    }

    // Clean up array aliases we created
    for alias in created_aliases {
        ctx.array_aliases.remove(&alias);
    }

    ctx.has_return = false;
    ctx.return_value = None;
    ctx.current_recursion_depth -= 1;

    result
}

// Note: execute_block is now implemented in statements.rs
// We use a local wrapper to avoid circular dependency issues
fn execute_block(ctx: &mut AwkContext, stmts: &[AwkStmt], program: &AwkProgram) {
    crate::commands::awk::statements::execute_block(ctx, stmts, program);
}

// ─── Getline ─────────────────────────────────────────────────────

/// Evaluate getline expression.
/// Returns 1 on success, 0 on EOF, -1 on error.
fn eval_getline(
    ctx: &mut AwkContext,
    variable: Option<&str>,
    file: Option<&AwkExpr>,
    command: Option<&AwkExpr>,
    program: &AwkProgram,
) -> String {
    // "cmd" | getline - read from command pipe (not supported in this implementation)
    if command.is_some() {
        return "-1".to_string(); // Command execution not supported
    }

    // getline < "file" - read from external file (stubbed for now)
    if let Some(file_expr) = file {
        let filename = eval_expr(ctx, file_expr, program);
        return eval_getline_from_file(ctx, variable, &filename);
    }

    // Plain getline - read from current input
    eval_getline_from_input(ctx, variable)
}

/// Read next line from current input (ctx.lines).
fn eval_getline_from_input(ctx: &mut AwkContext, variable: Option<&str>) -> String {
    let lines = match &ctx.lines {
        Some(l) => l,
        None => return "-1".to_string(),
    };

    let current_index = ctx.line_index.unwrap_or(0);
    let next_index = current_index + 1;

    if next_index >= lines.len() {
        return "0".to_string(); // EOF
    }

    let next_line = lines[next_index].clone();

    if let Some(var) = variable {
        set_variable(ctx, var, &next_line);
    } else {
        set_current_line(ctx, &next_line);
    }

    ctx.nr += 1;
    ctx.line_index = Some(next_index);

    "1".to_string()
}

/// Read next line from a file (stubbed implementation).
fn eval_getline_from_file(ctx: &mut AwkContext, variable: Option<&str>, filename: &str) -> String {
    // Special handling for /dev/null - always returns EOF
    if filename == "/dev/null" {
        return "0".to_string();
    }

    // Use internal cache for file contents
    let cache_key = format!("__fc_{}", filename);
    let index_key = format!("__fi_{}", filename);

    // Check if file is already cached
    let (lines, line_index): (Vec<String>, i64) = if let Some(cached) = ctx.vars.get(&cache_key) {
        let lines: Vec<String> = cached.split('\n').map(|s| s.to_string()).collect();
        let idx = ctx
            .vars
            .get(&index_key)
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(-1);
        (lines, idx)
    } else {
        // File not cached - in a real implementation, we would read the file here
        // For now, return error since we don't have filesystem access
        return "-1".to_string();
    };

    let next_index = (line_index + 1) as usize;
    if next_index >= lines.len() {
        return "0".to_string(); // EOF
    }

    let line = lines[next_index].clone();
    ctx.vars.insert(index_key, next_index.to_string());

    if let Some(var) = variable {
        set_variable(ctx, var, &line);
    } else {
        set_current_line(ctx, &line);
    }

    // Note: getline from file does NOT update NR

    "1".to_string()
}

// ─── Tuple ───────────────────────────────────────────────────────

/// Evaluate a tuple expression.
/// When used as an expression (comma operator), evaluates all and returns the last.
/// When used as an array key, this is handled by eval_array_key.
fn eval_tuple(ctx: &mut AwkContext, exprs: &[AwkExpr], program: &AwkProgram) -> String {
    if exprs.is_empty() {
        return String::new();
    }

    // Evaluate all expressions, return the last one
    let mut result = String::new();
    for expr in exprs {
        result = eval_expr(ctx, expr, program);
    }
    result
}

// ─── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::awk::fields::set_current_line;
    use crate::commands::awk::types::AwkProgram;

    fn empty_program() -> AwkProgram {
        AwkProgram {
            functions: vec![],
            rules: vec![],
        }
    }

    // ─── Literal Tests ───────────────────────────────────────────

    #[test]
    fn test_eval_number_literal() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::NumberLiteral(42.0);
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "42");
    }

    #[test]
    fn test_eval_number_literal_float() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::NumberLiteral(3.14);
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "3.14");
    }

    #[test]
    fn test_eval_string_literal() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::StringLiteral("hello".to_string());
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "hello");
    }

    #[test]
    fn test_eval_regex_literal_match() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        let program = empty_program();
        let expr = AwkExpr::RegexLiteral("world".to_string());
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    #[test]
    fn test_eval_regex_literal_no_match() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        let program = empty_program();
        let expr = AwkExpr::RegexLiteral("xyz".to_string());
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "0");
    }

    // ─── Field Reference Tests ───────────────────────────────────

    #[test]
    fn test_eval_field_ref_zero() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        let program = empty_program();
        let expr = AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(0.0)));
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "hello world");
    }

    #[test]
    fn test_eval_field_ref_one() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        let program = empty_program();
        let expr = AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(1.0)));
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "hello");
    }

    #[test]
    fn test_eval_field_ref_two() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        let program = empty_program();
        let expr = AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(2.0)));
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "world");
    }

    // ─── Variable Tests ──────────────────────────────────────────

    #[test]
    fn test_eval_variable() {
        let mut ctx = AwkContext::new();
        set_variable(&mut ctx, "x", "hello");
        let program = empty_program();
        let expr = AwkExpr::Variable("x".to_string());
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "hello");
    }

    #[test]
    fn test_eval_uninitialized_variable() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::Variable("undefined_var".to_string());
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "");
    }

    // ─── Array Access Tests ──────────────────────────────────────

    #[test]
    fn test_eval_array_access() {
        let mut ctx = AwkContext::new();
        set_array_element(&mut ctx, "arr", "key", "value");
        let program = empty_program();
        let expr = AwkExpr::ArrayAccess {
            array: "arr".to_string(),
            key: Box::new(AwkExpr::StringLiteral("key".to_string())),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "value");
    }

    #[test]
    fn test_eval_array_access_unset() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::ArrayAccess {
            array: "arr".to_string(),
            key: Box::new(AwkExpr::StringLiteral("missing".to_string())),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "");
    }

    // ─── Binary Arithmetic Tests ─────────────────────────────────

    #[test]
    fn test_eval_binary_add() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Add,
            left: Box::new(AwkExpr::NumberLiteral(3.0)),
            right: Box::new(AwkExpr::NumberLiteral(4.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "7");
    }

    #[test]
    fn test_eval_binary_sub() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Sub,
            left: Box::new(AwkExpr::NumberLiteral(10.0)),
            right: Box::new(AwkExpr::NumberLiteral(3.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "7");
    }

    #[test]
    fn test_eval_binary_mul() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Mul,
            left: Box::new(AwkExpr::NumberLiteral(3.0)),
            right: Box::new(AwkExpr::NumberLiteral(4.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "12");
    }

    #[test]
    fn test_eval_binary_div() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Div,
            left: Box::new(AwkExpr::NumberLiteral(10.0)),
            right: Box::new(AwkExpr::NumberLiteral(4.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "2.5");
    }

    #[test]
    fn test_eval_binary_mod() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Mod,
            left: Box::new(AwkExpr::NumberLiteral(10.0)),
            right: Box::new(AwkExpr::NumberLiteral(3.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    #[test]
    fn test_eval_binary_pow() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Pow,
            left: Box::new(AwkExpr::NumberLiteral(2.0)),
            right: Box::new(AwkExpr::NumberLiteral(3.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "8");
    }

    // ─── String Concatenation Tests ──────────────────────────────

    #[test]
    fn test_eval_concatenation() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::Concatenation {
            left: Box::new(AwkExpr::StringLiteral("hello".to_string())),
            right: Box::new(AwkExpr::StringLiteral("world".to_string())),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "helloworld");
    }

    // ─── Comparison Tests ────────────────────────────────────────

    #[test]
    fn test_eval_comparison_numeric_lt() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Lt,
            left: Box::new(AwkExpr::NumberLiteral(3.0)),
            right: Box::new(AwkExpr::NumberLiteral(4.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    #[test]
    fn test_eval_comparison_numeric_gt() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Gt,
            left: Box::new(AwkExpr::NumberLiteral(5.0)),
            right: Box::new(AwkExpr::NumberLiteral(3.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    #[test]
    fn test_eval_comparison_string_lt() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Lt,
            left: Box::new(AwkExpr::StringLiteral("abc".to_string())),
            right: Box::new(AwkExpr::StringLiteral("def".to_string())),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    #[test]
    fn test_eval_comparison_eq() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Eq,
            left: Box::new(AwkExpr::NumberLiteral(5.0)),
            right: Box::new(AwkExpr::NumberLiteral(5.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    #[test]
    fn test_eval_comparison_ne() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Ne,
            left: Box::new(AwkExpr::NumberLiteral(5.0)),
            right: Box::new(AwkExpr::NumberLiteral(3.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    // ─── Logical Operator Tests ──────────────────────────────────

    #[test]
    fn test_eval_logical_and_true() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::And,
            left: Box::new(AwkExpr::NumberLiteral(1.0)),
            right: Box::new(AwkExpr::NumberLiteral(1.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    #[test]
    fn test_eval_logical_and_false() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::And,
            left: Box::new(AwkExpr::NumberLiteral(1.0)),
            right: Box::new(AwkExpr::NumberLiteral(0.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "0");
    }

    #[test]
    fn test_eval_logical_and_short_circuit() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        // If left is false, right should not be evaluated
        // We test this by using a side effect (assignment)
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::And,
            left: Box::new(AwkExpr::NumberLiteral(0.0)),
            right: Box::new(AwkExpr::Assignment {
                operator: AssignOp::Assign,
                target: Box::new(AwkExpr::Variable("x".to_string())),
                value: Box::new(AwkExpr::NumberLiteral(1.0)),
            }),
        };
        eval_expr(&mut ctx, &expr, &program);
        // x should not be set because right side was not evaluated
        assert_eq!(get_variable(&ctx, "x"), "");
    }

    #[test]
    fn test_eval_logical_or_true() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Or,
            left: Box::new(AwkExpr::NumberLiteral(0.0)),
            right: Box::new(AwkExpr::NumberLiteral(1.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    #[test]
    fn test_eval_logical_or_false() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Or,
            left: Box::new(AwkExpr::NumberLiteral(0.0)),
            right: Box::new(AwkExpr::NumberLiteral(0.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "0");
    }

    #[test]
    fn test_eval_logical_or_short_circuit() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        // If left is true, right should not be evaluated
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::Or,
            left: Box::new(AwkExpr::NumberLiteral(1.0)),
            right: Box::new(AwkExpr::Assignment {
                operator: AssignOp::Assign,
                target: Box::new(AwkExpr::Variable("x".to_string())),
                value: Box::new(AwkExpr::NumberLiteral(1.0)),
            }),
        };
        eval_expr(&mut ctx, &expr, &program);
        // x should not be set because right side was not evaluated
        assert_eq!(get_variable(&ctx, "x"), "");
    }

    // ─── Regex Match Tests ───────────────────────────────────────

    #[test]
    fn test_eval_regex_match() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::MatchOp,
            left: Box::new(AwkExpr::StringLiteral("hello".to_string())),
            right: Box::new(AwkExpr::RegexLiteral("ell".to_string())),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    #[test]
    fn test_eval_regex_match_no_match() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::MatchOp,
            left: Box::new(AwkExpr::StringLiteral("hello".to_string())),
            right: Box::new(AwkExpr::RegexLiteral("xyz".to_string())),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "0");
    }

    #[test]
    fn test_eval_regex_not_match() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::BinaryOp {
            operator: BinaryOp::NotMatchOp,
            left: Box::new(AwkExpr::StringLiteral("hello".to_string())),
            right: Box::new(AwkExpr::RegexLiteral("xyz".to_string())),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    // ─── Unary Operator Tests ────────────────────────────────────

    #[test]
    fn test_eval_unary_not_true() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::UnaryOp {
            operator: UnaryOp::Not,
            operand: Box::new(AwkExpr::NumberLiteral(1.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "0");
    }

    #[test]
    fn test_eval_unary_not_false() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::UnaryOp {
            operator: UnaryOp::Not,
            operand: Box::new(AwkExpr::NumberLiteral(0.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    #[test]
    fn test_eval_unary_neg() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::UnaryOp {
            operator: UnaryOp::Neg,
            operand: Box::new(AwkExpr::NumberLiteral(5.0)),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "-5");
    }

    #[test]
    fn test_eval_unary_pos() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::UnaryOp {
            operator: UnaryOp::Pos,
            operand: Box::new(AwkExpr::StringLiteral("42".to_string())),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "42");
    }

    // ─── Ternary Tests ───────────────────────────────────────────

    #[test]
    fn test_eval_ternary_true() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::Ternary {
            condition: Box::new(AwkExpr::NumberLiteral(1.0)),
            consequent: Box::new(AwkExpr::StringLiteral("yes".to_string())),
            alternate: Box::new(AwkExpr::StringLiteral("no".to_string())),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "yes");
    }

    #[test]
    fn test_eval_ternary_false() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::Ternary {
            condition: Box::new(AwkExpr::NumberLiteral(0.0)),
            consequent: Box::new(AwkExpr::StringLiteral("yes".to_string())),
            alternate: Box::new(AwkExpr::StringLiteral("no".to_string())),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "no");
    }

    // ─── Assignment Tests ────────────────────────────────────────

    #[test]
    fn test_eval_assignment_simple() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::Assignment {
            operator: AssignOp::Assign,
            target: Box::new(AwkExpr::Variable("x".to_string())),
            value: Box::new(AwkExpr::NumberLiteral(42.0)),
        };
        let result = eval_expr(&mut ctx, &expr, &program);
        assert_eq!(result, "42");
        assert_eq!(get_variable(&ctx, "x"), "42");
    }

    #[test]
    fn test_eval_assignment_add() {
        let mut ctx = AwkContext::new();
        set_variable(&mut ctx, "x", "10");
        let program = empty_program();
        let expr = AwkExpr::Assignment {
            operator: AssignOp::AddAssign,
            target: Box::new(AwkExpr::Variable("x".to_string())),
            value: Box::new(AwkExpr::NumberLiteral(5.0)),
        };
        let result = eval_expr(&mut ctx, &expr, &program);
        assert_eq!(result, "15");
        assert_eq!(get_variable(&ctx, "x"), "15");
    }

    #[test]
    fn test_eval_assignment_sub() {
        let mut ctx = AwkContext::new();
        set_variable(&mut ctx, "x", "10");
        let program = empty_program();
        let expr = AwkExpr::Assignment {
            operator: AssignOp::SubAssign,
            target: Box::new(AwkExpr::Variable("x".to_string())),
            value: Box::new(AwkExpr::NumberLiteral(3.0)),
        };
        let result = eval_expr(&mut ctx, &expr, &program);
        assert_eq!(result, "7");
    }

    #[test]
    fn test_eval_assignment_to_field() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        let program = empty_program();
        let expr = AwkExpr::Assignment {
            operator: AssignOp::Assign,
            target: Box::new(AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(1.0)))),
            value: Box::new(AwkExpr::StringLiteral("goodbye".to_string())),
        };
        eval_expr(&mut ctx, &expr, &program);
        assert_eq!(get_field(&ctx, 1), "goodbye");
    }

    #[test]
    fn test_eval_assignment_to_array() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::Assignment {
            operator: AssignOp::Assign,
            target: Box::new(AwkExpr::ArrayAccess {
                array: "arr".to_string(),
                key: Box::new(AwkExpr::StringLiteral("key".to_string())),
            }),
            value: Box::new(AwkExpr::StringLiteral("value".to_string())),
        };
        eval_expr(&mut ctx, &expr, &program);
        assert_eq!(get_array_element(&ctx, "arr", "key"), "value");
    }

    // ─── Increment/Decrement Tests ───────────────────────────────

    #[test]
    fn test_eval_pre_increment() {
        let mut ctx = AwkContext::new();
        set_variable(&mut ctx, "x", "5");
        let program = empty_program();
        let expr = AwkExpr::PreIncrement(Box::new(AwkExpr::Variable("x".to_string())));
        let result = eval_expr(&mut ctx, &expr, &program);
        assert_eq!(result, "6");
        assert_eq!(get_variable(&ctx, "x"), "6");
    }

    #[test]
    fn test_eval_post_increment() {
        let mut ctx = AwkContext::new();
        set_variable(&mut ctx, "x", "5");
        let program = empty_program();
        let expr = AwkExpr::PostIncrement(Box::new(AwkExpr::Variable("x".to_string())));
        let result = eval_expr(&mut ctx, &expr, &program);
        assert_eq!(result, "5"); // Returns old value
        assert_eq!(get_variable(&ctx, "x"), "6"); // But variable is incremented
    }

    #[test]
    fn test_eval_pre_decrement() {
        let mut ctx = AwkContext::new();
        set_variable(&mut ctx, "x", "5");
        let program = empty_program();
        let expr = AwkExpr::PreDecrement(Box::new(AwkExpr::Variable("x".to_string())));
        let result = eval_expr(&mut ctx, &expr, &program);
        assert_eq!(result, "4");
        assert_eq!(get_variable(&ctx, "x"), "4");
    }

    #[test]
    fn test_eval_post_decrement() {
        let mut ctx = AwkContext::new();
        set_variable(&mut ctx, "x", "5");
        let program = empty_program();
        let expr = AwkExpr::PostDecrement(Box::new(AwkExpr::Variable("x".to_string())));
        let result = eval_expr(&mut ctx, &expr, &program);
        assert_eq!(result, "5"); // Returns old value
        assert_eq!(get_variable(&ctx, "x"), "4"); // But variable is decremented
    }

    // ─── In Expression Tests ─────────────────────────────────────

    #[test]
    fn test_eval_in_expr_true() {
        let mut ctx = AwkContext::new();
        set_array_element(&mut ctx, "arr", "key", "value");
        let program = empty_program();
        let expr = AwkExpr::InExpr {
            key: Box::new(AwkExpr::StringLiteral("key".to_string())),
            array: "arr".to_string(),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "1");
    }

    #[test]
    fn test_eval_in_expr_false() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::InExpr {
            key: Box::new(AwkExpr::StringLiteral("missing".to_string())),
            array: "arr".to_string(),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "0");
    }

    // ─── Function Call Tests ─────────────────────────────────────

    #[test]
    fn test_eval_function_call_length() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::FunctionCall {
            name: "length".to_string(),
            args: vec![AwkExpr::StringLiteral("hello".to_string())],
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "5");
    }

    #[test]
    fn test_eval_function_call_substr() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::FunctionCall {
            name: "substr".to_string(),
            args: vec![
                AwkExpr::StringLiteral("hello".to_string()),
                AwkExpr::NumberLiteral(2.0),
                AwkExpr::NumberLiteral(3.0),
            ],
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "ell");
    }

    #[test]
    fn test_eval_function_call_toupper() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::FunctionCall {
            name: "toupper".to_string(),
            args: vec![AwkExpr::StringLiteral("hello".to_string())],
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "HELLO");
    }

    // ─── Tuple Tests ─────────────────────────────────────────────

    #[test]
    fn test_eval_tuple_returns_last() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let expr = AwkExpr::Tuple(vec![
            AwkExpr::NumberLiteral(1.0),
            AwkExpr::NumberLiteral(2.0),
            AwkExpr::NumberLiteral(3.0),
        ]);
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "3");
    }

    #[test]
    fn test_eval_tuple_as_array_key() {
        let mut ctx = AwkContext::new();
        // Set SUBSEP to comma for easier testing
        ctx.subsep = ",".to_string();
        set_array_element(&mut ctx, "arr", "a,b", "value");
        let program = empty_program();
        let expr = AwkExpr::ArrayAccess {
            array: "arr".to_string(),
            key: Box::new(AwkExpr::Tuple(vec![
                AwkExpr::StringLiteral("a".to_string()),
                AwkExpr::StringLiteral("b".to_string()),
            ])),
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "value");
    }

    // ─── User Function Tests ─────────────────────────────────────

    #[test]
    fn test_eval_user_function_simple() {
        let mut ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![AwkFunctionDef {
                name: "double".to_string(),
                params: vec!["n".to_string()],
                body: vec![AwkStmt::Return(Some(AwkExpr::BinaryOp {
                    operator: BinaryOp::Mul,
                    left: Box::new(AwkExpr::Variable("n".to_string())),
                    right: Box::new(AwkExpr::NumberLiteral(2.0)),
                }))],
            }],
            rules: vec![],
        };
        let expr = AwkExpr::FunctionCall {
            name: "double".to_string(),
            args: vec![AwkExpr::NumberLiteral(5.0)],
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "10");
    }

    #[test]
    fn test_eval_user_function_no_return() {
        let mut ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![AwkFunctionDef {
                name: "noop".to_string(),
                params: vec![],
                body: vec![],
            }],
            rules: vec![],
        };
        let expr = AwkExpr::FunctionCall {
            name: "noop".to_string(),
            args: vec![],
        };
        assert_eq!(eval_expr(&mut ctx, &expr, &program), "");
    }
}
