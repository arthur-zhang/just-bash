/// AWK Statement Executor
///
/// Executes AWK statements including print, printf, control flow (if/while/for),
/// and special statements (break, continue, next, exit, return, delete).

use crate::commands::awk::builtins::format_printf;
use crate::commands::awk::coercion::{format_output, is_truthy, to_number};
use crate::commands::awk::context::AwkContext;
use crate::commands::awk::expressions::eval_expr;
use crate::commands::awk::types::{AwkExpr, AwkProgram, AwkStmt, RedirectInfo, RedirectType};
use crate::commands::awk::variables::{
    delete_array, delete_array_element, set_variable,
};

// ─── Block Execution ─────────────────────────────────────────────

/// Execute a block of statements.
///
/// Iterates through statements, stopping early if a control flow flag
/// (exit, next, break, continue, return) is set.
pub fn execute_block(ctx: &mut AwkContext, stmts: &[AwkStmt], program: &AwkProgram) {
    for stmt in stmts {
        execute_stmt(ctx, stmt, program);
        if should_break_execution(ctx) {
            break;
        }
    }
}

/// Check if execution should break out of current block.
///
/// Returns true if any control flow flag is set that should stop
/// normal statement execution.
pub fn should_break_execution(ctx: &AwkContext) -> bool {
    ctx.should_exit
        || ctx.should_next
        || ctx.should_next_file
        || ctx.loop_break
        || ctx.loop_continue
        || ctx.has_return
}

// ─── Statement Execution ─────────────────────────────────────────

/// Execute a single AWK statement.
pub fn execute_stmt(ctx: &mut AwkContext, stmt: &AwkStmt, program: &AwkProgram) {
    match stmt {
        AwkStmt::ExprStmt(expr) => {
            eval_expr(ctx, expr, program);
        }

        AwkStmt::Print { args, output } => {
            execute_print(ctx, args, output.as_ref(), program);
        }

        AwkStmt::Printf {
            format,
            args,
            output,
        } => {
            execute_printf(ctx, format, args, output.as_ref(), program);
        }

        AwkStmt::If {
            condition,
            consequent,
            alternate,
        } => {
            execute_if(ctx, condition, consequent, alternate.as_deref(), program);
        }

        AwkStmt::While { condition, body } => {
            execute_while(ctx, condition, body, program);
        }

        AwkStmt::DoWhile { body, condition } => {
            execute_do_while(ctx, body, condition, program);
        }

        AwkStmt::For {
            init,
            condition,
            update,
            body,
        } => {
            execute_for(
                ctx,
                init.as_deref(),
                condition.as_ref(),
                update.as_deref(),
                body,
                program,
            );
        }

        AwkStmt::ForIn {
            variable,
            array,
            body,
        } => {
            execute_for_in(ctx, variable, array, body, program);
        }

        AwkStmt::Block(stmts) => {
            execute_block(ctx, stmts, program);
        }

        AwkStmt::Break => {
            ctx.loop_break = true;
        }

        AwkStmt::Continue => {
            ctx.loop_continue = true;
        }

        AwkStmt::Next => {
            ctx.should_next = true;
        }

        AwkStmt::NextFile => {
            ctx.should_next_file = true;
        }

        AwkStmt::Exit(code_expr) => {
            if let Some(expr) = code_expr {
                ctx.exit_code = to_number(&eval_expr(ctx, expr, program)) as i32;
            }
            ctx.should_exit = true;
        }

        AwkStmt::Return(value_expr) => {
            ctx.return_value = value_expr.as_ref().map(|e| eval_expr(ctx, e, program));
            ctx.has_return = true;
        }

        AwkStmt::Delete { target } => {
            execute_delete(ctx, target, program);
        }
    }
}

// ─── Print Statement ─────────────────────────────────────────────

/// Execute a print statement.
///
/// If no args, prints $0. Otherwise evaluates all args, formats numbers
/// using OFMT (integers print directly), joins with OFS, and appends ORS.
/// Handles output redirection (>, >>, |).
fn execute_print(
    ctx: &mut AwkContext,
    args: &[AwkExpr],
    output: Option<&RedirectInfo>,
    program: &AwkProgram,
) {
    let text = if args.is_empty() {
        // No args: print $0 + ORS
        format!("{}{}", ctx.line, ctx.ors)
    } else {
        // Evaluate all args and format
        let values: Vec<String> = args
            .iter()
            .map(|arg| {
                let val = eval_expr(ctx, arg, program);
                format_print_value(&val, &ctx.ofmt)
            })
            .collect();
        format!("{}{}", values.join(&ctx.ofs), ctx.ors)
    };

    write_output(ctx, output, &text, program);
}

/// Format a value for print statement.
///
/// Numbers are formatted using OFMT, but integers print directly.
fn format_print_value(val: &str, ofmt: &str) -> String {
    // Check if the value looks like a number
    let trimmed = val.trim();
    if let Ok(n) = trimmed.parse::<f64>() {
        // It's a number - use OFMT formatting
        format_output(n, ofmt)
    } else {
        // Not a number - return as-is
        val.to_string()
    }
}

// ─── Printf Statement ────────────────────────────────────────────

/// Execute a printf statement.
///
/// Evaluates format string and args, calls format_printf, and writes
/// the result. Handles output redirection.
fn execute_printf(
    ctx: &mut AwkContext,
    format_expr: &AwkExpr,
    args: &[AwkExpr],
    output: Option<&RedirectInfo>,
    program: &AwkProgram,
) {
    let format_str = eval_expr(ctx, format_expr, program);
    let values: Vec<String> = args.iter().map(|a| eval_expr(ctx, a, program)).collect();
    let text = format_printf(&format_str, &values);

    write_output(ctx, output, &text, program);
}

// ─── Output Redirection ──────────────────────────────────────────

/// Write output text, handling optional redirection.
///
/// - No redirection: append to ctx.output
/// - `>` (Write): first write overwrites, subsequent appends (tracked in opened_files)
/// - `>>` (Append): always appends
/// - `|` (Pipe): not supported, just appends to output
fn write_output(
    ctx: &mut AwkContext,
    output: Option<&RedirectInfo>,
    text: &str,
    program: &AwkProgram,
) {
    match output {
        None => {
            ctx.output.push_str(text);
        }
        Some(redirect) => {
            let filename = eval_expr(ctx, &redirect.target, program);

            match redirect.redirect_type {
                RedirectType::Write => {
                    // First write to this file overwrites, subsequent appends
                    if !ctx.opened_files.contains(&filename) {
                        // First write - mark as opened and write
                        ctx.opened_files.insert(filename.clone());
                        // In a real implementation with fs_handle, we would overwrite here
                        // For now, just append to output
                        ctx.output.push_str(text);
                    } else {
                        // Subsequent write - append
                        ctx.output.push_str(text);
                    }
                }
                RedirectType::Append => {
                    // Always append
                    if !ctx.opened_files.contains(&filename) {
                        ctx.opened_files.insert(filename.clone());
                    }
                    ctx.output.push_str(text);
                }
                RedirectType::Pipe => {
                    // Pipe execution not supported - just append to output
                    ctx.output.push_str(text);
                }
            }
        }
    }
}

// ─── Control Flow Statements ─────────────────────────────────────

/// Execute an if statement.
fn execute_if(
    ctx: &mut AwkContext,
    condition: &AwkExpr,
    consequent: &AwkStmt,
    alternate: Option<&AwkStmt>,
    program: &AwkProgram,
) {
    let cond_val = eval_expr(ctx, condition, program);
    if is_truthy(&cond_val) {
        execute_stmt(ctx, consequent, program);
    } else if let Some(alt) = alternate {
        execute_stmt(ctx, alt, program);
    }
}

/// Execute a while loop.
///
/// Tracks iteration count against max_iterations to prevent infinite loops.
fn execute_while(
    ctx: &mut AwkContext,
    condition: &AwkExpr,
    body: &AwkStmt,
    program: &AwkProgram,
) {
    let mut iterations = 0;

    while is_truthy(&eval_expr(ctx, condition, program)) {
        iterations += 1;
        if iterations > ctx.max_iterations {
            // Exceeded iteration limit - break out
            break;
        }

        ctx.loop_continue = false;
        execute_stmt(ctx, body, program);

        if ctx.loop_break {
            ctx.loop_break = false;
            break;
        }
        if ctx.should_exit || ctx.should_next || ctx.should_next_file || ctx.has_return {
            break;
        }
    }
}

/// Execute a do-while loop.
///
/// Body executes at least once before condition is checked.
fn execute_do_while(
    ctx: &mut AwkContext,
    body: &AwkStmt,
    condition: &AwkExpr,
    program: &AwkProgram,
) {
    let mut iterations = 0;

    loop {
        iterations += 1;
        if iterations > ctx.max_iterations {
            break;
        }

        ctx.loop_continue = false;
        execute_stmt(ctx, body, program);

        if ctx.loop_break {
            ctx.loop_break = false;
            break;
        }
        if ctx.should_exit || ctx.should_next || ctx.should_next_file || ctx.has_return {
            break;
        }

        // Check condition after body execution
        if !is_truthy(&eval_expr(ctx, condition, program)) {
            break;
        }
    }
}

/// Execute a C-style for loop.
fn execute_for(
    ctx: &mut AwkContext,
    init: Option<&AwkStmt>,
    condition: Option<&AwkExpr>,
    update: Option<&AwkStmt>,
    body: &AwkStmt,
    program: &AwkProgram,
) {
    // Execute init statement
    if let Some(init_stmt) = init {
        execute_stmt(ctx, init_stmt, program);
    }

    let mut iterations = 0;

    loop {
        // Check condition (if present)
        if let Some(cond) = condition {
            if !is_truthy(&eval_expr(ctx, cond, program)) {
                break;
            }
        }

        iterations += 1;
        if iterations > ctx.max_iterations {
            break;
        }

        ctx.loop_continue = false;
        execute_stmt(ctx, body, program);

        if ctx.loop_break {
            ctx.loop_break = false;
            break;
        }
        if ctx.should_exit || ctx.should_next || ctx.should_next_file || ctx.has_return {
            break;
        }

        // Execute update statement
        if let Some(upd) = update {
            execute_stmt(ctx, upd, program);
        }
    }
}

/// Execute a for-in loop (iterate over array keys).
fn execute_for_in(
    ctx: &mut AwkContext,
    variable: &str,
    array: &str,
    body: &AwkStmt,
    program: &AwkProgram,
) {
    // Get array keys (need to collect to avoid borrow issues)
    let keys: Vec<String> = ctx
        .arrays
        .get(array)
        .map(|arr| arr.keys().cloned().collect())
        .unwrap_or_default();

    for key in keys {
        set_variable(ctx, variable, &key);

        ctx.loop_continue = false;
        execute_stmt(ctx, body, program);

        if ctx.loop_break {
            ctx.loop_break = false;
            break;
        }
        if ctx.should_exit || ctx.should_next || ctx.should_next_file || ctx.has_return {
            break;
        }
    }
}

// ─── Delete Statement ────────────────────────────────────────────

/// Execute a delete statement.
///
/// If target is ArrayAccess, deletes that specific element.
/// If target is Variable, deletes the entire array.
fn execute_delete(ctx: &mut AwkContext, target: &AwkExpr, program: &AwkProgram) {
    match target {
        AwkExpr::ArrayAccess { array, key } => {
            let key_str = eval_array_key(ctx, key, program);
            delete_array_element(ctx, array, &key_str);
        }
        AwkExpr::Variable(name) => {
            delete_array(ctx, name);
        }
        _ => {
            // Invalid delete target - ignore
        }
    }
}

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

// ─── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::awk::context::AwkContext;
    use crate::commands::awk::fields::set_current_line;
    use crate::commands::awk::types::{AssignOp, AwkProgram, BinaryOp};
    use crate::commands::awk::variables::{get_variable, has_array_element, set_array_element};

    fn empty_program() -> AwkProgram {
        AwkProgram {
            functions: vec![],
            rules: vec![],
        }
    }

    // ─── Expression Statement Tests ───────────────────────────────

    #[test]
    fn test_execute_expr_stmt() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::ExprStmt(AwkExpr::Assignment {
            operator: AssignOp::Assign,
            target: Box::new(AwkExpr::Variable("x".to_string())),
            value: Box::new(AwkExpr::NumberLiteral(42.0)),
        });
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(get_variable(&ctx, "x"), "42");
    }

    // ─── Print Statement Tests ────────────────────────────────────

    #[test]
    fn test_print_no_args_prints_line() {
        let mut ctx = AwkContext::new();
        set_current_line(&mut ctx, "hello world");
        let program = empty_program();
        let stmt = AwkStmt::Print {
            args: vec![],
            output: None,
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(ctx.output, "hello world\n");
    }

    #[test]
    fn test_print_with_args_joins_with_ofs() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::Print {
            args: vec![
                AwkExpr::StringLiteral("hello".to_string()),
                AwkExpr::StringLiteral("world".to_string()),
            ],
            output: None,
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(ctx.output, "hello world\n");
    }

    #[test]
    fn test_print_custom_ofs_ors() {
        let mut ctx = AwkContext::new();
        ctx.ofs = ",".to_string();
        ctx.ors = ";\n".to_string();
        let program = empty_program();
        let stmt = AwkStmt::Print {
            args: vec![
                AwkExpr::StringLiteral("a".to_string()),
                AwkExpr::StringLiteral("b".to_string()),
            ],
            output: None,
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(ctx.output, "a,b;\n");
    }

    // ─── Printf Statement Tests ───────────────────────────────────

    #[test]
    fn test_printf_format_string() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::Printf {
            format: AwkExpr::StringLiteral("Hello %s, you are %d years old".to_string()),
            args: vec![
                AwkExpr::StringLiteral("Alice".to_string()),
                AwkExpr::NumberLiteral(30.0),
            ],
            output: None,
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(ctx.output, "Hello Alice, you are 30 years old");
    }

    // ─── If Statement Tests ───────────────────────────────────────

    #[test]
    fn test_if_true_branch() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::If {
            condition: AwkExpr::NumberLiteral(1.0),
            consequent: Box::new(AwkStmt::ExprStmt(AwkExpr::Assignment {
                operator: AssignOp::Assign,
                target: Box::new(AwkExpr::Variable("x".to_string())),
                value: Box::new(AwkExpr::StringLiteral("yes".to_string())),
            })),
            alternate: Some(Box::new(AwkStmt::ExprStmt(AwkExpr::Assignment {
                operator: AssignOp::Assign,
                target: Box::new(AwkExpr::Variable("x".to_string())),
                value: Box::new(AwkExpr::StringLiteral("no".to_string())),
            }))),
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(get_variable(&ctx, "x"), "yes");
    }

    #[test]
    fn test_if_false_branch() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::If {
            condition: AwkExpr::NumberLiteral(0.0),
            consequent: Box::new(AwkStmt::ExprStmt(AwkExpr::Assignment {
                operator: AssignOp::Assign,
                target: Box::new(AwkExpr::Variable("x".to_string())),
                value: Box::new(AwkExpr::StringLiteral("yes".to_string())),
            })),
            alternate: Some(Box::new(AwkStmt::ExprStmt(AwkExpr::Assignment {
                operator: AssignOp::Assign,
                target: Box::new(AwkExpr::Variable("x".to_string())),
                value: Box::new(AwkExpr::StringLiteral("no".to_string())),
            }))),
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(get_variable(&ctx, "x"), "no");
    }

    // ─── While Loop Tests ─────────────────────────────────────────

    #[test]
    fn test_while_loop() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        // i = 0; while (i < 3) { i++ }
        set_variable(&mut ctx, "i", "0");
        let stmt = AwkStmt::While {
            condition: AwkExpr::BinaryOp {
                operator: BinaryOp::Lt,
                left: Box::new(AwkExpr::Variable("i".to_string())),
                right: Box::new(AwkExpr::NumberLiteral(3.0)),
            },
            body: Box::new(AwkStmt::ExprStmt(AwkExpr::PostIncrement(Box::new(
                AwkExpr::Variable("i".to_string()),
            )))),
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(get_variable(&ctx, "i"), "3");
    }

    #[test]
    fn test_while_loop_iteration_limit() {
        let mut ctx = AwkContext::new();
        ctx.max_iterations = 5;
        let program = empty_program();
        // Infinite loop: while (1) { i++ }
        set_variable(&mut ctx, "i", "0");
        let stmt = AwkStmt::While {
            condition: AwkExpr::NumberLiteral(1.0),
            body: Box::new(AwkStmt::ExprStmt(AwkExpr::PostIncrement(Box::new(
                AwkExpr::Variable("i".to_string()),
            )))),
        };
        execute_stmt(&mut ctx, &stmt, &program);
        // Should stop at max_iterations
        assert_eq!(get_variable(&ctx, "i"), "5");
    }

    // ─── Do-While Loop Tests ──────────────────────────────────────

    #[test]
    fn test_do_while_runs_at_least_once() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        // do { x = 1 } while (0)
        let stmt = AwkStmt::DoWhile {
            body: Box::new(AwkStmt::ExprStmt(AwkExpr::Assignment {
                operator: AssignOp::Assign,
                target: Box::new(AwkExpr::Variable("x".to_string())),
                value: Box::new(AwkExpr::NumberLiteral(1.0)),
            })),
            condition: AwkExpr::NumberLiteral(0.0),
        };
        execute_stmt(&mut ctx, &stmt, &program);
        // Body should have run once even though condition is false
        assert_eq!(get_variable(&ctx, "x"), "1");
    }

    // ─── For Loop Tests ───────────────────────────────────────────

    #[test]
    fn test_for_loop() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        // for (i = 0; i < 3; i++) { sum += i }
        let stmt = AwkStmt::For {
            init: Some(Box::new(AwkStmt::ExprStmt(AwkExpr::Assignment {
                operator: AssignOp::Assign,
                target: Box::new(AwkExpr::Variable("i".to_string())),
                value: Box::new(AwkExpr::NumberLiteral(0.0)),
            }))),
            condition: Some(AwkExpr::BinaryOp {
                operator: BinaryOp::Lt,
                left: Box::new(AwkExpr::Variable("i".to_string())),
                right: Box::new(AwkExpr::NumberLiteral(3.0)),
            }),
            update: Some(Box::new(AwkStmt::ExprStmt(AwkExpr::PostIncrement(Box::new(
                AwkExpr::Variable("i".to_string()),
            ))))),
            body: Box::new(AwkStmt::ExprStmt(AwkExpr::Assignment {
                operator: AssignOp::AddAssign,
                target: Box::new(AwkExpr::Variable("sum".to_string())),
                value: Box::new(AwkExpr::Variable("i".to_string())),
            })),
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(get_variable(&ctx, "sum"), "3"); // 0 + 1 + 2 = 3
    }

    // ─── For-In Loop Tests ────────────────────────────────────────

    #[test]
    fn test_for_in_loop() {
        let mut ctx = AwkContext::new();
        set_array_element(&mut ctx, "arr", "a", "1");
        set_array_element(&mut ctx, "arr", "b", "2");
        set_array_element(&mut ctx, "arr", "c", "3");
        let program = empty_program();
        // for (k in arr) { count++ }
        let stmt = AwkStmt::ForIn {
            variable: "k".to_string(),
            array: "arr".to_string(),
            body: Box::new(AwkStmt::ExprStmt(AwkExpr::PostIncrement(Box::new(
                AwkExpr::Variable("count".to_string()),
            )))),
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(get_variable(&ctx, "count"), "3");
    }

    // ─── Break Statement Tests ────────────────────────────────────

    #[test]
    fn test_break_exits_loop() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        // i = 0; while (1) { if (i >= 2) break; i++ }
        set_variable(&mut ctx, "i", "0");
        let stmt = AwkStmt::While {
            condition: AwkExpr::NumberLiteral(1.0),
            body: Box::new(AwkStmt::Block(vec![
                AwkStmt::If {
                    condition: AwkExpr::BinaryOp {
                        operator: BinaryOp::Ge,
                        left: Box::new(AwkExpr::Variable("i".to_string())),
                        right: Box::new(AwkExpr::NumberLiteral(2.0)),
                    },
                    consequent: Box::new(AwkStmt::Break),
                    alternate: None,
                },
                AwkStmt::ExprStmt(AwkExpr::PostIncrement(Box::new(AwkExpr::Variable(
                    "i".to_string(),
                )))),
            ])),
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(get_variable(&ctx, "i"), "2");
    }

    // ─── Continue Statement Tests ─────────────────────────────────

    #[test]
    fn test_continue_skips_iteration() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        // for (i = 0; i < 5; i++) { if (i == 2) continue; sum += i }
        let stmt = AwkStmt::For {
            init: Some(Box::new(AwkStmt::ExprStmt(AwkExpr::Assignment {
                operator: AssignOp::Assign,
                target: Box::new(AwkExpr::Variable("i".to_string())),
                value: Box::new(AwkExpr::NumberLiteral(0.0)),
            }))),
            condition: Some(AwkExpr::BinaryOp {
                operator: BinaryOp::Lt,
                left: Box::new(AwkExpr::Variable("i".to_string())),
                right: Box::new(AwkExpr::NumberLiteral(5.0)),
            }),
            update: Some(Box::new(AwkStmt::ExprStmt(AwkExpr::PostIncrement(Box::new(
                AwkExpr::Variable("i".to_string()),
            ))))),
            body: Box::new(AwkStmt::Block(vec![
                AwkStmt::If {
                    condition: AwkExpr::BinaryOp {
                        operator: BinaryOp::Eq,
                        left: Box::new(AwkExpr::Variable("i".to_string())),
                        right: Box::new(AwkExpr::NumberLiteral(2.0)),
                    },
                    consequent: Box::new(AwkStmt::Continue),
                    alternate: None,
                },
                AwkStmt::ExprStmt(AwkExpr::Assignment {
                    operator: AssignOp::AddAssign,
                    target: Box::new(AwkExpr::Variable("sum".to_string())),
                    value: Box::new(AwkExpr::Variable("i".to_string())),
                }),
            ])),
        };
        execute_stmt(&mut ctx, &stmt, &program);
        // sum = 0 + 1 + 3 + 4 = 8 (skipping 2)
        assert_eq!(get_variable(&ctx, "sum"), "8");
    }

    // ─── Next Statement Tests ─────────────────────────────────────

    #[test]
    fn test_next_sets_flag() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::Next;
        execute_stmt(&mut ctx, &stmt, &program);
        assert!(ctx.should_next);
    }

    // ─── NextFile Statement Tests ─────────────────────────────────

    #[test]
    fn test_nextfile_sets_flag() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::NextFile;
        execute_stmt(&mut ctx, &stmt, &program);
        assert!(ctx.should_next_file);
    }

    // ─── Exit Statement Tests ─────────────────────────────────────

    #[test]
    fn test_exit_sets_flag_and_code() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::Exit(Some(AwkExpr::NumberLiteral(42.0)));
        execute_stmt(&mut ctx, &stmt, &program);
        assert!(ctx.should_exit);
        assert_eq!(ctx.exit_code, 42);
    }

    #[test]
    fn test_exit_without_code() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::Exit(None);
        execute_stmt(&mut ctx, &stmt, &program);
        assert!(ctx.should_exit);
        assert_eq!(ctx.exit_code, 0);
    }

    // ─── Return Statement Tests ───────────────────────────────────

    #[test]
    fn test_return_sets_value_and_flag() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::Return(Some(AwkExpr::StringLiteral("result".to_string())));
        execute_stmt(&mut ctx, &stmt, &program);
        assert!(ctx.has_return);
        assert_eq!(ctx.return_value, Some("result".to_string()));
    }

    #[test]
    fn test_return_without_value() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::Return(None);
        execute_stmt(&mut ctx, &stmt, &program);
        assert!(ctx.has_return);
        assert_eq!(ctx.return_value, None);
    }

    // ─── Delete Statement Tests ───────────────────────────────────

    #[test]
    fn test_delete_array_element() {
        let mut ctx = AwkContext::new();
        set_array_element(&mut ctx, "arr", "key", "value");
        assert!(has_array_element(&ctx, "arr", "key"));
        let program = empty_program();
        let stmt = AwkStmt::Delete {
            target: AwkExpr::ArrayAccess {
                array: "arr".to_string(),
                key: Box::new(AwkExpr::StringLiteral("key".to_string())),
            },
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert!(!has_array_element(&ctx, "arr", "key"));
    }

    #[test]
    fn test_delete_entire_array() {
        let mut ctx = AwkContext::new();
        set_array_element(&mut ctx, "arr", "a", "1");
        set_array_element(&mut ctx, "arr", "b", "2");
        let program = empty_program();
        let stmt = AwkStmt::Delete {
            target: AwkExpr::Variable("arr".to_string()),
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert!(!has_array_element(&ctx, "arr", "a"));
        assert!(!has_array_element(&ctx, "arr", "b"));
    }

    // ─── Block Execution Tests ────────────────────────────────────

    #[test]
    fn test_execute_block_stops_on_break() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmts = vec![
            AwkStmt::ExprStmt(AwkExpr::Assignment {
                operator: AssignOp::Assign,
                target: Box::new(AwkExpr::Variable("x".to_string())),
                value: Box::new(AwkExpr::NumberLiteral(1.0)),
            }),
            AwkStmt::Break,
            AwkStmt::ExprStmt(AwkExpr::Assignment {
                operator: AssignOp::Assign,
                target: Box::new(AwkExpr::Variable("x".to_string())),
                value: Box::new(AwkExpr::NumberLiteral(2.0)),
            }),
        ];
        execute_block(&mut ctx, &stmts, &program);
        // x should be 1, not 2 (break stopped execution)
        assert_eq!(get_variable(&ctx, "x"), "1");
        assert!(ctx.loop_break);
    }

    #[test]
    fn test_should_break_execution() {
        let mut ctx = AwkContext::new();
        assert!(!should_break_execution(&ctx));

        ctx.should_exit = true;
        assert!(should_break_execution(&ctx));
        ctx.should_exit = false;

        ctx.should_next = true;
        assert!(should_break_execution(&ctx));
        ctx.should_next = false;

        ctx.should_next_file = true;
        assert!(should_break_execution(&ctx));
        ctx.should_next_file = false;

        ctx.loop_break = true;
        assert!(should_break_execution(&ctx));
        ctx.loop_break = false;

        ctx.loop_continue = true;
        assert!(should_break_execution(&ctx));
        ctx.loop_continue = false;

        ctx.has_return = true;
        assert!(should_break_execution(&ctx));
    }

    // ─── Print Number Formatting Tests ────────────────────────────

    #[test]
    fn test_print_integer_directly() {
        let mut ctx = AwkContext::new();
        let program = empty_program();
        let stmt = AwkStmt::Print {
            args: vec![AwkExpr::NumberLiteral(42.0)],
            output: None,
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(ctx.output, "42\n");
    }

    #[test]
    fn test_print_float_with_ofmt() {
        let mut ctx = AwkContext::new();
        ctx.ofmt = "%.2f".to_string();
        let program = empty_program();
        let stmt = AwkStmt::Print {
            args: vec![AwkExpr::NumberLiteral(3.14159)],
            output: None,
        };
        execute_stmt(&mut ctx, &stmt, &program);
        assert_eq!(ctx.output, "3.14\n");
    }
}
