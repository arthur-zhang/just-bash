/// AWK Interpreter Orchestrator
///
/// Main interpreter struct that orchestrates AWK program execution,
/// handling BEGIN blocks, line processing, END blocks, and pattern matching.

use regex_lite::Regex;

use crate::commands::awk::coercion::is_truthy;
use crate::commands::awk::context::AwkContext;
use crate::commands::awk::expressions::eval_expr;
use crate::commands::awk::fields::set_current_line;
use crate::commands::awk::statements::execute_block;
use crate::commands::awk::types::{AwkPattern, AwkProgram};

/// The AWK interpreter orchestrator.
///
/// Manages program execution including BEGIN/END blocks, line processing,
/// pattern matching, and range pattern state tracking.
pub struct AwkInterpreter {
    pub ctx: AwkContext,
    program: AwkProgram,
    range_states: Vec<bool>, // One bool per rule for range pattern tracking
}

impl AwkInterpreter {
    /// Create a new interpreter with the given context and program.
    ///
    /// Initializes range states and registers user-defined functions.
    pub fn new(ctx: AwkContext, program: AwkProgram) -> Self {
        let mut interpreter = AwkInterpreter {
            range_states: vec![false; program.rules.len()],
            ctx,
            program,
        };

        // Register user-defined functions in context
        for func in &interpreter.program.functions {
            interpreter
                .ctx
                .functions
                .insert(func.name.clone(), func.clone());
        }

        interpreter
    }

    /// Execute all BEGIN rules.
    ///
    /// BEGIN rules run before any input is processed.
    pub fn execute_begin(&mut self) {
        for rule in &self.program.rules {
            if self.ctx.should_exit {
                break;
            }

            if matches!(rule.pattern, Some(AwkPattern::Begin)) {
                execute_block(&mut self.ctx, &rule.action, &self.program);
                // Reset should_next after BEGIN (BEGIN doesn't use next)
                self.ctx.should_next = false;
            }
        }
    }

    /// Process a single input line.
    ///
    /// Updates context with the new line, increments NR/FNR,
    /// and executes matching rules.
    pub fn execute_line(&mut self, line: &str) {
        if self.ctx.should_exit {
            return;
        }

        // Update context with new line
        set_current_line(&mut self.ctx, line);
        self.ctx.nr += 1;
        self.ctx.fnr += 1;

        // Process each rule by index to avoid borrow issues
        let num_rules = self.program.rules.len();
        for rule_index in 0..num_rules {
            if self.ctx.should_exit {
                break;
            }

            if self.ctx.should_next {
                self.ctx.should_next = false;
                break;
            }

            if self.ctx.should_next_file {
                break;
            }

            // Skip BEGIN/END rules
            let is_begin_or_end = matches!(
                self.program.rules[rule_index].pattern,
                Some(AwkPattern::Begin) | Some(AwkPattern::End)
            );
            if is_begin_or_end {
                continue;
            }

            if self.matches_rule_by_index(rule_index) {
                self.execute_rule_action_by_index(rule_index);
            }
        }

        // Reset should_next at end of line processing
        self.ctx.should_next = false;
    }

    /// Execute all END rules.
    ///
    /// END rules run after all input has been processed.
    /// If exit is called during an END block, further END blocks are skipped.
    pub fn execute_end(&mut self) {
        // If already in END block (recursive call), return
        if self.ctx.in_end_block {
            return;
        }

        self.ctx.in_end_block = true;
        // Reset should_exit so END blocks can execute, but preserve exit_code
        self.ctx.should_exit = false;

        for rule in &self.program.rules {
            if matches!(rule.pattern, Some(AwkPattern::End)) {
                execute_block(&mut self.ctx, &rule.action, &self.program);
                // exit from END block stops further END blocks
                if self.ctx.should_exit {
                    break;
                }
            }
        }

        self.ctx.in_end_block = false;
    }

    /// Get accumulated output.
    pub fn get_output(&self) -> &str {
        &self.ctx.output
    }

    /// Get exit code.
    pub fn get_exit_code(&self) -> i32 {
        self.ctx.exit_code
    }

    /// Get context reference.
    pub fn get_context(&self) -> &AwkContext {
        &self.ctx
    }

    /// Get mutable context reference.
    pub fn get_context_mut(&mut self) -> &mut AwkContext {
        &mut self.ctx
    }

    /// Check if a rule matches the current line (by index).
    fn matches_rule_by_index(&mut self, rule_index: usize) -> bool {
        // Clone the pattern to avoid borrow issues
        let pattern = self.program.rules[rule_index].pattern.clone();

        match pattern {
            None => true, // No pattern = matches all lines

            Some(AwkPattern::Begin) | Some(AwkPattern::End) => false,

            Some(AwkPattern::Regex(ref pat)) => {
                match Regex::new(pat) {
                    Ok(re) => re.is_match(&self.ctx.line),
                    Err(_) => false,
                }
            }

            Some(AwkPattern::Expression(ref expr)) => {
                let result = eval_expr(&mut self.ctx, expr, &self.program);
                is_truthy(&result)
            }

            Some(AwkPattern::Range { ref start, ref end }) => {
                self.match_range_pattern_cloned(start, end, rule_index)
            }
        }
    }

    /// Match a range pattern and update state (using cloned patterns).
    fn match_range_pattern_cloned(
        &mut self,
        start: &AwkPattern,
        end: &AwkPattern,
        rule_index: usize,
    ) -> bool {
        let start_matches = self.match_pattern(start);
        let end_matches = self.match_pattern(end);

        if !self.range_states[rule_index] {
            // Not currently in range
            if start_matches {
                self.range_states[rule_index] = true;
                // Check if end also matches same line (single-line range)
                if end_matches {
                    self.range_states[rule_index] = false;
                }
                return true;
            }
            false
        } else {
            // Currently in range - always matches
            if end_matches {
                self.range_states[rule_index] = false;
            }
            true
        }
    }

    /// Match a single pattern (used for range start/end).
    fn match_pattern(&mut self, pattern: &AwkPattern) -> bool {
        match pattern {
            AwkPattern::Regex(pat) => {
                match Regex::new(pat) {
                    Ok(re) => re.is_match(&self.ctx.line),
                    Err(_) => false,
                }
            }
            AwkPattern::Expression(expr) => {
                let result = eval_expr(&mut self.ctx, expr, &self.program);
                is_truthy(&result)
            }
            _ => false,
        }
    }

    /// Execute a rule's action (by index).
    ///
    /// If the rule has no action (empty action vec), the default is to print $0.
    fn execute_rule_action_by_index(&mut self, rule_index: usize) {
        let action_is_empty = self.program.rules[rule_index].action.is_empty();
        if action_is_empty {
            // Default action: print $0
            let line = self.ctx.line.clone();
            let ors = self.ctx.ors.clone();
            self.ctx.output.push_str(&line);
            self.ctx.output.push_str(&ors);
        } else {
            // Clone the action to avoid borrow issues
            let action = self.program.rules[rule_index].action.clone();
            execute_block(&mut self.ctx, &action, &self.program);
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::awk::context::AwkContext;
    use crate::commands::awk::types::{
        AssignOp, AwkExpr, AwkFunctionDef, AwkPattern, AwkProgram, AwkRule, AwkStmt, BinaryOp,
    };

    fn empty_program() -> AwkProgram {
        AwkProgram {
            functions: vec![],
            rules: vec![],
        }
    }

    // ─── BEGIN Block Tests ────────────────────────────────────────

    #[test]
    fn test_execute_begin_block() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![AwkRule {
                pattern: Some(AwkPattern::Begin),
                action: vec![AwkStmt::Print {
                    args: vec![AwkExpr::StringLiteral("BEGIN".to_string())],
                    output: None,
                }],
            }],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_begin();

        assert_eq!(interp.get_output(), "BEGIN\n");
    }

    #[test]
    fn test_begin_runs_before_input() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![
                AwkRule {
                    pattern: Some(AwkPattern::Begin),
                    action: vec![AwkStmt::ExprStmt(AwkExpr::Assignment {
                        operator: AssignOp::Assign,
                        target: Box::new(AwkExpr::Variable("x".to_string())),
                        value: Box::new(AwkExpr::NumberLiteral(1.0)),
                    })],
                },
                AwkRule {
                    pattern: None,
                    action: vec![AwkStmt::Print {
                        args: vec![AwkExpr::Variable("x".to_string())],
                        output: None,
                    }],
                },
            ],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_begin();
        interp.execute_line("test");

        assert_eq!(interp.get_output(), "1\n");
    }

    // ─── END Block Tests ──────────────────────────────────────────

    #[test]
    fn test_execute_end_block() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![AwkRule {
                pattern: Some(AwkPattern::End),
                action: vec![AwkStmt::Print {
                    args: vec![AwkExpr::StringLiteral("END".to_string())],
                    output: None,
                }],
            }],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_end();

        assert_eq!(interp.get_output(), "END\n");
    }

    #[test]
    fn test_end_runs_after_input() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![
                AwkRule {
                    pattern: None,
                    action: vec![AwkStmt::ExprStmt(AwkExpr::PostIncrement(Box::new(
                        AwkExpr::Variable("count".to_string()),
                    )))],
                },
                AwkRule {
                    pattern: Some(AwkPattern::End),
                    action: vec![AwkStmt::Print {
                        args: vec![AwkExpr::Variable("count".to_string())],
                        output: None,
                    }],
                },
            ],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_line("line1");
        interp.execute_line("line2");
        interp.execute_line("line3");
        interp.execute_end();

        assert_eq!(interp.get_output(), "3\n");
    }

    // ─── Main Rule Execution Tests ────────────────────────────────

    #[test]
    fn test_execute_main_rules_for_each_line() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![AwkRule {
                pattern: None,
                action: vec![AwkStmt::Print {
                    args: vec![AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(0.0)))],
                    output: None,
                }],
            }],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_line("hello");
        interp.execute_line("world");

        assert_eq!(interp.get_output(), "hello\nworld\n");
    }

    // ─── Pattern Matching Tests ───────────────────────────────────

    #[test]
    fn test_regex_pattern_matching() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![AwkRule {
                pattern: Some(AwkPattern::Regex("hello".to_string())),
                action: vec![AwkStmt::Print {
                    args: vec![AwkExpr::StringLiteral("matched".to_string())],
                    output: None,
                }],
            }],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_line("hello world");
        interp.execute_line("goodbye world");
        interp.execute_line("say hello");

        assert_eq!(interp.get_output(), "matched\nmatched\n");
    }

    #[test]
    fn test_expression_pattern_matching() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![AwkRule {
                pattern: Some(AwkPattern::Expression(AwkExpr::BinaryOp {
                    operator: BinaryOp::Gt,
                    left: Box::new(AwkExpr::Variable("NR".to_string())),
                    right: Box::new(AwkExpr::NumberLiteral(1.0)),
                })),
                action: vec![AwkStmt::Print {
                    args: vec![AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(0.0)))],
                    output: None,
                }],
            }],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_line("line1");
        interp.execute_line("line2");
        interp.execute_line("line3");

        // Only lines 2 and 3 should match (NR > 1)
        assert_eq!(interp.get_output(), "line2\nline3\n");
    }

    // ─── Range Pattern Tests ──────────────────────────────────────

    #[test]
    fn test_range_pattern_start_end() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![AwkRule {
                pattern: Some(AwkPattern::Range {
                    start: Box::new(AwkPattern::Regex("START".to_string())),
                    end: Box::new(AwkPattern::Regex("END".to_string())),
                }),
                action: vec![AwkStmt::Print {
                    args: vec![AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(0.0)))],
                    output: None,
                }],
            }],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_line("before");
        interp.execute_line("START");
        interp.execute_line("middle");
        interp.execute_line("END");
        interp.execute_line("after");

        assert_eq!(interp.get_output(), "START\nmiddle\nEND\n");
    }

    #[test]
    fn test_range_pattern_single_line() {
        // When start and end match the same line
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![AwkRule {
                pattern: Some(AwkPattern::Range {
                    start: Box::new(AwkPattern::Regex("BOTH".to_string())),
                    end: Box::new(AwkPattern::Regex("BOTH".to_string())),
                }),
                action: vec![AwkStmt::Print {
                    args: vec![AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(0.0)))],
                    output: None,
                }],
            }],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_line("before");
        interp.execute_line("BOTH");
        interp.execute_line("after");
        interp.execute_line("BOTH");
        interp.execute_line("final");

        // Each BOTH line should match but immediately deactivate
        assert_eq!(interp.get_output(), "BOTH\nBOTH\n");
    }

    // ─── Default Action Tests ─────────────────────────────────────

    #[test]
    fn test_default_action_prints_line() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![AwkRule {
                pattern: Some(AwkPattern::Regex("^match".to_string())),
                action: vec![], // Empty action = default print $0
            }],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_line("match this");
        interp.execute_line("no match here");
        interp.execute_line("match again");

        assert_eq!(interp.get_output(), "match this\nmatch again\n");
    }

    // ─── Exit Tests ───────────────────────────────────────────────

    #[test]
    fn test_exit_in_begin_still_runs_end() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![
                AwkRule {
                    pattern: Some(AwkPattern::Begin),
                    action: vec![
                        AwkStmt::Print {
                            args: vec![AwkExpr::StringLiteral("BEGIN".to_string())],
                            output: None,
                        },
                        AwkStmt::Exit(Some(AwkExpr::NumberLiteral(42.0))),
                    ],
                },
                AwkRule {
                    pattern: Some(AwkPattern::End),
                    action: vec![AwkStmt::Print {
                        args: vec![AwkExpr::StringLiteral("END".to_string())],
                        output: None,
                    }],
                },
            ],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_begin();
        interp.execute_end();

        assert_eq!(interp.get_output(), "BEGIN\nEND\n");
        assert_eq!(interp.get_exit_code(), 42);
    }

    #[test]
    fn test_exit_in_end_stops_further_end_blocks() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![
                AwkRule {
                    pattern: Some(AwkPattern::End),
                    action: vec![
                        AwkStmt::Print {
                            args: vec![AwkExpr::StringLiteral("END1".to_string())],
                            output: None,
                        },
                        AwkStmt::Exit(None),
                    ],
                },
                AwkRule {
                    pattern: Some(AwkPattern::End),
                    action: vec![AwkStmt::Print {
                        args: vec![AwkExpr::StringLiteral("END2".to_string())],
                        output: None,
                    }],
                },
            ],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_end();

        // Only END1 should print, END2 should be skipped
        assert_eq!(interp.get_output(), "END1\n");
    }

    // ─── Next Tests ───────────────────────────────────────────────

    #[test]
    fn test_next_skips_to_next_line() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![
                AwkRule {
                    pattern: Some(AwkPattern::Regex("skip".to_string())),
                    action: vec![AwkStmt::Next],
                },
                AwkRule {
                    pattern: None,
                    action: vec![AwkStmt::Print {
                        args: vec![AwkExpr::FieldRef(Box::new(AwkExpr::NumberLiteral(0.0)))],
                        output: None,
                    }],
                },
            ],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_line("print this");
        interp.execute_line("skip this");
        interp.execute_line("print this too");

        assert_eq!(interp.get_output(), "print this\nprint this too\n");
    }

    // ─── NextFile Tests ───────────────────────────────────────────

    #[test]
    fn test_nextfile_sets_flag() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![AwkRule {
                pattern: None,
                action: vec![AwkStmt::NextFile],
            }],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_line("test");

        assert!(interp.ctx.should_next_file);
    }

    // ─── NR Tracking Tests ────────────────────────────────────────

    #[test]
    fn test_nr_increments_globally() {
        let ctx = AwkContext::new();
        let program = empty_program();

        let mut interp = AwkInterpreter::new(ctx, program);
        assert_eq!(interp.ctx.nr, 0);

        interp.execute_line("line1");
        assert_eq!(interp.ctx.nr, 1);

        interp.execute_line("line2");
        assert_eq!(interp.ctx.nr, 2);

        interp.execute_line("line3");
        assert_eq!(interp.ctx.nr, 3);
    }

    // ─── FNR Tracking Tests ───────────────────────────────────────

    #[test]
    fn test_fnr_tracking() {
        let ctx = AwkContext::new();
        let program = empty_program();

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_line("line1");
        assert_eq!(interp.ctx.fnr, 1);

        interp.execute_line("line2");
        assert_eq!(interp.ctx.fnr, 2);

        // Reset FNR for new file (simulated)
        interp.ctx.fnr = 0;
        interp.execute_line("new file line1");
        assert_eq!(interp.ctx.fnr, 1);
    }

    // ─── No Pattern Tests ─────────────────────────────────────────

    #[test]
    fn test_no_pattern_matches_all_lines() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![],
            rules: vec![AwkRule {
                pattern: None, // No pattern
                action: vec![AwkStmt::ExprStmt(AwkExpr::PostIncrement(Box::new(
                    AwkExpr::Variable("count".to_string()),
                )))],
            }],
        };

        let mut interp = AwkInterpreter::new(ctx, program);
        interp.execute_line("any");
        interp.execute_line("line");
        interp.execute_line("matches");

        assert_eq!(
            interp.ctx.vars.get("count").map(|s| s.as_str()),
            Some("3")
        );
    }

    // ─── User Function Registration Tests ─────────────────────────

    #[test]
    fn test_user_functions_registered() {
        let ctx = AwkContext::new();
        let program = AwkProgram {
            functions: vec![AwkFunctionDef {
                name: "myfunc".to_string(),
                params: vec!["x".to_string()],
                body: vec![AwkStmt::Return(Some(AwkExpr::Variable("x".to_string())))],
            }],
            rules: vec![],
        };

        let interp = AwkInterpreter::new(ctx, program);
        assert!(interp.ctx.functions.contains_key("myfunc"));
    }
}
