# Batch C: jq/yq Command Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement complete jq and yq commands with shared query-engine, porting all functionality from TypeScript.

**Architecture:** Three-layer design: (1) query-engine core (lexer, parser, evaluator, builtins), (2) jq command for JSON processing, (3) yq command for multi-format processing. The query-engine is shared between jq and yq.

**Tech Stack:** Rust, serde_json, serde_yaml, quick-xml, toml, csv, indexmap, base64, chrono, regex

**Reference:** TypeScript source at `/Users/arthur/PycharmProjects/just-bash/src/commands/`

---

## Module Structure

```
src/commands/
├── query_engine/           # 共享查询引擎 (~5,000 行)
│   ├── mod.rs             # 导出
│   ├── value.rs           # Value 类型定义
│   ├── ast.rs             # AST 节点定义
│   ├── lexer.rs           # 词法分析器
│   ├── parser.rs          # 递归下降解析器
│   ├── evaluator.rs       # 主解释器
│   ├── context.rs         # 执行上下文
│   ├── operations.rs      # 值/路径操作
│   └── builtins/          # 内置函数 (~150+)
│       ├── mod.rs
│       ├── string.rs
│       ├── array.rs
│       ├── object.rs
│       ├── math.rs
│       ├── date.rs
│       ├── format.rs
│       ├── type_fns.rs
│       ├── control.rs
│       ├── navigation.rs
│       ├── path.rs
│       ├── index.rs
│       └── sql.rs
├── jq/
│   └── mod.rs             # jq 命令 (~300 行)
└── yq/
    ├── mod.rs             # yq 命令 (~400 行)
    └── formats.rs         # 格式处理器 (~300 行)
```

---

## Task 1: Value Types and AST (`value.rs`, `ast.rs`)

**Files:**
- Create: `src/commands/query_engine/mod.rs`
- Create: `src/commands/query_engine/value.rs`
- Create: `src/commands/query_engine/ast.rs`

**Step 1: Create the query_engine module structure**

Create `src/commands/query_engine/mod.rs`:
```rust
pub mod value;
pub mod ast;

pub use value::Value;
pub use ast::*;
```

**Step 2: Implement Value type**

Create `src/commands/query_engine/value.rs` with:
- `Value` enum: Null, Bool, Number, String, Array, Object
- Use `IndexMap<String, Value>` for objects (preserves insertion order)
- Implement `Display`, `Clone`, `PartialEq`, `Debug`
- Implement `From` traits for common types
- Implement `Value::is_truthy()` - false and null are falsy
- Implement `Value::type_name()` - returns "null", "boolean", "number", "string", "array", "object"

Port from: `value-operations.ts` (167 lines)

**Step 3: Implement AST types**

Create `src/commands/query_engine/ast.rs` with:
- `TokenType` enum (all 67 token types)
- `Token` struct with type, value, position
- `BinaryOp` enum: Add, Sub, Mul, Div, Mod, Eq, Ne, Lt, Le, Gt, Ge, And, Or, Alt
- `UnaryOp` enum: Neg, Not
- `UpdateOp` enum: Assign, PipeUpdate, AddUpdate, SubUpdate, MulUpdate, DivUpdate, ModUpdate, AltUpdate
- `AstNode` enum (all 26 node types)
- `DestructurePattern` enum for variable binding
- `ObjectEntry` struct for object construction

Port from: `parser-types.ts` (299 lines)

**Step 4: Run cargo check**

Run: `cargo check 2>&1 | head -30`

**Step 5: Commit**

```bash
git add src/commands/query_engine/
git commit -m "feat(query-engine): add Value type and AST definitions"
```

---

## Task 2: Lexer (`lexer.rs`)

**Files:**
- Create: `src/commands/query_engine/lexer.rs`
- Modify: `src/commands/query_engine/mod.rs`

**Step 1: Write lexer tests**

Test cases (~20):
- Tokenize `.` → [Dot, Eof]
- Tokenize `..` → [DotDot, Eof]
- Tokenize `.foo` → [Dot, Ident("foo"), Eof]
- Tokenize `.[0]` → [Dot, LBracket, Number(0), RBracket, Eof]
- Tokenize `"hello"` → [String("hello"), Eof]
- Tokenize `123` → [Number(123), Eof]
- Tokenize `3.14` → [Number(3.14), Eof]
- Tokenize `1e-5` → [Number(0.00001), Eof]
- Tokenize `true false null` → [True, False, Null, Eof]
- Tokenize `if then else elif end` → keywords
- Tokenize `and or not` → keywords
- Tokenize `== != < <= > >=` → comparison ops
- Tokenize `+ - * / %` → arithmetic ops
- Tokenize `// //=` → Alt, UpdateAlt
- Tokenize `|= += -= *= /= %=` → update ops
- Tokenize `$var` → Ident("$var")
- Tokenize `@base64` → Ident("@base64")
- Tokenize `# comment\n.` → [Dot, Eof] (skip comment)
- Tokenize string escapes: `"a\nb\tc"` → String("a\nb\tc")
- Tokenize string interpolation marker: `"hello \(name)"` → String with `\(` preserved

**Step 2: Implement the lexer**

Key implementation:
- `pub fn tokenize(input: &str) -> Result<Vec<Token>, String>`
- Single-pass with lookahead via `peek()` and `advance()`
- Character classification helpers: `is_digit`, `is_alpha`, `is_alnum`
- Multi-character operator detection: `==`, `!=`, `<=`, `>=`, `//`, `+=`, etc.
- Keyword recognition via HashMap lookup
- String parsing with escape sequences: `\n`, `\r`, `\t`, `\\`, `\"`, `\(`
- Number parsing: integers, decimals, scientific notation
- Comment handling: `#` to end of line

Port from: `parser.ts` lines 82-339

**Step 3: Run tests**

Run: `cargo test query_engine::lexer --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/query_engine/lexer.rs src/commands/query_engine/mod.rs
git commit -m "feat(query-engine): add lexer/tokenizer"
```

---

## Task 3: Parser (`parser.rs`)

**Files:**
- Create: `src/commands/query_engine/parser.rs`
- Modify: `src/commands/query_engine/mod.rs`

**Step 1: Write parser tests**

Test cases (~25):
- Parse `.` → Identity
- Parse `.foo` → Field { name: "foo" }
- Parse `.[0]` → Index { index: Literal(0) }
- Parse `.[]` → Iterate
- Parse `.[1:3]` → Slice { start: 1, end: 3 }
- Parse `..` → Recurse
- Parse `.foo | .bar` → Pipe { left: Field, right: Field }
- Parse `.a, .b` → Comma { left, right }
- Parse `1 + 2` → BinaryOp { op: Add }
- Parse `1 == 2` → BinaryOp { op: Eq }
- Parse `.x and .y` → BinaryOp { op: And }
- Parse `.x // .y` → BinaryOp { op: Alt }
- Parse `-1` → UnaryOp { op: Neg }
- Parse `not` → Call { name: "not" }
- Parse `true`, `false`, `null` → Literal
- Parse `"hello"` → Literal(String)
- Parse `[1, 2, 3]` → Array
- Parse `{a: 1}` → Object
- Parse `{a}` → Object (shorthand)
- Parse `if .x then .y else .z end` → Cond
- Parse `try .x catch .y` → Try
- Parse `.x?` → Optional
- Parse `$var` → VarRef
- Parse `.x as $v | .y` → VarBind
- Parse `reduce .[] as $x (0; . + $x)` → Reduce
- Parse `def f: .; f` → Def
- Parse `length` → Call { name: "length" }
- Parse `map(.x)` → Call { name: "map", args: [Field] }

**Step 2: Implement recursive descent parser**

Operator precedence (lowest to highest):
1. Pipe `|`
2. Comma `,`
3. VarBind `as`
4. Update `= |= += -= *= /= %= //=`
5. Alt `//`
6. Or `or`
7. And `and`
8. Comparison `== != < <= > >=`
9. AddSub `+ -`
10. MulDiv `* / %`
11. Unary `- not`
12. Postfix `? .field [index] []`
13. Primary (literals, parens, calls)

Key methods:
- `parse()` → entry point
- `parse_pipe()`, `parse_comma()`, `parse_var_bind()`
- `parse_update()`, `parse_alt()`, `parse_or()`, `parse_and()`
- `parse_comparison()`, `parse_add_sub()`, `parse_mul_div()`
- `parse_unary()`, `parse_postfix()`, `parse_primary()`
- `parse_pattern()` → destructuring patterns
- `parse_object_construction()` → object literals
- `parse_string_interpolation()` → `"text \(expr) text"`
- `parse_if()` → if-then-elif-else-end

Port from: `parser.ts` lines 344-1090

**Step 3: Run tests**

Run: `cargo test query_engine::parser --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/query_engine/parser.rs src/commands/query_engine/mod.rs
git commit -m "feat(query-engine): add recursive descent parser"
```

---

## Task 4: Value Operations (`operations.rs`)

**Files:**
- Create: `src/commands/query_engine/operations.rs`
- Modify: `src/commands/query_engine/mod.rs`

**Step 1: Write tests**

Test cases (~15):
- `is_truthy`: false→false, null→false, 0→true, ""→true, true→true
- `deep_equal`: same values→true, different→false, nested objects
- `compare`: numbers, strings
- `compare_jq`: type ordering (null < bool < number < string < array < object)
- `compare_jq`: within-type comparison (arrays, objects)
- `deep_merge`: nested object merge
- `contains_deep`: string substring, array containment, object containment
- `set_path`: set value at path, create intermediates
- `delete_path`: delete value at path

**Step 2: Implement operations**

Functions to implement:
- `pub fn is_truthy(v: &Value) -> bool` - false and null are falsy
- `pub fn deep_equal(a: &Value, b: &Value) -> bool`
- `pub fn compare(a: &Value, b: &Value) -> Ordering`
- `pub fn compare_jq(a: &Value, b: &Value) -> Ordering` - jq type ordering
- `pub fn deep_merge(a: &Value, b: &Value) -> Value` - recursive object merge
- `pub fn contains_deep(a: &Value, b: &Value) -> bool` - jq containment
- `pub fn get_value_depth(v: &Value) -> usize`
- `pub fn set_path(value: &Value, path: &[PathElement], new_val: Value) -> Value`
- `pub fn delete_path(value: &Value, path: &[PathElement]) -> Value`

Port from: `value-operations.ts` (167 lines) + `path-operations.ts` (93 lines)

**Step 3: Run tests**

Run: `cargo test query_engine::operations --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/query_engine/operations.rs src/commands/query_engine/mod.rs
git commit -m "feat(query-engine): add value and path operations"
```

---

## Task 5: Execution Context (`context.rs`)

**Files:**
- Create: `src/commands/query_engine/context.rs`
- Modify: `src/commands/query_engine/mod.rs`

**Step 1: Implement EvalContext**

```rust
pub struct EvalContext {
    pub vars: HashMap<String, Value>,
    pub funcs: HashMap<String, FunctionDef>,
    pub env: HashMap<String, String>,
    pub root: Option<Value>,
    pub current_path: Vec<PathElement>,
    pub labels: HashSet<String>,
    pub max_iterations: usize,
    pub max_depth: usize,
    pub iteration_count: usize,
}

pub struct FunctionDef {
    pub params: Vec<String>,
    pub body: AstNode,
    pub closure: Option<HashMap<String, FunctionDef>>,
}

pub enum PathElement {
    Key(String),
    Index(usize),
}
```

Key methods:
- `EvalContext::new()` - default limits: max_iterations=10000, max_depth=2000
- `EvalContext::with_var(&self, name, value) -> EvalContext` - immutable update
- `EvalContext::with_func(&self, key, def) -> EvalContext`
- `EvalContext::bind_pattern(&self, pattern, value) -> Option<EvalContext>`

Port from: `evaluator.ts` lines 96-202

**Step 2: Implement error types**

```rust
pub enum JqError {
    Type(String),
    Runtime(String),
    Parse(String),
    Break { name: String, results: Vec<Value> },
    ExecutionLimit(String),
}
```

**Step 3: Run cargo check**

Run: `cargo check 2>&1 | head -30`

**Step 4: Commit**

```bash
git add src/commands/query_engine/context.rs src/commands/query_engine/mod.rs
git commit -m "feat(query-engine): add execution context and error types"
```

---

## Task 6: Core Evaluator (`evaluator.rs`)

**Files:**
- Create: `src/commands/query_engine/evaluator.rs`
- Modify: `src/commands/query_engine/mod.rs`

**Step 1: Write evaluator tests**

Test cases (~30):
- Identity: `.` on `{"a":1}` → `{"a":1}`
- Field: `.a` on `{"a":1}` → `1`
- Field on null: `.a` on `null` → `null`
- Nested field: `.a.b` on `{"a":{"b":2}}` → `2`
- Index: `.[0]` on `[1,2,3]` → `1`
- Negative index: `.[-1]` on `[1,2,3]` → `3`
- Slice: `.[1:3]` on `[1,2,3,4]` → `[2,3]`
- Iterate: `.[]` on `[1,2,3]` → `1, 2, 3`
- Iterate object: `.[]` on `{"a":1,"b":2}` → `1, 2`
- Pipe: `.a | .b` on `{"a":{"b":1}}` → `1`
- Comma: `.a, .b` on `{"a":1,"b":2}` → `1, 2`
- Literal: `42` → `42`
- Array construction: `[.a, .b]` on `{"a":1,"b":2}` → `[1,2]`
- Object construction: `{a: .x}` on `{"x":1}` → `{"a":1}`
- Object shorthand: `{a}` on `{"a":1}` → `{"a":1}`
- BinaryOp add: `1 + 2` → `3`
- BinaryOp string concat: `"a" + "b"` → `"ab"`
- BinaryOp array concat: `[1] + [2]` → `[1,2]`
- BinaryOp object merge: `{"a":1} + {"b":2}` → `{"a":1,"b":2}`
- BinaryOp subtract: `5 - 3` → `2`
- BinaryOp multiply: `3 * 4` → `12`
- BinaryOp divide: `10 / 3` → `3.333...`
- BinaryOp string divide: `"a,b,c" / ","` → `["a","b","c"]`
- BinaryOp modulo: `10 % 3` → `1`
- Comparison: `1 < 2` → `true`
- And/Or: `true and false` → `false`
- Alt: `null // 42` → `42`
- UnaryOp neg: `-(3)` → `-3`
- Cond: `if true then 1 else 2 end` → `1`
- Try: `try error catch "caught"` → `"caught"`
- Optional: `.foo?` on `42` → (empty)
- VarBind: `.x as $v | $v + 1` on `{"x":10}` → `11`
- Recurse: `..` on `{"a":{"b":1}}` → multiple values
- StringInterp: `"hello \(.name)"` on `{"name":"world"}` → `"hello world"`
- UpdateOp: `.a = 5` on `{"a":1}` → `{"a":5}`
- UpdateOp pipe: `.a |= . + 1` on `{"a":1}` → `{"a":2}`
- Reduce: `reduce .[] as $x (0; . + $x)` on `[1,2,3]` → `6`
- Foreach: `foreach .[] as $x (0; . + $x)` on `[1,2,3]` → `1, 3, 6`
- Label/Break: `label $out | foreach .[] as $x (0; . + $x; if . > 3 then ., break $out else . end)` on `[1,2,3]` → `1, 3, 6`
- Def: `def double: . * 2; [.[] | double]` on `[1,2,3]` → `[2,4,6]`
- VarRef $ENV: `$ENV.HOME` → env value

**Step 2: Implement the evaluator**

Core function:
```rust
pub fn evaluate(value: &Value, ast: &AstNode, ctx: &mut EvalContext) -> Result<Vec<Value>, JqError>
```

Implement all AstNode cases matching TypeScript evaluator.ts lines 364-790:
- Identity, Field, Index, Slice, Iterate
- Pipe, Comma, Literal, Array, Object, Paren
- BinaryOp (with short-circuit for and/or/alt)
- UnaryOp, Cond, Try, Optional
- Call (delegates to builtin dispatch)
- VarBind, VarRef ($ENV special case)
- Recurse, StringInterp, UpdateOp
- Reduce, Foreach, Label, Break, Def

Also implement:
- `apply_update()` - recursive path update (lines 797-1008)
- `apply_del()` - recursive path deletion (lines 1010-1227)
- `eval_binary_op()` - all binary operations (lines 1229-1369)
- `normalize_index()` - negative index handling
- `extract_path_from_ast()` - path extraction for parent/root
- `collect_paths()` - path collection for path() builtin

Port from: `evaluator.ts` (1805 lines)

**Step 3: Run tests**

Run: `cargo test query_engine::evaluator --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/query_engine/evaluator.rs src/commands/query_engine/mod.rs
git commit -m "feat(query-engine): add core evaluator"
```

---

## Task 7: Builtin Functions - Math & Type (`builtins/math.rs`, `builtins/type_fns.rs`)

**Files:**
- Create: `src/commands/query_engine/builtins/mod.rs`
- Create: `src/commands/query_engine/builtins/math.rs`
- Create: `src/commands/query_engine/builtins/type_fns.rs`
- Modify: `src/commands/query_engine/mod.rs`

**Step 1: Write tests**

Math tests (~15):
- `floor` on 3.7 → 3
- `ceil` on 3.2 → 4
- `round` on 3.5 → 4
- `sqrt` on 9 → 3
- `pow(2; 10)` → 1024
- `atan2(1; 1)` → ~0.785
- `log` on e → 1
- `fabs` on -5 → 5
- `infinite` → infinity
- `nan` → NaN

Type tests (~10):
- `type` on 1 → "number"
- `type` on "s" → "string"
- `type` on null → "null"
- `arrays` on [1] → [1] (pass through)
- `arrays` on 1 → (empty)
- `numbers` on 1 → 1
- `strings` on "s" → "s"
- `isinfinite` on infinity → true
- `isnan` on NaN → true
- `isnormal` on 1 → true

**Step 2: Implement math builtins**

Simple math (24 functions via lookup table):
floor, ceil, round, sqrt, log, log10, log2, exp, sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, asinh, acosh, atanh, cbrt, expm1, log1p, trunc

Advanced math:
atan2, pow, fabs, hypot, exp2, exp10, log1p, copysign, drem, fdim, fmax, fmin, frexp, ldexp, logb

Port from: `builtins/math-builtins.ts` (159 lines)

**Step 3: Implement type builtins**

Functions: type, arrays, objects, iterables, booleans, numbers, strings, nulls, values, scalars, isnormal, isnan, isinfinite, isfinite, infinite, nan

Port from: `builtins/type-builtins.ts` (105 lines)

**Step 4: Run tests**

Run: `cargo test query_engine::builtins --lib 2>&1 | tail -5`

**Step 5: Commit**

```bash
git add src/commands/query_engine/builtins/
git commit -m "feat(query-engine): add math and type builtins"
```

---

## Task 8: Builtin Functions - String (`builtins/string.rs`)

**Files:**
- Create: `src/commands/query_engine/builtins/string.rs`
- Modify: `src/commands/query_engine/builtins/mod.rs`

**Step 1: Write tests**

Test cases (~20):
- `"hello" | length` → 5
- `"Hello" | ascii_downcase` → "hello"
- `"hello" | ascii_upcase` → "HELLO"
- `"hello" | ltrimstr("hel")` → "lo"
- `"hello" | rtrimstr("lo")` → "hel"
- `"hello" | startswith("hel")` → true
- `"hello" | endswith("lo")` → true
- `"hello" | explode` → [104,101,108,108,111]
- `[104,101,108,108,111] | implode` → "hello"
- `"a,b,c" | split(",")` → ["a","b","c"]
- `["a","b","c"] | join(",")` → "a,b,c"
- `"abc" | test("b")` → true
- `"abc" | match("(b)")` → match object
- `"abc" | capture("(?P<x>b)")` → {"x":"b"}
- `"abcabc" | scan("b")` → ["b","b"]
- `"hello" | sub("l";"L")` → "heLlo"
- `"hello" | gsub("l";"L")` → "heLLo"
- `" hello " | trim` → "hello"
- `" hello " | ltrim` → "hello "
- `" hello " | rtrim` → " hello"
- `"abc" | splits("")` → "a","b","c"

**Step 2: Implement string builtins**

Functions: join, split, splits, test, match, capture, scan, gsub, sub, startswith, endswith, ltrimstr, rtrimstr, ltrim, rtrim, trim, ascii_downcase, ascii_upcase, explode, implode, tostring, tonumber, ascii

Port from: `builtins/string-builtins.ts` (317 lines)

**Step 3: Run tests**

Run: `cargo test query_engine::builtins::string --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/query_engine/builtins/string.rs src/commands/query_engine/builtins/mod.rs
git commit -m "feat(query-engine): add string builtins"
```

---

## Task 9: Builtin Functions - Object & Array (`builtins/object.rs`, `builtins/array.rs`)

**Files:**
- Create: `src/commands/query_engine/builtins/object.rs`
- Create: `src/commands/query_engine/builtins/array.rs`
- Modify: `src/commands/query_engine/builtins/mod.rs`

**Step 1: Write tests**

Object tests (~15):
- `{"a":1,"b":2} | keys` → ["a","b"]
- `{"a":1,"b":2} | values` → [1,2]
- `{"a":1,"b":2} | length` → 2
- `{"a":1} | has("a")` → true
- `{"a":1} | to_entries` → [{"key":"a","value":1}]
- `[{"key":"a","value":1}] | from_entries` → {"a":1}
- `{"a":1} | with_entries(.value |= . + 1)` → {"a":2}
- `[1,2,3] | reverse` → [3,2,1]
- `[[1,2],[3]] | flatten` → [1,2,3]
- `[1,1,2] | unique` → [1,2]
- `[1,2,3] | add` → 6
- `{"a":1} | tojson` → `"{\"a\":1}"`
- `"{\"a\":1}" | fromjson` → {"a":1}
- `42 | tostring` → "42"
- `"42" | tonumber` → 42

Array tests (~20):
- `[3,1,2] | sort` → [1,2,3]
- `[{"a":2},{"a":1}] | sort_by(.a)` → [{"a":1},{"a":2}]
- `[1,2,3] | bsearch(2)` → 1
- `[1,1,2,2] | unique_by(.)` → [1,2]
- `[{"a":1},{"a":2},{"a":1}] | group_by(.a)` → [[{"a":1},{"a":1}],[{"a":2}]]
- `[1,2,3] | max` → 3
- `[1,2,3] | min` → 1
- `[{"a":3},{"a":1}] | max_by(.a)` → {"a":3}
- `[{"a":3},{"a":1}] | min_by(.a)` → {"a":1}
- `[true, false] | any` → true
- `[true, true] | all` → true
- `[1,2,3] | map(. * 2)` → [2,4,6]
- `{"a":1,"b":2} | map_values(. + 10)` → {"a":11,"b":12}
- `2 | select(. > 1)` → 2
- `1 | select(. > 1)` → (empty)
- `{"a":1} | has("a")` → true
- `"a" | in({"a":1})` → true
- `[1,2,3] | contains([2])` → true
- `[2] | inside([1,2,3])` → true
- `[[1,2],[3,4]] | flatten(1)` → [1,2,3,4]

**Step 2: Implement object builtins**

Functions: keys, keys_unsorted, values, length, utf8bytelength, has, in, to_entries, from_entries, with_entries, reverse, flatten, unique, add, tojson, fromjson, tostring, tonumber, toboolean, tostream, fromstream, truncate_stream

Port from: `builtins/object-builtins.ts` (378 lines)

**Step 3: Implement array builtins**

Functions: sort, sort_by, bsearch, unique_by, group_by, max, max_by, min, min_by, add, any, all, select, map, map_values, has, in, contains, inside, flatten, reverse, unique, indices, index, rindex

Port from: `builtins/array-builtins.ts` (324 lines)

**Step 4: Run tests**

Run: `cargo test query_engine::builtins::object --lib 2>&1 | tail -5`
Run: `cargo test query_engine::builtins::array --lib 2>&1 | tail -5`

**Step 5: Commit**

```bash
git add src/commands/query_engine/builtins/object.rs src/commands/query_engine/builtins/array.rs
git commit -m "feat(query-engine): add object and array builtins"
```

---

## Task 10: Builtin Functions - Control, Navigation, Path, Index, Format, Date, SQL

**Files:**
- Create: `src/commands/query_engine/builtins/control.rs`
- Create: `src/commands/query_engine/builtins/navigation.rs`
- Create: `src/commands/query_engine/builtins/path.rs`
- Create: `src/commands/query_engine/builtins/index.rs`
- Create: `src/commands/query_engine/builtins/format.rs`
- Create: `src/commands/query_engine/builtins/date.rs`
- Create: `src/commands/query_engine/builtins/sql.rs`
- Modify: `src/commands/query_engine/builtins/mod.rs`

**Step 1: Write tests**

Control tests (~10):
- `[1,2,3] | first` → 1
- `[1,2,3] | last` → 3
- `null | range(5)` → 0,1,2,3,4
- `null | range(2;5)` → 2,3,4
- `null | range(0;10;3)` → 0,3,6,9
- `[1,2,3] | limit(2; .[])` → 1,2
- `null | empty` → (empty)
- `[] | isempty` → true (via `isempty(.[])`)
- `0 | until(. >= 5; . + 1)` → 5
- `1 | repeat(. * 2) | select(. > 100) | limit(1; .)` → 128 (via label/break)

Navigation tests (~8):
- `{"a":{"b":1}} | recurse` → multiple values
- `{"a":1} | walk(if type == "number" then . + 1 else . end)` → {"a":2}
- `[[1,2],[3,4]] | transpose` → [[1,3],[2,4]]
- `null | [combinations(2)]` on `[[1,2],[3,4]]` → [[1,3],[1,4],[2,3],[2,4]]

Path tests (~8):
- `{"a":{"b":1}} | getpath(["a","b"])` → 1
- `{"a":1} | setpath(["b"]; 2)` → {"a":1,"b":2}
- `{"a":1,"b":2} | delpaths([["a"]])` → {"b":2}
- `{"a":1} | path(.a)` → ["a"]
- `{"a":{"b":1}} | [paths]` → [["a"],["a","b"]]
- `{"a":{"b":1}} | [leaf_paths]` → [["a","b"]]
- `{"a":1,"b":2} | del(.a)` → {"b":2}
- `{"a":1,"b":2} | pick(.a)` → {"a":1}

Index tests (~5):
- `"abcabc" | index("b")` → 1
- `"abcabc" | rindex("b")` → 4
- `"abcabc" | indices("b")` → [1,4]
- `[1,2,3,2] | index(2)` → 1
- `[1,2,3,2] | rindex(2)` → 3

Format tests (~8):
- `"hello" | @base64` → "aGVsbG8="
- `"aGVsbG8=" | @base64d` → "hello"
- `"hello world" | @uri` → "hello%20world"
- `"hello%20world" | @urid` → "hello world"
- `["a","b"] | @csv` → `"\"a\",\"b\""`
- `["a","b"] | @tsv` → `"a\tb"`
- `{"a":1} | @json` → `"{\"a\":1}"`
- `"<b>" | @html` → "&lt;b&gt;"

Date tests (~5):
- `now` → current timestamp (number)
- `0 | todate` → "1970-01-01T00:00:00Z"
- `"2024-01-01T00:00:00Z" | fromdate` → timestamp
- `0 | strftime("%Y-%m-%d")` → "1970-01-01"
- `0 | gmtime` → time array

SQL tests (~3):
- `2 | IN(1, 2, 3)` → true
- `null | [range(5)] | INDEX(. * 2)` → index object
- `null | JOIN(INDEX([1,2]; .); [1,3]; .)` → joined results

**Step 2: Implement all remaining builtins**

Control: first, last, nth, range, limit, skip, isempty, isvalid, until, while, repeat, empty, error
Navigation: recurse, recurse_down, walk, transpose, combinations, parent, parents, root
Path: getpath, setpath, delpaths, path, paths, leaf_paths, del, pick
Index: index, rindex, indices
Format: @base64, @base64d, @uri, @urid, @csv, @tsv, @json, @text, @html, @sh
Date: now, fromdate, todate, fromdateiso8601, todateiso8601, gmtime, mktime, strftime, strptime
SQL: IN, INDEX, JOIN

Port from:
- `builtins/control-builtins.ts` (286 lines)
- `builtins/navigation-builtins.ts` (219 lines)
- `builtins/path-builtins.ts` (209 lines)
- `builtins/index-builtins.ts` (145 lines)
- `builtins/format-builtins.ts` (134 lines)
- `builtins/date-builtins.ts` (211 lines)
- `builtins/sql-builtins.ts` (118 lines)

**Step 3: Run tests**

Run: `cargo test query_engine::builtins --lib 2>&1 | tail -10`

**Step 4: Commit**

```bash
git add src/commands/query_engine/builtins/
git commit -m "feat(query-engine): add control, navigation, path, index, format, date, sql builtins"
```

---

## Task 11: Builtin Dispatch & Integration

**Files:**
- Modify: `src/commands/query_engine/evaluator.rs`
- Modify: `src/commands/query_engine/builtins/mod.rs`
- Modify: `src/commands/query_engine/mod.rs`

**Step 1: Write integration tests**

Test cases (~15):
- Complex pipe: `[.[] | select(. > 2)]` on `[1,2,3,4]` → `[3,4]`
- Nested function: `[.[] | . * 2 | select(. > 4)]` on `[1,2,3,4]` → `[6,8]`
- Object transform: `[.[] | {name, age}]` on array of objects
- Group and count: `.[] | group_by(.type) | map({type: .[0].type, count: length})`
- Sort and take: `[.[] | .score] | sort | reverse | .[0:3]`
- Reduce with object: `reduce .[] as $x ({}; . + {($x.name): $x.value})`
- User-defined function: `def addtwo: . + 2; [.[] | addtwo]`
- Recursive function: `def fac: if . <= 1 then 1 else . * ((. - 1) | fac) end; 5 | fac` → 120
- String interpolation: `"Name: \(.name), Age: \(.age)"` on object
- Try-catch chain: `try (.a | .b) catch "error"`
- Update nested: `.users[0].name = "new"` on nested object
- Delete and reconstruct: `del(.a) | . + {c: 3}` on `{"a":1,"b":2}`
- Label/break with limit: `first(.[] | select(. > 5))` on `[1,3,7,9]` → 7
- Alternative patterns: `.x as {a: $a} ?// {b: $b} | ...`
- `builtins` returns list of all builtins

**Step 2: Wire up builtin dispatch**

In `evaluator.rs`, implement `eval_builtin()` function that dispatches to all 12 builtin modules in order:
1. Simple math functions (lookup table)
2. Math builtins
3. String builtins
4. Date builtins
5. Format builtins
6. Type builtins
7. Object builtins
8. Array builtins
9. Path builtins
10. Index builtins
11. Control builtins
12. Navigation builtins
13. SQL builtins
14. Special cases (builtins, empty, error, env, debug)
15. User-defined functions

Port from: `evaluator.ts` lines 1375-1728

**Step 3: Run all query_engine tests**

Run: `cargo test query_engine --lib 2>&1 | tail -10`

**Step 4: Commit**

```bash
git add src/commands/query_engine/
git commit -m "feat(query-engine): wire up builtin dispatch and integration tests"
```

---

## Task 12: jq Command (`jq/mod.rs`)

**Files:**
- Create: `src/commands/jq/mod.rs`
- Modify: `src/commands/mod.rs`
- Modify: `src/commands/registry.rs`

**Step 1: Write jq integration tests**

Test cases (~30, port from jq.test.ts + jq.basic.test.ts):
- Basic: `jq '.' '{"a":1}'` → `{"a":1}`
- Field: `jq '.a' '{"a":1}'` → `1`
- Raw output: `jq -r '.name' '{"name":"hello"}'` → `hello` (no quotes)
- Compact: `jq -c '.' '{"a":1}'` → `{"a":1}` (one line)
- Sort keys: `jq -S '.' '{"b":2,"a":1}'` → keys sorted
- Null input: `jq -n '1 + 2'` → `3`
- Slurp: `jq -s '.' with multiple JSON values` → array
- Exit status: `jq -e '.x' '{"x":null}'` → exit code 1
- Join output: `jq -j '.[]' '[1,2,3]'` → `123` (no newlines)
- Tab indent: `jq --tab '.' '{"a":1}'` → tab-indented
- JSON stream: `jq '.' '{"a":1}{"b":2}'` → two outputs
- File input: `jq '.a' file.json` → reads from file
- Multiple files: `jq '.' file1.json file2.json`
- Error handling: `jq '.x.y' '{"x":1}'` → error
- Parse error: `jq 'invalid' '{}'` → parse error exit code 5
- Unknown function: `jq 'foo' '{}'` → exit code 3
- Help: `jq --help` → help text
- Combined flags: `jq -rc '.name' '{"name":"hello"}'`
- Stdin: pipe input via stdin

**Step 2: Implement jq command**

Key components:
- `JqCommand` struct implementing `Command` trait
- `parse_json_stream(input: &str) -> Result<Vec<Value>, String>` - handles concatenated JSON
- `format_value(v: &Value, compact, raw, sort_keys, use_tab, indent) -> String` - output formatting
- CLI argument parsing: -r, -c, -e, -s, -n, -j, -S, --tab, -a, -C, -M
- Input handling: stdin, files, multiple files
- Error handling: parse errors (exit 5), unknown functions (exit 3), file errors (exit 2)

Port from: `jq/jq.ts` (348 lines)

**Step 3: Register in registry**

Add to `src/commands/mod.rs`:
```rust
pub mod jq;
pub mod query_engine;
```

Add to `src/commands/registry.rs` in `register_batch_c()`:
```rust
use super::jq::JqCommand;
registry.register(Box::new(JqCommand));
```

**Step 4: Run tests**

Run: `cargo test jq --lib 2>&1 | tail -10`

**Step 5: Commit**

```bash
git add src/commands/jq/ src/commands/query_engine/ src/commands/mod.rs src/commands/registry.rs
git commit -m "feat(commands): add jq command with JSON processing"
```

---

## Task 13: yq Formats (`yq/formats.rs`)

**Files:**
- Create: `src/commands/yq/formats.rs`

**Step 1: Write format tests**

Test cases (~20):
- Parse YAML: `"name: hello"` → `{"name":"hello"}`
- Parse JSON: `'{"a":1}'` → Value
- Parse XML: `"<root><a>1</a></root>"` → nested object
- Parse INI: `"[section]\nkey=value"` → `{"section":{"key":"value"}}`
- Parse CSV: `"name,age\nAlice,30"` → `[{"name":"Alice","age":30}]`
- Parse TOML: `'[package]\nname = "test"'` → `{"package":{"name":"test"}}`
- Format YAML output
- Format JSON output (compact and pretty)
- Format XML output
- Format INI output
- Format CSV output
- Format TOML output
- Detect format from extension: .yaml, .yml, .json, .xml, .ini, .csv, .tsv, .toml
- Extract YAML front-matter: `---\ntitle: hello\n---\ncontent`
- Extract TOML front-matter: `+++\ntitle = "hello"\n+++\ncontent`
- Multi-document YAML: `---\na: 1\n---\nb: 2`

**Step 2: Implement format handlers**

Key functions:
- `parse_input(input: &str, options: &FormatOptions) -> Result<Value, String>`
- `format_output(value: &Value, options: &FormatOptions) -> String`
- `detect_format_from_extension(filename: &str) -> Option<Format>`
- `extract_front_matter(input: &str) -> Option<FrontMatter>`
- `parse_all_yaml_documents(input: &str) -> Vec<Value>`
- `parse_csv(input: &str, delimiter: &str, has_header: bool) -> Value`
- `format_csv(value: &Value, delimiter: &str) -> String`

Dependencies:
- `serde_json` for JSON
- `serde_yaml` for YAML
- `quick-xml` + serde for XML
- `toml` for TOML
- `csv` for CSV
- Simple INI parser (implement inline, ~50 lines)

Port from: `yq/formats.ts` (319 lines)

**Step 3: Run tests**

Run: `cargo test yq::formats --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/yq/formats.rs
git commit -m "feat(yq): add multi-format parsing and output"
```

---

## Task 14: yq Command (`yq/mod.rs`)

**Files:**
- Create: `src/commands/yq/mod.rs`
- Modify: `src/commands/mod.rs`
- Modify: `src/commands/registry.rs`

**Step 1: Write yq integration tests**

Test cases (~25, port from yq.test.ts):
- Basic YAML: `yq '.name' 'name: hello'` → `hello`
- YAML array: `yq '.[0]' '- a\n- b'` → `a`
- Output JSON: `yq -o json '.' 'name: hello'` → `{"name":"hello"}`
- Output YAML: `yq '.' '{"name":"hello"}'` (input auto-detected as YAML)
- Input JSON: `yq -p json '.a' '{"a":1}'` → `1`
- Input XML: `yq -p xml '.root.a' '<root><a>1</a></root>'` → `1`
- Input INI: `yq -p ini '.section.key' '[section]\nkey=value'` → `value`
- Input CSV: `yq -p csv '.[0].name' 'name,age\nAlice,30'` → `Alice`
- Input TOML: `yq -p toml '.package.name' '[package]\nname = "test"'` → `test`
- Format conversion: YAML→JSON, JSON→YAML, XML→JSON, INI→JSON, CSV→JSON, TOML→JSON
- Raw output: `yq -r '.name' 'name: hello'` → `hello`
- Compact: `yq -o json -c '.' 'name: hello'` → compact JSON
- Null input: `yq -n '{"a":1}'` → `{"a":1}`
- Slurp: `yq -s '.' multi-doc YAML` → array
- Front-matter: `yq -f '.title' '---\ntitle: hello\n---\ncontent'` → `hello`
- In-place: `yq -i '.name = "new"' file.yaml`
- Auto-detect format from extension
- Exit status: `yq -e '.x' 'x: null'` → exit code 1
- Help: `yq --help`
- Combined flags: `yq -o json -rc '.name' 'name: hello'`
- Error handling: parse errors, file not found

**Step 2: Implement yq command**

Key components:
- `YqCommand` struct implementing `Command` trait
- CLI argument parsing: -p, -o, -i, -r, -c, -e, -s, -n, -j, -f, -P, -I, --xml-*, --csv-*
- Format auto-detection from file extension
- Input reading: stdin, file
- Query execution via shared query-engine
- Output formatting via formats module
- In-place file modification
- Front-matter extraction

Port from: `yq/yq.ts` (381 lines)

**Step 3: Register in registry**

Add to `src/commands/mod.rs`:
```rust
pub mod yq;
```

Add to `src/commands/registry.rs` in `register_batch_c()`:
```rust
use super::yq::YqCommand;
registry.register(Box::new(YqCommand));
```

**Step 4: Run tests**

Run: `cargo test yq --lib 2>&1 | tail -10`

**Step 5: Commit**

```bash
git add src/commands/yq/ src/commands/mod.rs src/commands/registry.rs
git commit -m "feat(commands): add yq command with multi-format processing"
```

---

## Task 15: Update Migration Roadmap

**Files:**
- Modify: `docs/plans/migration-roadmap.md`

**Step 1: Update roadmap**

Mark Batch C as complete with stats:
- New files count
- New code lines
- New test count
- Total test count

**Step 2: Run full test suite**

Run: `cargo test 2>&1 | tail -5`
Verify all tests pass.

**Step 3: Commit**

```bash
git add docs/plans/migration-roadmap.md
git commit -m "docs: mark batch C (jq/yq) as completed"
```

---

## Dependencies to Add

Add to `Cargo.toml`:
```toml
serde_json = "1"
serde_yaml = "0.9"
quick-xml = { version = "0.36", features = ["serialize"] }
toml = "0.8"
csv = "1"
indexmap = { version = "2", features = ["serde"] }
base64 = "0.22"
chrono = "0.4"
```

---

## Summary

| Task | Component | Est. Lines | Tests |
|------|-----------|-----------|-------|
| 1 | Value + AST types | ~400 | 0 |
| 2 | Lexer | ~350 | ~20 |
| 3 | Parser | ~750 | ~25 |
| 4 | Operations | ~250 | ~15 |
| 5 | Context | ~200 | 0 |
| 6 | Evaluator | ~1,200 | ~30 |
| 7 | Math + Type builtins | ~300 | ~25 |
| 8 | String builtins | ~350 | ~20 |
| 9 | Object + Array builtins | ~700 | ~35 |
| 10 | Remaining builtins | ~1,000 | ~50 |
| 11 | Dispatch + Integration | ~200 | ~15 |
| 12 | jq command | ~350 | ~30 |
| 13 | yq formats | ~350 | ~20 |
| 14 | yq command | ~400 | ~25 |
| 15 | Roadmap update | ~10 | 0 |
| **Total** | | **~6,800** | **~310** |
