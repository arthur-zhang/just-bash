# Batch C: jq/yq 命令实现设计

## 概述

Batch C 包含三个主要组件：
1. **query-engine** - 共享查询引擎（~6,086 行 TS → ~5,000 行 Rust）
2. **jq 命令** - JSON 处理（~348 行 TS → ~300 行 Rust）
3. **yq 命令** - 多格式处理（~700 行 TS → ~600 行 Rust）

## 模块结构

```
src/commands/
├── query_engine/           # 共享查询引擎
│   ├── mod.rs             # 导出
│   ├── value.rs           # Value 类型定义
│   ├── lexer.rs           # 词法分析器
│   ├── parser.rs          # 递归下降解析器
│   ├── ast.rs             # AST 节点定义
│   ├── evaluator.rs       # 主解释器
│   ├── context.rs         # 执行上下文
│   ├── operations.rs      # 值/路径操作
│   └── builtins/          # 内置函数
│       ├── mod.rs
│       ├── string.rs      # 字符串函数
│       ├── array.rs       # 数组函数
│       ├── object.rs      # 对象函数
│       ├── math.rs        # 数学函数
│       ├── date.rs        # 日期函数
│       ├── format.rs      # 格式化函数
│       ├── type_fns.rs    # 类型函数
│       ├── control.rs     # 控制流函数
│       ├── navigation.rs  # 导航函数
│       ├── path.rs        # 路径函数
│       ├── index.rs       # 索引函数
│       └── sql.rs         # SQL 函数
├── jq/
│   └── mod.rs             # jq 命令
└── yq/
    ├── mod.rs             # yq 命令
    └── formats.rs         # 格式处理器
```

## Value 类型

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(IndexMap<String, Value>),  // 保持插入顺序
}
```

## AST 节点（23 种类型）

```rust
pub enum AstNode {
    // 导航
    Identity,                                    // .
    Field { name: String, base: Option<Box<AstNode>> },
    Index { base: Box<AstNode>, index: Box<AstNode> },
    Slice { base: Box<AstNode>, start: Option<Box<AstNode>>, end: Option<Box<AstNode>> },
    Iterate { base: Box<AstNode> },              // .[]
    Recurse,                                     // ..

    // 操作符
    Pipe { left: Box<AstNode>, right: Box<AstNode> },
    Comma { left: Box<AstNode>, right: Box<AstNode> },
    BinaryOp { op: BinaryOp, left: Box<AstNode>, right: Box<AstNode> },
    UnaryOp { op: UnaryOp, operand: Box<AstNode> },

    // 字面量与构造
    Literal(Value),
    Array { elements: Option<Box<AstNode>> },
    Object { entries: Vec<ObjectEntry> },
    StringInterp { parts: Vec<StringPart> },

    // 控制流
    Cond { ... },
    Try { body: Box<AstNode>, catch: Option<Box<AstNode>> },
    Optional { base: Box<AstNode> },

    // 函数与变量
    Call { name: String, args: Vec<AstNode> },
    VarRef { name: String },
    VarBind { expr: Box<AstNode>, pattern: DestructurePattern, body: Box<AstNode> },
    Def { name: String, params: Vec<String>, body: Box<AstNode>, rest: Box<AstNode> },

    // 高级
    Reduce { ... },
    Foreach { ... },
    Label { name: String, body: Box<AstNode> },
    Break { name: String },
    UpdateOp { op: UpdateOp, path: Box<AstNode>, value: Box<AstNode> },
}
```

## Token 类型（67 种）

操作符、分隔符、算术、比较、逻辑、赋值、关键字、字面量、特殊。

## Parser 优先级（从低到高）

1. Pipe (|)
2. Comma (,)
3. Variable binding (as)
4. Update operators (=, |=, +=, etc.)
5. Alternative (//)
6. Logical OR (or)
7. Logical AND (and)
8. Comparison (==, !=, <, <=, >, >=)
9. Addition/Subtraction (+, -)
10. Multiplication/Division (*, /, %)
11. Unary (-, not)
12. Postfix (?, .[...], .field)
13. Primary (literals, parens, calls)

## Evaluator

```rust
pub fn evaluate(
    value: &Value,
    ast: &AstNode,
    ctx: &mut EvalContext
) -> Result<Vec<Value>, JqError>
```

- 生成器语义：每个表达式产生 0 到多个输出值
- 惰性求值：builtins 接收 evaluate 函数
- 执行限制：max_iterations=10000, max_depth=2000

## EvalContext

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
```

## Builtins（12 类，~150+ 函数）

| 类别 | 函数数 | 主要函数 |
|------|--------|----------|
| string | ~25 | join, split, test, match, gsub, sub |
| array | ~20 | sort, sort_by, unique, group_by, max, min |
| object | ~20 | keys, values, to_entries, from_entries, has |
| math | ~30 | floor, ceil, sqrt, sin, cos, atan2, pow |
| date | ~10 | now, fromdate, todate, strftime |
| format | ~10 | @base64, @uri, @csv, @tsv, @json |
| type | ~15 | type, arrays, objects, numbers, strings |
| control | ~15 | first, last, nth, range, limit, empty |
| navigation | ~10 | recurse, walk, transpose, parent |
| path | ~10 | getpath, setpath, delpaths, paths |
| index | ~5 | index, rindex, indices |
| sql | ~3 | IN, INDEX, JOIN |

## jq 命令

```rust
struct JqOptions {
    raw_output: bool,      // -r
    compact: bool,         // -c
    sort_keys: bool,       // -S
    tab: bool,             // --tab
    slurp: bool,           // -s
    null_input: bool,      // -n
    exit_status: bool,     // -e
    join_output: bool,     // -j
}
```

## yq 命令

```rust
pub enum Format {
    Yaml, Json, Xml, Ini, Csv, Toml,
}

struct YqOptions {
    input_format: Option<Format>,
    output_format: Option<Format>,
    in_place: bool,
    raw_output: bool,
    compact: bool,
    slurp: bool,
    null_input: bool,
    front_matter: bool,
    indent: usize,
    xml_attribute_prefix: String,
    xml_content_name: String,
    csv_delimiter: Option<char>,
    csv_header: bool,
}
```

## 依赖 Crates

- `serde_json` - JSON
- `serde_yaml` - YAML
- `quick-xml` + `serde` - XML
- `toml` - TOML
- `csv` - CSV
- `indexmap` - 有序 Map
- `base64` - Base64 编解码
- `chrono` - 日期时间

## 测试计划

- jq: 175 个测试
- yq: 191 个测试
- query-engine: 内部单元测试
- 总计: ~400+ 测试
