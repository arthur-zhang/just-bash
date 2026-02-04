# Just-Bash TypeScript to Rust Migration Tracker

## Parser 模块 (11 个文件)

- [x] `parser/arithmetic-parser.ts` → `parser/arithmetic_parser.rs` ✅ 已对齐
- [x] `parser/arithmetic-primaries.ts` → `parser/arithmetic_primaries.rs` ✅ 已对齐
- [x] `parser/command-parser.ts` → `parser/command_parser.rs` ✅ 已对齐
- [x] `parser/compound-parser.ts` → `parser/compound_parser.rs` ✅ 已对齐
- [x] `parser/conditional-parser.ts` → `parser/conditional_parser.rs` ✅ 已对齐
- [x] `parser/expansion-parser.ts` → `parser/expansion_parser.rs` ✅ 已对齐
- [x] `parser/lexer.ts` → `parser/lexer.rs` ✅ 已对齐
- [x] `parser/parser-substitution.ts` → `parser/parser_substitution.rs` ✅ 已对齐
- [x] `parser/parser.ts` → `parser/parser.rs` ✅ 已对齐
- [x] `parser/types.ts` → `parser/types.rs` ✅ 已对齐
- [x] `parser/word-parser.ts` → `parser/word_parser.rs` ✅ 已对齐

## Interpreter 模块

### 主目录 (18 个源文件 + 5 个测试文件)

#### 源文件
- [x] `interpreter/alias-expansion.ts` → `interpreter/alias_expansion.rs` ✅ 已对齐
- [x] `interpreter/arithmetic.ts` → `interpreter/arithmetic.rs` ✅ 已对齐
- [ ] `interpreter/assignment-expansion.ts` → **未迁移**
- [ ] `interpreter/builtin-dispatch.ts` → **未迁移**
- [x] `interpreter/command-resolution.ts` → `interpreter/command_resolution.rs` ✅ 已对齐
- [x] `interpreter/conditionals.ts` → `interpreter/conditionals.rs` ✅ 已对齐
- [x] `interpreter/control-flow.ts` → `interpreter/control_flow.rs` ✅ 已对齐
- [x] `interpreter/errors.ts` → `interpreter/errors.rs` ✅ 已对齐
- [ ] `interpreter/expansion.ts` → **未迁移** (有 expansion/ 目录但没有主文件)
- [x] `interpreter/functions.ts` → `interpreter/functions.rs` ✅ 已对齐
- [x] `interpreter/index.ts` → `interpreter/mod.rs`
- [ ] `interpreter/interpreter.ts` → **未迁移**
- [x] `interpreter/pipeline-execution.ts` → `interpreter/pipeline_execution.rs` ✅ 已对齐
- [ ] `interpreter/redirections.ts` → **未迁移**
- [ ] `interpreter/simple-command-assignments.ts` → **未迁移**
- [x] `interpreter/subshell-group.ts` → `interpreter/subshell_group.rs` ✅ 已对齐
- [ ] `interpreter/type-command.ts` → **未迁移**
- [x] `interpreter/types.ts` → `interpreter/types.rs` ✅ 已对齐

#### 测试文件
- [ ] `interpreter/arithmetic.test.ts`
- [ ] `interpreter/assoc-array.test.ts`
- [ ] `interpreter/control-flow.test.ts`
- [ ] `interpreter/redirections.binary.test.ts`

### builtins/ (28 个源文件 + 16 个测试文件) - **部分迁移**

#### 源文件
- [x] `builtins/break.ts` → `builtins/break_cmd.rs` ✅ 已对齐
- [ ] `builtins/cd.ts`
- [ ] `builtins/compgen.ts`
- [x] `builtins/complete.ts` → `builtins/complete_cmd.rs` ✅ 已对齐
- [x] `builtins/compopt.ts` → `builtins/compopt_cmd.rs` ✅ 已对齐
- [x] `builtins/continue.ts` → `builtins/continue_cmd.rs` ✅ 已对齐
- [x] `builtins/declare-array-parsing.ts` → `builtins/declare_array_parsing.rs` ✅ 已对齐
- [x] `builtins/declare-print.ts` → `builtins/declare_print.rs` ✅ 已对齐
- [ ] `builtins/declare.ts`
- [ ] `builtins/dirs.ts`
- [ ] `builtins/eval.ts`
- [x] `builtins/exit.ts` → `builtins/exit_cmd.rs` ✅ 已对齐
- [x] `builtins/export.ts` → `builtins/export_cmd.rs` ✅ 已对齐
- [x] `builtins/getopts.ts` → `builtins/getopts_cmd.rs` ✅ 已对齐
- [ ] `builtins/hash.ts`
- [x] `builtins/help.ts` → `builtins/help_cmd.rs` ✅ 已对齐
- [ ] `builtins/index.ts`
- [ ] `builtins/let.ts`
- [ ] `builtins/local.ts`
- [x] `builtins/mapfile.ts` → `builtins/mapfile_cmd.rs` ✅ 已对齐
- [x] `builtins/read.ts` → `builtins/read_cmd.rs` ✅ 已对齐
- [x] `builtins/return.ts` → `builtins/return_cmd.rs` ✅ 已对齐
- [x] `builtins/set.ts` → `builtins/set_cmd.rs` ✅ 已对齐
- [x] `builtins/shift.ts` → `builtins/shift_cmd.rs` ✅ 已对齐
- [x] `builtins/shopt.ts` → `builtins/shopt_cmd.rs` ✅ 已对齐
- [ ] `builtins/source.ts`
- [ ] `builtins/unset.ts`
- [ ] `builtins/variable-assignment.ts`

#### 测试文件
- [ ] `builtins/break.test.ts`
- [ ] `builtins/cd.test.ts`
- [ ] `builtins/complete.test.ts`
- [ ] `builtins/compopt.test.ts`
- [ ] `builtins/continue.test.ts`
- [ ] `builtins/eval.test.ts`
- [ ] `builtins/exit.test.ts`
- [ ] `builtins/export.test.ts`
- [ ] `builtins/local.test.ts`
- [ ] `builtins/posix-fatal.test.ts`
- [ ] `builtins/read.test.ts`
- [ ] `builtins/return.test.ts`
- [ ] `builtins/set.test.ts`
- [ ] `builtins/shift.test.ts`
- [ ] `builtins/source.test.ts`
- [ ] `builtins/unset.test.ts`

### expansion/ (23 个源文件 + 1 个测试文件)

#### 源文件
- [x] `expansion/analysis.ts` → `expansion/analysis.rs` ✅ 已对齐
- [x] `expansion/arith-text-expansion.ts` → `expansion/arith_text_expansion.rs` ⚠️ 不完整 (不执行命令替换)
- [x] `expansion/array-pattern-ops.ts` → `expansion/array_pattern_ops.rs` ✅ 已对齐
- [x] `expansion/array-prefix-suffix.ts` → `expansion/array_prefix_suffix.rs` ⚠️ 不完整 (缺少 AssignDefault、AST 解析)
- [x] `expansion/array-slice-transform.ts` → `expansion/array_slice_transform.rs` ✅ 已对齐 (Rust 更完整)
- [x] `expansion/array-word-expansion.ts` → `expansion/array_word_expansion.rs` ✅ 已对齐
- [x] `expansion/brace-range.ts` → `expansion/brace_range.rs` ✅ 已对齐
- [x] `expansion/command-substitution.ts` → `expansion/command_substitution.rs` ✅ 已对齐
- [x] `expansion/glob-escape.ts` → `expansion/glob_escape.rs` ✅ 已对齐
- [x] `expansion/indirect-expansion.ts` → `expansion/indirect_expansion.rs` ⚠️ 不完整 (缺少多个函数)
- [x] `expansion/parameter-ops.ts` → `expansion/parameter_ops.rs` ⚠️ 不完整 (缺少多个操作)
- [x] `expansion/pattern-expansion.ts` → `expansion/pattern_expansion.rs` ⚠️ 不完整 (缺少异步命令替换)
- [x] `expansion/pattern-removal.ts` → `expansion/pattern_removal.rs` ✅ 已对齐
- [x] `expansion/pattern.ts` → `expansion/pattern.rs` ✅ 已对齐
- [x] `expansion/positional-params.ts` → `expansion/positional_params.rs` ✅ 已对齐
- [x] `expansion/prompt.ts` → `expansion/prompt.rs` ✅ 已对齐
- [x] `expansion/quoting.ts` → `expansion/quoting.rs` ✅ 已对齐
- [x] `expansion/tilde.ts` → `expansion/tilde.rs` ✅ 已对齐
- [x] `expansion/unquoted-expansion.ts` → `expansion/unquoted_expansion.rs` ⚠️ 不完整 (缺少大量函数)
- [x] `expansion/variable-attrs.ts` → `expansion/variable_attrs.rs` ✅ 已对齐
- [x] `expansion/variable.ts` → `expansion/variable.rs` ✅ 已对齐
- [x] `expansion/word-glob-expansion.ts` → `expansion/word_glob_expansion.rs` ⚠️ 不完整 (只有基础 glob)
- [x] `expansion/word-split.ts` → `expansion/word_split.rs` ✅ 已对齐

#### 测试文件
- [ ] `expansion/prompt.test.ts`

### helpers/ (22 个源文件 + 1 个测试文件)

#### 源文件
- [x] `helpers/array.ts` → `helpers/array.rs` ✅ 已对齐
- [x] `helpers/condition.ts` → `helpers/condition.rs` ✅ 已对齐
- [x] `helpers/errors.ts` → `helpers/error_utils.rs` ✅ 已对齐
- [x] `helpers/file-tests.ts` → `helpers/file_tests.rs` ✅ 已对齐 (已补充)
- [x] `helpers/ifs.ts` → `helpers/ifs.rs` ✅ 已对齐
- [x] `helpers/loop.ts` → `helpers/loop_helpers.rs` ✅ 已对齐
- [x] `helpers/nameref.ts` → `helpers/nameref.rs` ✅ 已对齐
- [x] `helpers/numeric-compare.ts` → `helpers/numeric_compare.rs` ✅ 已对齐
- [x] `helpers/quoting.ts` → `helpers/quoting.rs` ✅ 已对齐
- [x] `helpers/readonly.ts` → `helpers/readonly.rs` ✅ 已对齐
- [x] `helpers/regex.ts` → `helpers/regex_utils.rs` ✅ 已对齐
- [x] `helpers/result.ts` → `helpers/result.rs` ✅ 已对齐 (API 略有差异)
- [x] `helpers/shell-constants.ts` → `helpers/shell_constants.rs` ✅ 已对齐
- [x] `helpers/shellopts.ts` → `helpers/shellopts.rs` ✅ 已对齐
- [x] `helpers/statements.ts` → `helpers/statements.rs` ✅ 已对齐 (已补充)
- [x] `helpers/string-compare.ts` → `helpers/string_compare.rs` ✅ 已对齐 (已补充)
- [x] `helpers/string-tests.ts` → `helpers/string_tests.rs` ✅ 已对齐
- [x] `helpers/tilde.ts` → `helpers/tilde.rs` ✅ 已对齐
- [x] `helpers/variable-tests.ts` → `helpers/variable_tests.rs` ✅ 已对齐 (已补充)
- [x] `helpers/word-matching.ts` → `helpers/word_matching.rs` ✅ 已对齐
- [x] `helpers/word-parts.ts` → `helpers/word_parts.rs` ✅ 已对齐
- [x] `helpers/xtrace.ts` → `helpers/xtrace.rs` ✅ 已对齐 (已补充)

#### 测试文件
- [ ] `helpers/xtrace.test.ts`

---

## 统计

| 模块 | 已迁移 | 未迁移 | 完全对齐 | 部分对齐 | 总计 |
|------|--------|--------|----------|----------|------|
| parser | 11 | 0 | 11 ✅ | 0 | 11 |
| interpreter (主目录) | 11 | 7 | 11 ✅ | 0 | 18 |
| interpreter/builtins | 16 | 12 | 16 ✅ | 0 | 28 |
| interpreter/expansion | 23 | 0 | 23 ✅ | 0 | 23 |
| interpreter/helpers | 22 | 0 | 22 ✅ | 0 | 22 |
| **源文件总计** | **83** | **19** | **83** | **0** | **102** |
| 测试文件 | 0 | 22 | 0 | 0 | 22 |

## 迁移进度

- 源文件已迁移: 83 / 102 (81%)
- 完全对齐: 83 / 83 (100%) ✅
- 部分对齐 (需补充): 0 / 83 (0%)
- 测试文件已完成: 0 / 22 (0%)

---

## 对比详情

### Parser 模块 (11/11 完全对齐) ✅
| 文件 | 状态 | 备注 |
|------|------|------|
| arithmetic-parser.ts | ✅ | 已修复 |
| arithmetic-primaries.ts | ✅ | |
| command-parser.ts | ✅ | |
| compound-parser.ts | ✅ | |
| conditional-parser.ts | ✅ | |
| expansion-parser.ts | ✅ | |
| lexer.ts | ✅ | 已修复 |
| parser-substitution.ts | ✅ | |
| parser.ts | ✅ | 已修复 |
| types.ts | ✅ | 已修复 |
| word-parser.ts | ✅ | |

### Interpreter 主目录 (10/10 完全对齐) ✅
| 文件 | 状态 | 备注 |
|------|------|------|
| alias-expansion.ts | ✅ | |
| arithmetic.ts | ✅ | 已修复 |
| command-resolution.ts | ✅ | |
| control-flow.ts | ✅ | |
| errors.ts | ✅ | |
| functions.ts | ✅ | 已修复 |
| pipeline-execution.ts | ✅ | 已修复 |
| subshell-group.ts | ✅ | 已修复 |
| types.ts | ✅ | |

### Expansion 目录 (23/23 完全对齐) ✅
| 文件 | 状态 | 备注 |
|------|------|------|
| analysis.ts | ✅ | |
| arith-text-expansion.ts | ✅ | 已补充命令替换回调 |
| array-pattern-ops.ts | ✅ | 架构差异是有意设计 |
| array-prefix-suffix.ts | ✅ | 已补充 AssignDefault |
| array-slice-transform.ts | ✅ | Rust 版本更完整 |
| array-word-expansion.ts | ✅ | |
| brace-range.ts | ✅ | |
| command-substitution.ts | ✅ | |
| glob-escape.ts | ✅ | |
| indirect-expansion.ts | ✅ | 已补充 alternative 处理 |
| parameter-ops.ts | ✅ | 已补充多个操作处理 |
| pattern-expansion.ts | ✅ | 已补充命令替换回调 |
| pattern-removal.ts | ✅ | |
| pattern.ts | ✅ | |
| positional-params.ts | ✅ | |
| prompt.ts | ✅ | |
| quoting.ts | ✅ | |
| tilde.ts | ✅ | |
| unquoted-expansion.ts | ✅ | 已补充切片和模式操作 |
| variable-attrs.ts | ✅ | |
| variable.ts | ✅ | |
| word-glob-expansion.ts | ✅ | 已补充 word 展开辅助函数 |
| word-split.ts | ✅ | 已修复 |

### Helpers 目录 (22/22 完全对齐) ✅
| 文件 | 状态 | 备注 |
|------|------|------|
| array.ts | ✅ | |
| condition.ts | ✅ | |
| errors.ts | ✅ | |
| file-tests.ts | ✅ | 已补充 evaluateFileTest |
| ifs.ts | ✅ | |
| loop.ts | ✅ | |
| nameref.ts | ✅ | |
| numeric-compare.ts | ✅ | |
| quoting.ts | ✅ | |
| readonly.ts | ✅ | |
| regex.ts | ✅ | |
| result.ts | ✅ | API 略有差异 (failure_with_code) |
| shell-constants.ts | ✅ | |
| shellopts.ts | ✅ | 已修复 |
| statements.ts | ✅ | 已补充 executeStatements |
| string-compare.ts | ✅ | 已补充模式匹配支持 |
| string-tests.ts | ✅ | |
| tilde.ts | ✅ | |
| variable-tests.ts | ✅ | 已补充算术表达式求值 |
| word-matching.ts | ✅ | |
| word-parts.ts | ✅ | |
| xtrace.ts | ✅ | 已补充 PS4 变量展开 |

---

## 待迁移关键文件

### 高优先级 (核心解释器)
1. `interpreter/interpreter.ts` - 主解释器入口
2. `interpreter/conditionals.ts` - 条件表达式
3. `interpreter/redirections.ts` - 重定向处理
4. `interpreter/expansion.ts` - 展开主入口
5. `interpreter/builtin-dispatch.ts` - 内建命令分发

### 中优先级 (内建命令)
- 整个 `builtins/` 目录 (28 个文件)

### 低优先级 (辅助功能)
- `interpreter/assignment-expansion.ts`
- `interpreter/simple-command-assignments.ts`
- `interpreter/type-command.ts`

---

## 需要补充的功能 (部分对齐文件)

### Expansion 模块 (8 个文件需补充)

1. ~~**arith-text-expansion.rs** - 需要添加命令替换执行~~ ✅ 已补充
2. ~~**array-prefix-suffix.rs** - 需要添加 AssignDefault 操作、AST 解析~~ ✅ 已补充
3. ~~**indirect-expansion.rs** - 需要添加 handleIndirectInAlternative 等函数~~ ✅ 已补充
4. ~~**parameter-ops.rs** - 需要添加 handleAssignDefault、handleErrorIfUnset、handleIndirection 等~~ ✅ 已补充
5. ~~**pattern-expansion.rs** - 需要添加异步命令替换执行~~ ✅ 已补充
6. ~~**unquoted-expansion.rs** - 需要添加模式操作、切片操作、glob 展开等~~ ✅ 已补充
7. ~~**word-glob-expansion.rs** - 需要添加完整的 word 展开逻辑~~ ✅ 已补充

### Helpers 模块 (1 个文件需补充)

1. ~~**file-tests.rs** - 需要添加 evaluateFileTest、evaluateBinaryFileTest~~ ✅ 已补充
2. ~~**statements.rs** - 需要添加 executeStatements 函数~~ ✅ 已补充
3. ~~**string-compare.rs** - 需要添加模式匹配支持 (usePattern, extglob)~~ ✅ 已补充
4. ~~**variable-tests.rs** - 需要添加算术表达式求值~~ ✅ 已补充
5. ~~**xtrace.rs** - 需要添加 PS4 变量展开~~ ✅ 已补充

---

## 本次补充完成的文件

### Helpers 模块 (5 个文件已补充)

1. **file-tests.rs** ✅
   - 添加了 `FileStat` 结构体
   - 添加了 `FileSystem` trait
   - 添加了 `evaluate_file_test` 函数
   - 添加了 `evaluate_binary_file_test` 函数
   - 添加了 `evaluate_file_test_str` 和 `evaluate_binary_file_test_str` 辅助函数

2. **string-compare.rs** ✅
   - 添加了 `compare_strings_with_pattern` 函数，支持模式匹配
   - 添加了 `compare_strings_with_pattern_str` 辅助函数
   - 支持 `use_pattern`、`nocasematch`、`extglob` 参数

3. **variable-tests.rs** ✅
   - 添加了 `VariableTestResult` 结构体
   - 添加了 `evaluate_variable_test_with_arith` 函数，支持算术表达式求值回调

4. **statements.rs** ✅
   - 添加了 `StatementError` 枚举
   - 添加了 `execute_statements` 泛型函数
   - 添加了 `execute_statements_with_catch` 函数，支持错误捕获

5. **xtrace.rs** ✅
   - 添加了 `get_xtrace_prefix_expanded` 函数，支持 PS4 变量展开
   - 添加了 `get_xtrace_prefix_with_error` 函数，支持错误报告

### Expansion 模块 (4 个文件已补充)

6. **parameter-ops.rs** ✅
   - 添加了 `handle_default_value` 函数
   - 添加了 `handle_assign_default` 函数
   - 添加了 `ErrorIfUnsetResult` 结构体
   - 添加了 `handle_error_if_unset` 函数
   - 添加了 `handle_use_alternative` 函数
   - 添加了 `handle_indirection` 函数
   - 添加了 `get_parameter_length_extended` 函数
   - 添加了 `apply_substring_to_array` 函数
   - 添加了 `apply_case_modification_to_array` 函数
   - 添加了 `apply_transform_op_extended` 函数 (支持 A, E, K, k 操作符)

7. **indirect-expansion.rs** ✅
   - 添加了 `IndirectAlternativeContext` 结构体
   - 添加了 `check_indirect_in_alternative` 函数 (处理 ${ref+${!ref}} 模式)
   - 添加了 `check_indirection_with_inner_alternative` 函数 (处理 ${!ref+${!ref}} 模式)
   - 添加了 `check_indirect_array_assign_default` 函数

8. **array-prefix-suffix.rs** ✅
   - 添加了 `check_array_assign_default` 函数
   - 添加了 `parse_array_subscript` 函数

9. **arith-text-expansion.rs** ✅
   - 添加了 `SubscriptExpansionResult` 结构体
   - 添加了 `expand_subscript_for_assoc_array_with_exec` 函数，支持命令替换回调

10. **pattern-expansion.rs** ✅
    - 添加了 `PatternExpansionResult` 结构体
    - 添加了 `expand_variables_in_pattern_with_exec` 函数，支持命令替换回调
    - 添加了 `expand_variables_in_double_quoted_pattern_with_exec` 辅助函数

11. **unquoted-expansion.rs** ✅
    - 添加了 `expand_unquoted_array_pattern_removal` 函数
    - 添加了 `expand_unquoted_array_pattern_replacement` 函数
    - 添加了 `expand_unquoted_positional_slice` 函数
    - 添加了 `expand_unquoted_array_slice` 函数

12. **word-glob-expansion.rs** ✅
    - 添加了 `WordExpansionOptions` 结构体
    - 添加了 `handle_brace_expansion_results` 函数
    - 添加了 `split_and_glob_expand_with_state` 函数
    - 添加了 `expand_values_with_glob` 函数
