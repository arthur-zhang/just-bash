# AWK Command Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement a complete AWK interpreter in Rust, porting all functionality from the TypeScript reference implementation.

**Architecture:** Three-stage pipeline (Lexer → Parser → Interpreter) with modular interpreter design. The lexer tokenizes AWK source into a token stream, the parser builds an AST via recursive descent, and the interpreter executes the AST with a runtime context managing variables, arrays, fields, and control flow.

**Tech Stack:** Rust, regex_lite, async_trait, Arc<dyn FileSystem>, InMemoryFs for testing

**Reference:** TypeScript source at `/Users/arthur/PycharmProjects/just-bash/src/commands/awk/`

---

## Module Structure

```
src/commands/awk/
├── mod.rs              # AwkCommand entry point + integration tests
├── types.rs            # AST type definitions (enums)
├── lexer.rs            # Tokenizer
├── parser.rs           # Recursive descent parser (incl. print parsing)
├── interpreter.rs      # Main interpreter orchestrator
├── context.rs          # Runtime context (variables, state, control flow)
├── expressions.rs      # Expression evaluator
├── statements.rs       # Statement executor
├── fields.rs           # Field splitting and $N access
├── variables.rs        # Variable/array management
├── coercion.rs         # AWK type coercion rules
└── builtins.rs         # 30+ built-in functions
```

---

## Task 1: AST Types (`types.rs`)

**Files:**
- Create: `src/commands/awk/types.rs`

**Step 1: Create the types module with all AST definitions**

This is a pure data module with no logic. Define all enums/structs for tokens, expressions, statements, patterns, and program structure. Key types:

- `TokenType` enum (40+ variants): Number, String, Regex, Ident, all keywords (Begin/End/If/Else/While/Do/For/In/Break/Continue/Next/NextFile/Exit/Return/Delete/Function/Print/Printf/Getline), arithmetic ops, comparison ops, regex match ops, logical ops, assignment ops, inc/dec, ternary, punctuation, brackets, Dollar/Append/Pipe/Eof
- `Token` struct: token_type, value (String), line, column
- `AwkExpr` enum: NumberLiteral(f64), StringLiteral(String), RegexLiteral(String), FieldRef(Box), Variable(String), ArrayAccess{array,key}, BinaryOp{op,left,right}, UnaryOp{op,operand}, Pre/PostIncrement/Decrement, Ternary{cond,cons,alt}, FunctionCall{name,args}, Assignment{op,target,value}, InExpr{key,array}, Getline{variable?,file?,command?}, Tuple(Vec), Concatenation{left,right}
- `BinaryOp` enum: Add/Sub/Mul/Div/Mod/Pow/Eq/Ne/Lt/Gt/Le/Ge/MatchOp/NotMatchOp/And/Or
- `UnaryOp` enum: Not/Neg/Pos
- `AssignOp` enum: Assign/AddAssign/SubAssign/MulAssign/DivAssign/ModAssign/PowAssign
- `AwkStmt` enum: ExprStmt, Print{args,output?}, Printf{format,args,output?}, If{cond,cons,alt?}, While{cond,body}, DoWhile{body,cond}, For{init?,cond?,update?,body}, ForIn{var,array,body}, Block(Vec), Break, Continue, Next, NextFile, Exit(Option), Return(Option), Delete{target}
- `RedirectInfo` struct with `RedirectType` enum (Write/Append/Pipe)
- `AwkPattern` enum: Begin, End, Expression(AwkExpr), Regex(String), Range{start,end}
- `AwkRule` struct: pattern (Option), action (Vec<AwkStmt>)
- `AwkFunctionDef` struct: name, params, body
- `AwkProgram` struct: functions (Vec), rules (Vec)

Port from: `ast.ts` (293 lines)

**Step 2: Run `cargo check`**

Run: `cargo check 2>&1 | head -20`

**Step 3: Commit**

```bash
git add src/commands/awk/types.rs
git commit -m "feat(awk): add AST types and token definitions"
```

---

## Task 2: Type Coercion (`coercion.rs`)

**Files:**
- Create: `src/commands/awk/coercion.rs`

**Step 1: Write tests for AWK type coercion**

Test cases:
- `to_number("42")` → 42.0
- `to_number("3.14")` → 3.14
- `to_number("123abc")` → 123.0 (leading numeric prefix)
- `to_number("abc")` → 0.0
- `to_number("")` → 0.0
- `to_string(42.0)` → "42"
- `to_string(3.14)` → "3.14"
- `to_string(0.0)` → "0"
- `is_truthy("0")` → false
- `is_truthy("")` → false
- `is_truthy("1")` → true
- `is_truthy("abc")` → true
- `looks_like_number("42")` → true
- `looks_like_number("3.14")` → true
- `looks_like_number(" 42 ")` → true (with whitespace)
- `looks_like_number("abc")` → false
- `looks_like_number("1e5")` → true (scientific notation)

**Step 2: Implement coercion functions**

Key functions:
- `to_number(s: &str) -> f64` — Parse leading numeric prefix, 0.0 for non-numeric
- `to_string(n: f64) -> String` — Integer check: if n == n.floor() && n.is_finite(), format as integer
- `is_truthy(s: &str) -> bool` — "0" and "" are false, everything else true
- `looks_like_number(s: &str) -> bool` — Trim whitespace, check if entire string is valid number
- `compare_values(a: &str, b: &str) -> std::cmp::Ordering` — If both look like numbers, compare numerically; otherwise lexicographic
- `format_output(n: f64, ofmt: &str) -> String` — Format number using OFMT

Port from: `interpreter/type-coercion.ts` (72 lines)

**Step 3: Run tests**

Run: `cargo test awk::coercion --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/awk/coercion.rs
git commit -m "feat(awk): add type coercion utilities"
```

---

## Task 3: Lexer (`lexer.rs`)

**Files:**
- Create: `src/commands/awk/lexer.rs`

**Step 1: Write lexer tests**

Test cases (~20 tests):
- Tokenize simple program: `{ print $1 }` → LBrace, Print, Dollar, Number("1"), RBrace
- Tokenize with string: `{ print "hello" }` → includes String("hello")
- Tokenize regex: `/pattern/ { print }` → Regex("pattern"), LBrace, Print, RBrace
- Tokenize operators: `a + b * c` → Ident, Plus, Ident, Star, Ident
- Tokenize comparison: `$1 == "foo"` → Dollar, Number, Eq, String
- Tokenize assignment ops: `x += 1` → Ident, PlusAssign, Number
- Tokenize increment: `i++` → Ident, Increment
- Tokenize BEGIN/END: `BEGIN { x=0 }` → Begin, LBrace, Ident, Assign, Number, RBrace
- Tokenize keywords: `if`, `else`, `while`, `for`, `in`, `function`, `return`, `delete`, `getline`
- Tokenize regex vs division: `a / b` (division) vs `/pattern/` (regex) — context-sensitive
- Tokenize string escapes: `"hello\tworld\n"` → String with actual tab/newline
- Tokenize hex/octal escapes: `"\x41\101"` → String("AA")
- Tokenize comments: `# comment\n{ print }` → skips comment
- Tokenize line continuation: `a +\n b` → Plus not interrupted
- Tokenize scientific numbers: `1.5e-10` → Number
- Tokenize append operator: `>>` → Append
- Tokenize pipe: `|` → Pipe
- Tokenize ternary: `? :` → Question, Colon
- Tokenize regex match: `~ !~` → Match, NotMatch
- Tokenize `**` as Caret (power alias)

**Step 2: Implement the lexer**

Key implementation details:
- `pub fn tokenize(input: &str) -> Vec<Token>` — main entry point
- Context-sensitive regex detection via `can_be_regex(last_token)` — regex allowed after: None, Newline, Semicolon, LBrace, RBrace, LParen, LBracket, Comma, all assignment ops, And, Or, Not, Match, NotMatch, Question, Colon, comparison ops, arithmetic ops, Print, Printf, If, While, Do, For, Return
- String escape handling: \n, \t, \r, \f, \b, \v, \a (0x07), \\, \", \/, \xHH (hex), \0-\377 (octal)
- POSIX character class expansion in regex: `[[:space:]]` → `[ \t\n\r\f\v]`, etc.
- Number parsing: integer, decimal, scientific notation
- Line continuation: `\` + newline = skip
- Comments: `#` to end of line
- `**` is alias for `^`

Port from: `lexer.ts` (860 lines)

**Step 3: Run tests**

Run: `cargo test awk::lexer --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/awk/lexer.rs
git commit -m "feat(awk): add script lexer/tokenizer"
```

---

## Task 4: Parser (`parser.rs`)

**Files:**
- Create: `src/commands/awk/parser.rs`

**Step 1: Write parser tests**

Test cases (~20 tests):
- Parse simple rule: `{ print $0 }` → one rule, no pattern, print stmt
- Parse BEGIN/END: `BEGIN { x=0 } END { print x }` → begin rule + end rule
- Parse regex pattern: `/foo/ { print }` → regex pattern rule
- Parse expression pattern: `NR > 5 { print }` → expr pattern rule
- Parse range pattern: `/start/,/end/ { print }` → range pattern
- Parse if/else: `{ if ($1 > 0) print "pos"; else print "neg" }`
- Parse while loop: `{ while (i < 10) i++ }`
- Parse for loop: `{ for (i=0; i<10; i++) print i }`
- Parse for-in: `{ for (k in arr) print k }`
- Parse function def: `function add(a, b) { return a + b }`
- Parse assignment operators: `{ x += 1; y *= 2 }`
- Parse ternary: `{ print (x > 0 ? "pos" : "neg") }`
- Parse print with redirect: `{ print "hello" > "file.txt" }`
- Parse print with append: `{ print "hello" >> "file.txt" }`
- Parse print with pipe: `{ print "hello" | "cmd" }`
- Parse printf: `{ printf "%s %d\n", $1, $2 }`
- Parse delete: `{ delete arr[key] }`
- Parse getline variants: `getline`, `getline var`, `getline < "file"`, `"cmd" | getline`
- Parse multi-dim array: `a[1,2]` → concatenation with SUBSEP
- Parse default action: `/pattern/` without braces → default `{ print $0 }`
- Parse nested blocks: `{ if (1) { if (2) { print } } }`
- Parse do-while: `{ do { i++ } while (i < 10) }`

**Step 2: Implement the recursive descent parser**

Key implementation details:

Operator precedence (lowest to highest):
1. Assignment (=, +=, etc.) — right associative
2. Ternary (?:)
3. Pipe getline (expr | getline)
4. Logical OR (||)
5. Logical AND (&&)
6. Array membership (in)
7. Concatenation (implicit — adjacent expressions)
8. Regex match (~, !~)
9. Comparison (<, <=, >, >=, ==, !=)
10. Addition/Subtraction (+, -)
11. Multiplication/Division/Modulo (*, /, %)
12. Unary (!, -, +, prefix ++/--)
13. Exponentiation (^) — RIGHT associative
14. Postfix (++, --)
15. Primary (literals, variables, function calls, field refs)

Tricky parsing cases:
- `$i++` parses as `($i)++` not `$(i++)` — special field index parsing
- Concatenation detection: implicit when two expressions adjacent without operator
- Print context: `>` and `>>` are redirection, not comparison (use ternary lookahead)
- For-in detection: lookahead to distinguish `for (var in array)` from C-style for
- Multi-dimensional arrays: `a[1,2,3]` → concatenation with SUBSEP
- Default action: pattern without `{}` → `{ print $0 }`
- Empty print: `print` alone → `print $0`
- Printf with/without parens: `printf fmt, args` and `printf(fmt, args)`

Port from: `parser2.ts` (1171 lines) + `parser2-print.ts` (428 lines)

**Step 3: Run tests**

Run: `cargo test awk::parser --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/awk/parser.rs
git commit -m "feat(awk): add recursive descent parser"
```

---

## Task 5: Fields Module (`fields.rs`)

**Files:**
- Create: `src/commands/awk/fields.rs`

**Step 1: Write field handling tests**

Test cases (~15 tests):
- Split line with default FS (whitespace): `"  hello  world  "` → ["hello", "world"]
- Split line with custom FS: FS=":" on `"a:b:c"` → ["a", "b", "c"]
- Split line with regex FS: FS=`[,;]` on `"a,b;c"` → ["a", "b", "c"]
- Get field $0 returns entire line
- Get field $1 returns first field
- Get field beyond NF returns empty string
- Get field $-1 returns empty string
- Set field $1 updates field and rebuilds $0 with OFS
- Set field $0 re-splits into fields
- Set field beyond NF extends fields array with empty strings
- Empty line has 0 fields
- Single-char FS splits correctly
- Tab FS: FS="\t" on `"a\tb\tc"` → ["a", "b", "c"]
- Default FS trims leading/trailing whitespace

**Step 2: Implement field functions**

Key functions:
- `split_fields(line: &str, field_sep: &Regex) -> Vec<String>` — Split line into fields
- `get_field(fields: &[String], line: &str, index: i64) -> String` — Get $N
- `set_field(fields: &mut Vec<String>, line: &mut String, index: i64, value: &str, ofs: &str)` — Set $N
- `set_current_line(fields: &mut Vec<String>, nf: &mut usize, line: &mut String, new_line: &str, field_sep: &Regex)` — Update $0 and re-split
- `create_field_sep_regex(fs: &str) -> Regex` — Compile FS into regex (default " " → `\s+`)

Port from: `interpreter/fields.ts` (89 lines)

**Step 3: Run tests**

Run: `cargo test awk::fields --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/awk/fields.rs
git commit -m "feat(awk): add field splitting and access"
```

---

## Task 6: Variables Module (`variables.rs`)

**Files:**
- Create: `src/commands/awk/variables.rs`

**Step 1: Write variable management tests**

Test cases (~12 tests):
- Get uninitialized variable returns ""
- Set and get variable
- Get built-in variable FS returns context FS
- Set FS triggers field separator recompilation
- Set NF truncates fields
- Set NF extends fields with empty strings
- Get/set array element
- Has array element (true/false)
- Delete array element
- Delete entire array
- Array alias resolution (for function params)
- Get ARGV/ENVIRON from special arrays

**Step 2: Implement variable functions**

Key functions:
- `get_variable(ctx: &AwkContext, name: &str) -> String` — Built-in var dispatch + user vars
- `set_variable(ctx: &mut AwkContext, name: &str, value: &str)` — With side effects for FS, NF
- `get_array_element(ctx: &AwkContext, array: &str, key: &str) -> String`
- `set_array_element(ctx: &mut AwkContext, array: &str, key: &str, value: &str)`
- `has_array_element(ctx: &AwkContext, array: &str, key: &str) -> bool`
- `delete_array_element(ctx: &mut AwkContext, array: &str, key: &str)`
- `delete_array(ctx: &mut AwkContext, array: &str)`

Built-in variable getters: FS, OFS, ORS, OFMT, NR, NF, FNR, FILENAME, RSTART, RLENGTH, SUBSEP, ARGC
Built-in variable setters with side effects: FS (recompile regex), NF (truncate/extend fields)

Port from: `interpreter/variables.ts` (199 lines)

**Step 3: Run tests**

Run: `cargo test awk::variables --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/awk/variables.rs
git commit -m "feat(awk): add variable and array management"
```

---

## Task 7: Runtime Context (`context.rs`)

**Files:**
- Create: `src/commands/awk/context.rs`

**Step 1: Write context tests**

Test cases (~8 tests):
- Create default context has correct defaults (FS=" ", OFS=" ", ORS="\n", OFMT="%.6g", SUBSEP="\x1c")
- Create context with custom FS
- Create context with variables
- Context tracks NR, FNR, NF correctly
- Control flow flags default to false
- Execution limits default to 10000 iterations, 100 recursion depth
- Output buffer starts empty
- Reset FNR per file

**Step 2: Implement the runtime context**

```rust
pub struct AwkContext {
    // Built-in variables
    pub fs: String,
    pub ofs: String,
    pub ors: String,
    pub ofmt: String,
    pub nr: usize,
    pub nf: usize,
    pub fnr: usize,
    pub filename: String,
    pub rstart: usize,
    pub rlength: i64,  // -1 when no match
    pub subsep: String,
    pub argc: usize,
    pub argv: HashMap<String, String>,
    pub environ: HashMap<String, String>,

    // Current line state
    pub fields: Vec<String>,
    pub line: String,
    pub field_sep: Regex,

    // User data
    pub vars: HashMap<String, String>,
    pub arrays: HashMap<String, HashMap<String, String>>,
    pub array_aliases: HashMap<String, String>,
    pub functions: HashMap<String, AwkFunctionDef>,

    // Getline support
    pub lines: Option<Vec<String>>,
    pub line_index: Option<usize>,

    // Execution limits
    pub max_iterations: usize,
    pub max_recursion_depth: usize,
    pub current_recursion_depth: usize,

    // Control flow
    pub exit_code: i32,
    pub should_exit: bool,
    pub should_next: bool,
    pub should_next_file: bool,
    pub loop_break: bool,
    pub loop_continue: bool,
    pub return_value: Option<String>,
    pub has_return: bool,
    pub in_end_block: bool,

    // I/O
    pub output: String,
    pub fs_handle: Option<Arc<dyn FileSystem>>,
    pub opened_files: HashSet<String>,
}
```

Port from: `interpreter/context.ts` (158 lines)

**Step 3: Run tests**

Run: `cargo test awk::context --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/awk/context.rs
git commit -m "feat(awk): add runtime context"
```

---

## Task 8: Built-in Functions (`builtins.rs`)

**Files:**
- Create: `src/commands/awk/builtins.rs`

**Step 1: Write built-in function tests**

Test cases (~30 tests):

String functions:
- `length("hello")` → 5
- `length("")` → 0
- `substr("hello", 2)` → "ello"
- `substr("hello", 2, 3)` → "ell"
- `index("hello", "ll")` → 3
- `index("hello", "xyz")` → 0
- `split("a:b:c", arr, ":")` → 3, arr[1]="a", arr[2]="b", arr[3]="c"
- `sub(/o/, "0", s)` where s="foo" → "f0o", returns 1
- `gsub(/o/, "0", s)` where s="foo" → "f00", returns 2
- `match("hello", /ll/)` → 3, RSTART=3, RLENGTH=2
- `match("hello", /xyz/)` → 0, RSTART=0, RLENGTH=-1
- `tolower("HELLO")` → "hello"
- `toupper("hello")` → "HELLO"
- `gensub(/(.)(.)/, "\\2\\1", "g", "abcd")` → "badc"
- `sprintf("%d %s", 42, "hello")` → "42 hello"

Math functions:
- `int(3.9)` → 3
- `int(-3.9)` → -3
- `sqrt(4)` → 2
- `sin(0)` → 0
- `cos(0)` → 1
- `atan2(0, 1)` → 0
- `log(1)` → 0
- `exp(0)` → 1
- `rand()` → [0, 1)
- `srand(42)` → deterministic sequence

Printf format:
- `sprintf("%s", "hello")` → "hello"
- `sprintf("%d", 42)` → "42"
- `sprintf("%05d", 42)` → "00042"
- `sprintf("%-10s", "hi")` → "hi        "
- `sprintf("%c", 65)` → "A"
- `sprintf("%%")` → "%"

**Step 2: Implement all built-in functions**

String functions: length, substr, index, split, sub, gsub, match, gensub, tolower, toupper, sprintf
Math functions: int, sqrt, sin, cos, atan2, log, exp, rand, srand
Stub functions: system (error), close (0), fflush (0)

Key details:
- sub/gsub replacement: `&` = matched text, `\&` = literal &, `\\` = literal \
- gensub replacement: `\0` or `&` = entire match, `\1`-`\9` = capture groups
- sprintf: %s, %d, %i, %f, %e, %E, %g, %G, %x, %X, %o, %c, %% with flags (-, +, space, 0, #) and width/precision (* from args)
- sub/gsub default target is $0; modifying field updates $0, modifying $0 re-splits

Port from: `builtins.ts` (888 lines)

**Step 3: Run tests**

Run: `cargo test awk::builtins --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/awk/builtins.rs
git commit -m "feat(awk): add built-in functions"
```

---

## Task 9: Expression Evaluator (`expressions.rs`)

**Files:**
- Create: `src/commands/awk/expressions.rs`

**Step 1: Write expression evaluation tests**

Test cases (~25 tests):
- Evaluate number literal: `42` → "42"
- Evaluate string literal: `"hello"` → "hello"
- Evaluate regex literal: `/foo/` against $0="foobar" → "1"
- Evaluate field ref: `$1` with fields=["a","b"] → "a"
- Evaluate variable: `x` with vars={x:"42"} → "42"
- Evaluate uninitialized variable → ""
- Evaluate array access: `arr["key"]` → value
- Evaluate binary add: `3 + 4` → "7"
- Evaluate binary string concat: `"hello" " " "world"` → "hello world"
- Evaluate comparison (numeric): `3 < 4` → "1"
- Evaluate comparison (string): `"abc" < "def"` → "1"
- Evaluate comparison (mixed): both look like numbers → numeric compare
- Evaluate logical AND short-circuit: `0 && side_effect` → "0" (no side effect)
- Evaluate logical OR short-circuit: `1 || side_effect` → "1"
- Evaluate regex match: `"hello" ~ /ell/` → "1"
- Evaluate regex not-match: `"hello" !~ /xyz/` → "1"
- Evaluate ternary: `1 ? "yes" : "no"` → "yes"
- Evaluate assignment: `x = 42` → "42", x is now "42"
- Evaluate compound assignment: `x += 5` with x="10" → "15"
- Evaluate pre-increment: `++x` with x="5" → "6"
- Evaluate post-increment: `x++` with x="5" → "5" (x becomes "6")
- Evaluate in-expr: `"key" in arr` → "1" if exists
- Evaluate function call: built-in `length("abc")` → "3"
- Evaluate user function call with params and return
- Evaluate getline (basic)

**Step 2: Implement expression evaluator**

Key function: `pub async fn eval_expr(ctx: &mut AwkContext, expr: &AwkExpr, program: &AwkProgram) -> String`

Implementation by expression type:
- Literals: return value directly
- Regex: match against $0, return "1" or "0"
- FieldRef: evaluate index, call get_field()
- Variable: call get_variable()
- ArrayAccess: evaluate key (with SUBSEP for tuples), call get_array_element()
- BinaryOp: dispatch by operator, handle short-circuit for &&/||
- UnaryOp: !, -, +
- Ternary: short-circuit evaluation
- FunctionCall: check builtins first, then user functions
- Assignment: evaluate value, apply to target (variable, field, array)
- Pre/PostIncrement/Decrement: modify and return appropriately
- InExpr: check array membership
- Getline: read from input/file/command
- Concatenation: evaluate both sides, concatenate strings

User function calls:
- Check recursion depth limit
- Save parameter variables (local scope)
- Set up array aliases for pass-by-reference
- Execute function body
- Restore parameter variables
- Return returnValue or ""

Getline variants:
- Plain `getline` — read from current input, update $0 and NR
- `getline var` — read into variable, update NR
- `getline < file` — read from file (cached), don't update NR
- `cmd | getline` — read from command output (cached), don't update NR
- File/command caching: store content and index in special variables

Port from: `interpreter/expressions.ts` (694 lines)

**Step 3: Run tests**

Run: `cargo test awk::expressions --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/awk/expressions.rs
git commit -m "feat(awk): add expression evaluator"
```

---

## Task 10: Statement Executor (`statements.rs`)

**Files:**
- Create: `src/commands/awk/statements.rs`

**Step 1: Write statement execution tests**

Test cases (~20 tests):
- Execute expression statement
- Execute print with no args → prints $0 + ORS
- Execute print with args → joins with OFS + ORS
- Execute print with redirect to file (>)
- Execute print with append (>>)
- Execute print with pipe (|)
- Execute printf with format string
- Execute if/else (true branch)
- Execute if/else (false branch)
- Execute while loop with iteration limit
- Execute do-while (body runs at least once)
- Execute for loop (C-style)
- Execute for-in loop over array keys
- Execute break exits loop
- Execute continue skips to next iteration
- Execute next sets shouldNext flag
- Execute nextfile sets shouldNextFile flag
- Execute exit sets shouldExit and exit code
- Execute return sets returnValue and hasReturn
- Execute delete array element
- Execute delete entire array
- Execute block of statements (stops on break conditions)

**Step 2: Implement statement executor**

Key function: `pub async fn execute_block(ctx: &mut AwkContext, stmts: &[AwkStmt], program: &AwkProgram)`

Implementation details:
- Execute statements sequentially
- Check `should_break_execution()` after each: shouldExit, shouldNext, shouldNextFile, loopBreak, loopContinue, hasReturn
- Print: evaluate args, format numbers with OFMT (integers print directly), join with OFS, append ORS
- Printf: evaluate format and args, call format_printf()
- File redirection: `>` first write overwrites then appends (tracked in opened_files), `>>` always appends
- Loops: track iteration count against max_iterations, reset loopContinue each iteration
- For-in: iterate over array keys, set loop variable
- Delete: single element or entire array

Port from: `interpreter/statements.ts` (384 lines)

**Step 3: Run tests**

Run: `cargo test awk::statements --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/awk/statements.rs
git commit -m "feat(awk): add statement executor"
```

---

## Task 11: Interpreter Orchestrator (`interpreter.rs`)

**Files:**
- Create: `src/commands/awk/interpreter.rs`

**Step 1: Write interpreter tests**

Test cases (~15 tests):
- Execute BEGIN block before input
- Execute END block after input
- Execute main rules for each line
- Pattern matching: regex pattern
- Pattern matching: expression pattern
- Pattern matching: range pattern (start/end)
- Range pattern: single-line range (start and end match same line)
- Default action (no braces) → print $0
- exit in BEGIN still runs END blocks
- exit in END stops further END blocks
- next skips to next line
- nextfile skips to next file
- NR increments globally, FNR resets per file
- FILENAME set per file
- No main rules and no END → skip file reading

**Step 2: Implement the interpreter**

```rust
pub struct AwkInterpreter {
    ctx: AwkContext,
    program: AwkProgram,
    range_states: Vec<bool>,
}

impl AwkInterpreter {
    pub fn new(ctx: AwkContext, program: AwkProgram) -> Self
    pub async fn execute_begin(&mut self)
    pub async fn execute_line(&mut self, line: &str)
    pub async fn execute_end(&mut self)
    pub fn get_output(&self) -> &str
    pub fn get_exit_code(&self) -> i32
    pub fn get_context(&self) -> &AwkContext
}
```

Key logic:
- `execute_begin()`: run all BEGIN rules
- `execute_line(line)`: set current line, increment NR/FNR, match patterns, execute actions
- `execute_end()`: run all END rules (even after exit), set inEndBlock flag
- Pattern matching: regex → match against $0, expression → evaluate as boolean, range → track state
- Range state: one bool per rule, toggle on start match, toggle off on end match

Port from: `interpreter/interpreter.ts` (192 lines)

**Step 3: Run tests**

Run: `cargo test awk::interpreter --lib 2>&1 | tail -5`

**Step 4: Commit**

```bash
git add src/commands/awk/interpreter.rs
git commit -m "feat(awk): add interpreter orchestrator"
```

---

## Task 12: AwkCommand Entry Point (`mod.rs`)

**Files:**
- Create: `src/commands/awk/mod.rs`
- Modify: `src/commands/mod.rs` — add `pub mod awk;`
- Modify: `src/commands/registry.rs` — register AwkCommand in batch B

**Step 1: Write integration tests**

Test cases (~60 tests covering all major features):

Basic functionality:
- `awk '{ print }' input` → prints all lines
- `awk '{ print $1 }' input` → prints first field
- `awk '{ print $1, $2 }' input` → prints fields with OFS
- `awk -F: '{ print $1 }' input` → custom field separator
- `awk -v x=10 '{ print x }' input` → preset variable
- `awk 'BEGIN { print "start" } { print } END { print "end" }' input`

Patterns:
- `awk '/pattern/ { print }' input` → regex pattern
- `awk 'NR > 2 { print }' input` → expression pattern
- `awk '/start/,/end/ { print }' input` → range pattern
- `awk 'NR == 1' input` → pattern without action (default print)

Operators:
- Arithmetic: `awk '{ print $1 + $2 }'`
- String concatenation: `awk '{ print $1 $2 }'`
- Comparison: `awk '$1 > 10 { print }'`
- Regex match: `awk '$1 ~ /foo/ { print }'`
- Ternary: `awk '{ print ($1 > 0 ? "pos" : "neg") }'`
- Assignment: `awk '{ sum += $1 } END { print sum }'`
- Increment: `awk '{ print NR, $0 }'`

Control flow:
- if/else: `awk '{ if ($1 > 0) print "pos"; else print "neg" }'`
- while: `awk 'BEGIN { i=0; while(i<5) { print i; i++ } }'`
- for: `awk 'BEGIN { for(i=0;i<5;i++) print i }'`
- for-in: `awk '{ a[$1]++ } END { for(k in a) print k, a[k] }'`
- break/continue in loops
- next: `awk '/skip/ { next } { print }'`
- exit: `awk '{ print; exit }'`

Functions:
- User-defined: `awk 'function add(a,b) { return a+b } { print add($1,$2) }'`
- Built-in string: length, substr, index, split, sub, gsub, match, tolower, toupper
- Built-in math: int, sqrt, sin, cos, atan2, log, exp
- sprintf/printf
- rand/srand

Arrays:
- Associative: `awk '{ count[$1]++ } END { for(k in count) print k, count[k] }'`
- delete element
- delete array
- "in" operator: `awk '{ if ("key" in arr) print "found" }'`
- Multi-dimensional: `awk '{ a[1,2] = "x"; print a[1,2] }'`

Fields:
- $0 is entire line
- Modifying $1 rebuilds $0 with OFS
- Modifying $0 re-splits fields
- $NF is last field
- Setting field beyond NF extends with empty strings

Built-in variables:
- NR, NF, FNR, FILENAME
- FS, OFS, ORS
- RSTART, RLENGTH (after match())
- ARGC, ARGV
- SUBSEP

Getline:
- `getline` from stdin
- `getline var` into variable
- `getline < "file"` from file
- `"cmd" | getline` from command (if supported)

Output redirection:
- `print "x" > "file"`
- `print "x" >> "file"`

Edge cases:
- Empty input
- Multiple files
- No program (error)
- Parse error handling
- Execution limit (infinite loop protection)

**Step 2: Implement AwkCommand**

```rust
pub struct AwkCommand;

#[async_trait]
impl Command for AwkCommand {
    fn name(&self) -> &str { "awk" }
    fn usage(&self) -> &str { ... }
    async fn execute(&self, args: Vec<String>, ctx: &ExecutionContext) -> CommandResult { ... }
}
```

Command-line parsing:
- `-F FS` or `-FFS` — set field separator
- `-v VAR=VAL` — set variable before execution
- First non-option argument is the AWK program
- Remaining arguments are input files (or stdin if none)

Execution flow:
1. Parse options (-F, -v)
2. Parse AWK program → AST (return error on parse failure)
3. Create runtime context with FS, variables, limits
4. Set up ARGC/ARGV, ENVIRON
5. Execute BEGIN blocks
6. If exit in BEGIN, still run END
7. If no main rules AND no END, skip file reading
8. Read files (or stdin), process each line
9. Execute END blocks (always)
10. Return output, stderr, exit code

Port from: `awk2.ts` (275 lines)

**Step 3: Register command**

Add to `src/commands/mod.rs`: `pub mod awk;`
Add to `src/commands/registry.rs`: register AwkCommand in `register_batch_b()`

**Step 4: Run all tests**

Run: `cargo test 2>&1 | tail -10`
Expected: All tests pass (previous 1034 + new awk tests)

**Step 5: Commit**

```bash
git add src/commands/awk/ src/commands/mod.rs src/commands/registry.rs
git commit -m "feat(awk): add AwkCommand entry point with integration tests"
```

---

## Task 13: Update Migration Roadmap

**Files:**
- Modify: `docs/plans/migration-roadmap.md`

**Step 1: Update roadmap**

Mark awk as completed with test count and total.

**Step 2: Commit**

```bash
git add docs/plans/migration-roadmap.md
git commit -m "docs: mark awk command as completed in migration roadmap"
```

---

## Dependency Graph

```
Task 1 (types.rs) ─────────────────────────────────────────────┐
    │                                                           │
    ├── Task 2 (coercion.rs) ──────────────────────────────┐    │
    │                                                      │    │
    ├── Task 3 (lexer.rs) ─── Task 4 (parser.rs) ────┐    │    │
    │                                                 │    │    │
    ├── Task 5 (fields.rs) ──────────────────────┐    │    │    │
    │                                            │    │    │    │
    ├── Task 6 (variables.rs) ──────────────┐    │    │    │    │
    │                                       │    │    │    │    │
    └── Task 7 (context.rs) ───────────┐    │    │    │    │    │
                                       │    │    │    │    │    │
    Task 8 (builtins.rs) ─────────┐    │    │    │    │    │    │
                                  │    │    │    │    │    │    │
                                  ▼    ▼    ▼    ▼    ▼    ▼    │
                              Task 9 (expressions.rs)           │
                                       │                        │
                                       ▼                        │
                              Task 10 (statements.rs)           │
                                       │                        │
                                       ▼                        │
                              Task 11 (interpreter.rs)          │
                                       │                        │
                                       ▼                        │
                              Task 12 (mod.rs + registry) ──────┘
                                       │
                                       ▼
                              Task 13 (roadmap update)
```

## Parallelization Opportunities

- **Phase 1 (parallel):** Tasks 2, 3, 5, 6, 7 can all start after Task 1
- **Phase 2 (parallel):** Task 4 (needs 3), Task 8 (needs 2)
- **Phase 3 (sequential):** Tasks 9 → 10 → 11 → 12 → 13

## Estimated Test Counts

| Task | Module | Tests |
|------|--------|-------|
| 1 | types.rs | 0 (pure data) |
| 2 | coercion.rs | ~17 |
| 3 | lexer.rs | ~20 |
| 4 | parser.rs | ~22 |
| 5 | fields.rs | ~15 |
| 6 | variables.rs | ~12 |
| 7 | context.rs | ~8 |
| 8 | builtins.rs | ~30 |
| 9 | expressions.rs | ~25 |
| 10 | statements.rs | ~20 |
| 11 | interpreter.rs | ~15 |
| 12 | mod.rs | ~60 |
| **Total** | | **~244** |
