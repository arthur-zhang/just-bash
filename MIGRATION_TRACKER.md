# Just-Bash TypeScript to Rust Migration Tracker

## Parser 模块 (11 个文件)

- [ ] `parser/arithmetic-parser.ts` → `parser/arithmetic_parser.rs` 
- [ ] `parser/arithmetic-primaries.ts` → `parser/arithmetic_primaries.rs` 
- [ ] `parser/command-parser.ts` → `parser/command_parser.rs` 
- [ ] `parser/compound-parser.ts` → `parser/compound_parser.rs` 
- [ ] `parser/conditional-parser.ts` → `parser/conditional_parser.rs` 
- [ ] `parser/expansion-parser.ts` → `parser/expansion_parser.rs` 
- [ ] `parser/lexer.ts` → `parser/lexer.rs` 
- [ ] `parser/parser-substitution.ts` → `parser/parser_substitution.rs` 
- [ ] `parser/parser.ts` → `parser/parser.rs` 
- [ ] `parser/types.ts` → `parser/types.rs` 
- [ ] `parser/word-parser.ts` → `parser/word_parser.rs` 

## Interpreter 模块

### 主目录 (18 个源文件 + 5 个测试文件)

#### 源文件
- [ ] `interpreter/alias-expansion.ts` → `interpreter/alias_expansion.rs` ✅ 已迁移 (100% 完成，11个测试) 
- [ ] `interpreter/arithmetic.ts` → `interpreter/arithmetic.rs` ✅ 已迁移 (98% 完成，CommandSubst 需要 execFn) 
- [ ] `interpreter/assignment-expansion.ts` → `interpreter/assignment_expansion.rs` ✅ 已迁移
- [ ] `interpreter/builtin-dispatch.ts` → `interpreter/builtin_dispatch.rs` ✅ 已迁移
- [ ] `interpreter/command-resolution.ts` → `interpreter/command_resolution.rs` ✅ 已迁移 (100% 完成，10个测试) 
- [ ] `interpreter/conditionals.ts` → `interpreter/conditionals.rs` ✅ 已迁移 (已补充 evaluate_test_args 函数) 
- [ ] `interpreter/control-flow.ts` → `interpreter/control_flow.rs` ✅ 已迁移 (已补充 execute_if/for/while/until/case 函数) 
- [ ] `interpreter/errors.ts` → `interpreter/errors.rs` ✅ 已迁移 (100% 完成，增加了 InterpreterError 统一枚举) 
- [ ] `interpreter/expansion.ts` → `interpreter/word_expansion.rs` ✅ 已迁移 (骨架实现，需运行时回调)
- [ ] `interpreter/functions.ts` → `interpreter/functions.rs` ✅ 已迁移 (60% 完成，setup/cleanup 已迁移，执行逻辑需在调用方实现) 
- [ ] `interpreter/index.ts` → `interpreter/mod.rs`
- [ ] `interpreter/interpreter.ts` → `interpreter/interpreter.rs` ✅ 已迁移 (trait 定义，需运行时实现)
- [ ] `interpreter/pipeline-execution.ts` → `interpreter/pipeline_execution.rs` ✅ 已迁移 (70% 完成，辅助结构已迁移，主循环需在调用方实现) 
- [ ] `interpreter/redirections.ts` → `interpreter/redirections.rs` ✅ 已迁移
- [ ] `interpreter/simple-command-assignments.ts` → `interpreter/simple_command_assignments.rs` ✅ 已迁移
- [ ] `interpreter/subshell-group.ts` → `interpreter/subshell_group.rs` ✅ 已迁移 (55% 完成，状态管理已迁移，执行逻辑需在调用方实现) 
- [ ] `interpreter/type-command.ts` → `interpreter/type_command.rs` ✅ 已迁移
- [ ] `interpreter/types.ts` → `interpreter/types.rs` ✅ 已迁移 (100% 完成，所有类型定义) 

#### 测试文件 (已内联到 Rust 模块)
- [ ] `interpreter/arithmetic.test.ts` → `arithmetic.rs` 内联测试 ✅
- [ ] `interpreter/assoc-array.test.ts` → 相关模块内联测试 ✅
- [ ] `interpreter/control-flow.test.ts` → `control_flow.rs` 内联测试 ✅
- [ ] `interpreter/redirections.binary.test.ts` → `redirections.rs` 内联测试 ✅

### builtins/ (28 个源文件 + 16 个测试文件) - **已完成** ✅

#### 源文件
- [ ] `builtins/break.ts` → `builtins/break_cmd.rs` ✅ 已迁移 (100% 完成，5个测试) 
- [ ] `builtins/cd.ts` → `builtins/cd_cmd.rs` ✅ 已迁移 (已补充 -P 选项的 symlink 解析)
- [ ] `builtins/compgen.ts` → `builtins/compgen_cmd.rs` ✅ 已迁移 (部分功能需要运行时)
- [ ] `builtins/complete.ts` → `builtins/complete_cmd.rs` ✅ 已迁移 (100% 完成，5个测试) 
- [ ] `builtins/compopt.ts` → `builtins/compopt_cmd.rs` ✅ 已迁移 (100% 完成，5个测试) 
- [ ] `builtins/continue.ts` → `builtins/continue_cmd.rs` ✅ 已迁移 (100% 完成，3个测试) 
- [ ] `builtins/declare-array-parsing.ts` → `builtins/declare_array_parsing.rs` ✅ 已迁移 (100% 完成，10个测试) 
- [ ] `builtins/declare-print.ts` → `builtins/declare_print.rs` ✅ 已迁移 (100% 完成，9个测试) 
- [ ] `builtins/declare.ts` → `builtins/declare_cmd.rs` ✅ 已迁移
- [ ] `builtins/dirs.ts` → `builtins/dirs_cmd.rs` ✅ 已迁移
- [ ] `builtins/eval.ts` → `builtins/eval_cmd.rs` ✅ 已迁移 (参数解析和状态管理，执行需运行时)
- [ ] `builtins/exit.ts` → `builtins/exit_cmd.rs` ✅ 已迁移 (100% 完成，5个测试) 
- [ ] `builtins/export.ts` → `builtins/export_cmd.rs` ✅ 已迁移 (100% 完成，9个测试) 
- [ ] `builtins/getopts.ts` → `builtins/getopts_cmd.rs` ✅ 已迁移 (100% 完成，8个测试) 
- [ ] `builtins/hash.ts` → `builtins/hash_cmd.rs` ✅ 已迁移
- [ ] `builtins/help.ts` → `builtins/help_cmd.rs` ✅ 已迁移 (95% 完成，7个测试，部分帮助文档未迁移) 
- [ ] `builtins/index.ts` → `builtins/mod.rs` ✅ 已完成 (模块导出)
- [ ] `builtins/let.ts` → `builtins/let_cmd.rs` ✅ 已迁移
- [ ] `builtins/local.ts` → `builtins/local_cmd.rs` ✅ 已迁移 (已补充 tempenv 和 localvar-nest 处理)
- [ ] `builtins/mapfile.ts` → `builtins/mapfile_cmd.rs` ✅ 已迁移 (100% 完成，7个测试) 
- [ ] `builtins/read.ts` → `builtins/read_cmd.rs` ✅ 已迁移 (100% 完成，10个测试) 
- [ ] `builtins/return.ts` → `builtins/return_cmd.rs` ✅ 已迁移 (100% 完成，5个测试) 
- [ ] `builtins/set.ts` → `builtins/set_cmd.rs` ✅ 已迁移 (100% 完成，9个测试) 
- [ ] `builtins/shift.ts` → `builtins/shift_cmd.rs` ✅ 已迁移 (100% 完成，8个测试) 
- [ ] `builtins/shopt.ts` → `builtins/shopt_cmd.rs` ✅ 已迁移 (100% 完成，9个测试) 
- [ ] `builtins/source.ts` → `builtins/source_cmd.rs` ✅ 已迁移 (参数解析和状态管理，执行需运行时)
- [ ] `builtins/unset.ts` → `builtins/unset_cmd.rs` ✅ 已迁移 (已补充 cell-unset/dynamic-unset 和算术表达式索引)
- [ ] `builtins/variable-assignment.ts` → `builtins/variable_assignment.rs` ✅ 已迁移

#### 测试文件 (已内联到 Rust 模块)
- [ ] `builtins/break.test.ts` → `break_cmd.rs` 内联测试 ✅
- [ ] `builtins/cd.test.ts` → `cd_cmd.rs` 内联测试 ✅
- [ ] `builtins/complete.test.ts` → `complete_cmd.rs` 内联测试 ✅
- [ ] `builtins/compopt.test.ts` → `compopt_cmd.rs` 内联测试 ✅
- [ ] `builtins/continue.test.ts` → `continue_cmd.rs` 内联测试 ✅
- [ ] `builtins/eval.test.ts` → `eval_cmd.rs` 内联测试 ✅
- [ ] `builtins/exit.test.ts` → `exit_cmd.rs` 内联测试 ✅
- [ ] `builtins/export.test.ts` → `export_cmd.rs` 内联测试 ✅
- [ ] `builtins/local.test.ts` → `local_cmd.rs` 内联测试 ✅
- [ ] `builtins/posix-fatal.test.ts` → 相关模块内联测试 ✅
- [ ] `builtins/read.test.ts` → `read_cmd.rs` 内联测试 ✅
- [ ] `builtins/return.test.ts` → `return_cmd.rs` 内联测试 ✅
- [ ] `builtins/set.test.ts` → `set_cmd.rs` 内联测试 ✅
- [ ] `builtins/shift.test.ts` → `shift_cmd.rs` 内联测试 ✅
- [ ] `builtins/source.test.ts` → `source_cmd.rs` 内联测试 ✅
- [ ] `builtins/unset.test.ts` → `unset_cmd.rs` 内联测试 ✅

### expansion/ (23 个源文件 + 1 个测试文件)

#### 源文件
- [ ] `expansion/analysis.ts` → `expansion/analysis.rs` 
- [ ] `expansion/arith-text-expansion.ts` → `expansion/arith_text_expansion.rs`  (已补充命令替换回调)
- [ ] `expansion/array-pattern-ops.ts` → `expansion/array_pattern_ops.rs` 
- [ ] `expansion/array-prefix-suffix.ts` → `expansion/array_prefix_suffix.rs`  (已补充 AssignDefault)
- [ ] `expansion/array-slice-transform.ts` → `expansion/array_slice_transform.rs`  (Rust 更完整)
- [ ] `expansion/array-word-expansion.ts` → `expansion/array_word_expansion.rs` 
- [ ] `expansion/brace-range.ts` → `expansion/brace_range.rs` 
- [ ] `expansion/command-substitution.ts` → `expansion/command_substitution.rs` 
- [ ] `expansion/glob-escape.ts` → `expansion/glob_escape.rs` 
- [ ] `expansion/indirect-expansion.ts` → `expansion/indirect_expansion.rs`  (已补充 alternative 处理)
- [ ] `expansion/parameter-ops.ts` → `expansion/parameter_ops.rs`  (已补充多个操作)
- [ ] `expansion/pattern-expansion.ts` → `expansion/pattern_expansion.rs`  (已补充命令替换回调)
- [ ] `expansion/pattern-removal.ts` → `expansion/pattern_removal.rs` 
- [ ] `expansion/pattern.ts` → `expansion/pattern.rs` 
- [ ] `expansion/positional-params.ts` → `expansion/positional_params.rs` 
- [ ] `expansion/prompt.ts` → `expansion/prompt.rs` 
- [ ] `expansion/quoting.ts` → `expansion/quoting.rs` 
- [ ] `expansion/tilde.ts` → `expansion/tilde.rs` 
- [ ] `expansion/unquoted-expansion.ts` → `expansion/unquoted_expansion.rs`  (已补充切片和模式操作)
- [ ] `expansion/variable-attrs.ts` → `expansion/variable_attrs.rs` 
- [ ] `expansion/variable.ts` → `expansion/variable.rs` 
- [ ] `expansion/word-glob-expansion.ts` → `expansion/word_glob_expansion.rs`  (已补充 word 展开辅助函数)
- [ ] `expansion/word-split.ts` → `expansion/word_split.rs` 

#### 测试文件 (已内联到 Rust 模块)
- [ ] `expansion/prompt.test.ts` → `prompt.rs` 内联测试 ✅

### helpers/ (22 个源文件 + 1 个测试文件)

#### 源文件
- [ ] `helpers/array.ts` → `helpers/array.rs` 
- [ ] `helpers/condition.ts` → `helpers/condition.rs` 
- [ ] `helpers/errors.ts` → `helpers/error_utils.rs` 
- [ ] `helpers/file-tests.ts` → `helpers/file_tests.rs`  (已补充)
- [ ] `helpers/ifs.ts` → `helpers/ifs.rs` 
- [ ] `helpers/loop.ts` → `helpers/loop_helpers.rs` 
- [ ] `helpers/nameref.ts` → `helpers/nameref.rs` 
- [ ] `helpers/numeric-compare.ts` → `helpers/numeric_compare.rs` 
- [ ] `helpers/quoting.ts` → `helpers/quoting.rs` 
- [ ] `helpers/readonly.ts` → `helpers/readonly.rs` 
- [ ] `helpers/regex.ts` → `helpers/regex_utils.rs` 
- [ ] `helpers/result.ts` → `helpers/result.rs`  (API 略有差异)
- [ ] `helpers/shell-constants.ts` → `helpers/shell_constants.rs` 
- [ ] `helpers/shellopts.ts` → `helpers/shellopts.rs` 
- [ ] `helpers/statements.ts` → `helpers/statements.rs`  (已补充)
- [ ] `helpers/string-compare.ts` → `helpers/string_compare.rs`  (已补充)
- [ ] `helpers/string-tests.ts` → `helpers/string_tests.rs` 
- [ ] `helpers/tilde.ts` → `helpers/tilde.rs` 
- [ ] `helpers/variable-tests.ts` → `helpers/variable_tests.rs`  (已补充)
- [ ] `helpers/word-matching.ts` → `helpers/word_matching.rs` 
- [ ] `helpers/word-parts.ts` → `helpers/word_parts.rs` 
- [ ] `helpers/xtrace.ts` → `helpers/xtrace.rs`  (已补充)

#### 测试文件 (已内联到 Rust 模块)
- [ ] `helpers/xtrace.test.ts` → `xtrace.rs` 内联测试 ✅

