# 第一阶段详细迁移计划：核心基础设施

## 概述

第一阶段的目标是建立 Rust 版本的核心运行时基础设施，使解释器能够实际执行脚本。

**预计新增代码**: ~3,000-4,000 行 Rust 代码

---

## 1. 文件系统模块 (filesystem/)

### 1.1 目标结构

```
src/
├── fs/
│   ├── mod.rs              # 模块导出
│   ├── types.rs            # 类型定义 (FsStat, FsEntry, etc.)
│   ├── encoding.rs         # 编码转换 (utf8, base64, hex, etc.)
│   ├── in_memory_fs.rs     # 内存文件系统实现
│   └── overlay_fs.rs       # 覆盖文件系统 (可选，第二优先级)
```

### 1.2 核心类型 (types.rs)

从 TypeScript `interface.ts` 迁移:

```rust
/// 支持的编码类型
pub enum BufferEncoding {
    Utf8,
    Ascii,
    Binary,
    Base64,
    Hex,
    Latin1,
}

/// 文件内容类型
pub enum FileContent {
    Text(String),
    Binary(Vec<u8>),
}

/// 文件系统条目类型
pub enum FsEntry {
    File {
        content: Vec<u8>,
        mode: u32,
        mtime: SystemTime,
    },
    Directory {
        mode: u32,
        mtime: SystemTime,
    },
    Symlink {
        target: String,
        mode: u32,
        mtime: SystemTime,
    },
}

/// 文件状态信息
pub struct FsStat {
    pub is_file: bool,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub mode: u32,
    pub size: u64,
    pub mtime: SystemTime,
}

/// 目录条目 (类似 Node.js Dirent)
pub struct DirentEntry {
    pub name: String,
    pub is_file: bool,
    pub is_directory: bool,
    pub is_symlink: bool,
}

/// mkdir 选项
pub struct MkdirOptions {
    pub recursive: bool,
}

/// rm 选项
pub struct RmOptions {
    pub recursive: bool,
    pub force: bool,
}

/// cp 选项
pub struct CpOptions {
    pub recursive: bool,
}
```

### 1.3 FileSystem Trait

扩展现有的 `interpreter/interpreter.rs` 中的 `FileSystem` trait:

```rust
/// 文件系统接口
pub trait FileSystem: Send + Sync {
    // 读取操作
    fn read_file(&self, path: &str) -> Result<String, FsError>;
    fn read_file_buffer(&self, path: &str) -> Result<Vec<u8>, FsError>;

    // 写入操作
    fn write_file(&self, path: &str, content: &[u8]) -> Result<(), FsError>;
    fn append_file(&self, path: &str, content: &[u8]) -> Result<(), FsError>;

    // 目录操作
    fn mkdir(&self, path: &str, options: &MkdirOptions) -> Result<(), FsError>;
    fn readdir(&self, path: &str) -> Result<Vec<String>, FsError>;
    fn readdir_with_file_types(&self, path: &str) -> Result<Vec<DirentEntry>, FsError>;

    // 删除操作
    fn rm(&self, path: &str, options: &RmOptions) -> Result<(), FsError>;

    // 复制/移动
    fn cp(&self, src: &str, dest: &str, options: &CpOptions) -> Result<(), FsError>;
    fn mv(&self, src: &str, dest: &str) -> Result<(), FsError>;

    // 元数据操作
    fn exists(&self, path: &str) -> bool;
    fn stat(&self, path: &str) -> Result<FsStat, FsError>;
    fn lstat(&self, path: &str) -> Result<FsStat, FsError>;
    fn chmod(&self, path: &str, mode: u32) -> Result<(), FsError>;
    fn utimes(&self, path: &str, mtime: SystemTime) -> Result<(), FsError>;

    // 符号链接
    fn symlink(&self, target: &str, link_path: &str) -> Result<(), FsError>;
    fn link(&self, existing: &str, new: &str) -> Result<(), FsError>;
    fn readlink(&self, path: &str) -> Result<String, FsError>;
    fn realpath(&self, path: &str) -> Result<String, FsError>;

    // 路径操作
    fn resolve_path(&self, base: &str, path: &str) -> String;
    fn get_all_paths(&self) -> Vec<String>;
}
```

### 1.4 InMemoryFs 实现 (in_memory_fs.rs)

从 TypeScript `in-memory-fs.ts` (685 行) 迁移:

**核心数据结构**:
```rust
pub struct InMemoryFs {
    data: HashMap<String, FsEntry>,
}
```

**关键方法**:
- `normalize_path()` - 路径规范化
- `dirname()` - 获取父目录
- `ensure_parent_dirs()` - 确保父目录存在
- `resolve_path_with_symlinks()` - 解析符号链接
- `resolve_intermediate_symlinks()` - 解析中间符号链接 (用于 lstat)

**预计代码量**: ~600-800 行

### 1.5 编码工具 (encoding.rs)

从 TypeScript `encoding.ts` (81 行) 迁移:

```rust
/// 将内容转换为字节数组
pub fn to_buffer(content: &FileContent, encoding: BufferEncoding) -> Vec<u8>;

/// 将字节数组转换为字符串
pub fn from_buffer(buffer: &[u8], encoding: BufferEncoding) -> String;
```

**预计代码量**: ~100 行

---

## 2. Bash 主类 (bash.rs)

### 2.1 目标结构

```
src/
├── bash.rs                 # 主 Bash 环境类
├── lib.rs                  # 更新导出
```

### 2.2 核心结构

从 TypeScript `Bash.ts` (~600 行) 迁移:

```rust
/// Bash 执行选项
pub struct BashOptions {
    /// 初始文件
    pub files: Option<HashMap<String, FileContent>>,
    /// 环境变量
    pub env: Option<HashMap<String, String>>,
    /// 工作目录
    pub cwd: Option<String>,
    /// 文件系统实例
    pub fs: Option<Box<dyn FileSystem>>,
    /// 执行限制
    pub limits: Option<ExecutionLimits>,
    /// 自定义命令
    pub custom_commands: Option<Vec<Command>>,
}

/// 执行选项
pub struct ExecOptions {
    /// 临时环境变量
    pub env: Option<HashMap<String, String>>,
    /// 临时工作目录
    pub cwd: Option<String>,
    /// 是否保留原始脚本格式
    pub raw_script: bool,
}

/// Bash 环境
pub struct Bash {
    fs: Box<dyn FileSystem>,
    commands: HashMap<String, Command>,
    limits: ExecutionLimits,
    state: InterpreterState,
}

impl Bash {
    /// 创建新的 Bash 环境
    pub fn new(options: BashOptions) -> Self;

    /// 执行脚本
    pub fn exec(&mut self, script: &str, options: Option<ExecOptions>) -> ExecResult;

    /// 注册命令
    pub fn register_command(&mut self, command: Command);

    /// 读取文件
    pub fn read_file(&self, path: &str) -> Result<String, FsError>;

    /// 写入文件
    pub fn write_file(&self, path: &str, content: &str) -> Result<(), FsError>;

    /// 获取当前目录
    pub fn get_cwd(&self) -> &str;

    /// 获取环境变量
    pub fn get_env(&self) -> &HashMap<String, String>;
}
```

### 2.3 执行流程

```
exec(script)
  → parse(script)           // 使用现有 parser
  → execute_script(ast)     // 使用现有 interpreter
  → 返回 ExecResult
```

### 2.4 脚本规范化

从 TypeScript `normalizeScript()` 函数迁移:
- 处理缩进的多行脚本
- 保留 heredoc 内容中的空白

**预计代码量**: ~400-500 行

---

## 3. 命令注册系统

### 3.1 目标结构

```
src/
├── commands/
│   ├── mod.rs              # 模块导出和命令注册
│   ├── types.rs            # Command trait 和类型
│   └── registry.rs         # 命令注册表
```

### 3.2 Command Trait

```rust
/// 命令执行结果
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// 命令上下文
pub struct CommandContext<'a> {
    pub args: &'a [String],
    pub env: &'a HashMap<String, String>,
    pub cwd: &'a str,
    pub stdin: &'a str,
    pub fs: &'a dyn FileSystem,
}

/// 命令 trait
pub trait Command: Send + Sync {
    fn name(&self) -> &str;
    fn execute(&self, ctx: &CommandContext) -> CommandResult;
}

/// 命令注册表
pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}
```

**预计代码量**: ~200 行

---

## 4. 整合现有模块

### 4.1 需要连接的模块

现有 Rust 代码中已实现但需要整合的模块:

1. **parser/** - 已完成，需要在 Bash::exec() 中调用
2. **interpreter/types.rs** - InterpreterState 需要与 Bash 类整合
3. **interpreter/builtins/** - 28 个内置命令需要注册到 CommandRegistry
4. **interpreter/expansion/** - 需要在执行时调用
5. **interpreter/control_flow.rs** - 需要在执行时调用

### 4.2 执行器整合

需要实现一个完整的执行器，将各模块串联:

```rust
impl Bash {
    fn execute_script(&mut self, ast: &ScriptNode) -> ExecResult {
        // 遍历语句
        for statement in &ast.statements {
            self.execute_statement(statement)?;
        }
        // ...
    }

    fn execute_statement(&mut self, stmt: &StatementNode) -> ExecResult {
        // 处理管道
        // 处理 && 和 ||
        // ...
    }

    fn execute_command(&mut self, cmd: &CommandNode) -> ExecResult {
        match cmd {
            CommandNode::Simple(simple) => self.execute_simple_command(simple),
            CommandNode::Compound(compound) => self.execute_compound_command(compound),
            CommandNode::FunctionDef(func) => self.define_function(func),
        }
    }
}
```

---

## 5. 实现顺序

### 步骤 1: 文件系统类型和 trait (~1 天)
1. 创建 `src/fs/mod.rs`
2. 创建 `src/fs/types.rs` - 定义所有类型
3. 扩展 `FileSystem` trait

### 步骤 2: InMemoryFs 实现 (~2 天)
1. 创建 `src/fs/in_memory_fs.rs`
2. 实现所有 FileSystem trait 方法
3. 添加单元测试

### 步骤 3: 命令系统 (~1 天)
1. 创建 `src/commands/mod.rs`
2. 创建 `src/commands/types.rs`
3. 实现 CommandRegistry

### 步骤 4: Bash 主类 (~2 天)
1. 创建 `src/bash.rs`
2. 实现构造函数和初始化
3. 实现 exec() 方法
4. 整合 parser 和 interpreter

### 步骤 5: 整合测试 (~1 天)
1. 创建集成测试
2. 验证基本脚本执行
3. 修复问题

---

## 6. 依赖关系

```
                    ┌─────────────┐
                    │   Bash      │
                    │  (主入口)   │
                    └──────┬──────┘
                           │
           ┌───────────────┼───────────────┐
           │               │               │
           ▼               ▼               ▼
    ┌──────────┐    ┌──────────┐    ┌──────────┐
    │  Parser  │    │Interpreter│   │FileSystem│
    │ (已完成) │    │ (已完成)  │   │ (待实现) │
    └──────────┘    └──────────┘    └──────────┘
                           │
                           ▼
                    ┌──────────┐
                    │ Commands │
                    │ (待实现) │
                    └──────────┘
```

---

## 7. 测试计划

### 7.1 单元测试

- `fs/in_memory_fs.rs` - 文件操作测试
- `bash.rs` - 基本执行测试

### 7.2 集成测试

```rust
#[test]
fn test_basic_echo() {
    let mut bash = Bash::new(BashOptions::default());
    let result = bash.exec("echo hello");
    assert_eq!(result.stdout, "hello\n");
    assert_eq!(result.exit_code, 0);
}

#[test]
fn test_variable_expansion() {
    let mut bash = Bash::new(BashOptions::default());
    bash.exec("x=world");
    let result = bash.exec("echo hello $x");
    assert_eq!(result.stdout, "hello world\n");
}

#[test]
fn test_file_operations() {
    let mut bash = Bash::new(BashOptions::default());
    bash.exec("echo content > /tmp/test.txt");
    let result = bash.exec("cat /tmp/test.txt");
    assert_eq!(result.stdout, "content\n");
}
```

---

## 8. 风险和注意事项

1. **异步 vs 同步**: TypeScript 版本使用 async/await，Rust 版本需要决定是否使用 async
   - 建议: 先实现同步版本，后续可添加 async 支持

2. **错误处理**: 需要统一的错误类型
   - 建议: 创建 `FsError` 枚举，包含所有可能的文件系统错误

3. **符号链接循环检测**: 需要防止无限循环
   - 建议: 使用 `HashSet` 跟踪已访问路径，限制最大深度 (40)

4. **路径规范化**: 需要处理 `.`, `..`, 多余斜杠等
   - 建议: 参考 TypeScript 实现的 `normalizePath()` 函数

---

## 9. 完成标准

第一阶段完成的标志:

- [x] `InMemoryFs` 通过所有单元测试 ✅
- [x] `Bash::exec()` 能执行简单脚本 ✅
- [x] 变量赋值和展开正常工作 ✅
- [x] 基本控制流 (if/for/while) 正常工作 ✅
- [x] 内置命令 (echo, cd, pwd) 正常工作 ✅
- [ ] 文件重定向正常工作 (待实现)

---

## 10. 实现总结 (2026-02-06 完成)

### 10.1 新增文件

| 文件 | 行数 | 说明 |
|------|------|------|
| `src/fs/mod.rs` | ~10 | 模块导出 |
| `src/fs/types.rs` | ~320 | FileSystem trait, FsError, 编码工具 |
| `src/fs/in_memory_fs.rs` | ~660 | InMemoryFs 完整实现 |
| `src/bash.rs` | ~430 | Bash 主类 |
| `src/interpreter/execution_engine.rs` | ~750 | 核心执行引擎 |
| `src/interpreter/sync_fs_adapter.rs` | ~240 | Async/Sync 适配器 |
| **总计** | **~2,410** | |

### 10.2 修改文件

| 文件 | 修改内容 |
|------|----------|
| `src/lib.rs` | 添加 fs, bash 模块导出 |
| `src/interpreter/mod.rs` | 添加 execution_engine, sync_fs_adapter 导出 |
| `src/interpreter/subshell_group.rs` | 修改闭包签名，传递 state 参数 |
| `Cargo.toml` | 添加 tokio rt-multi-thread feature |

### 10.3 架构决策

1. **Async FileSystem + Sync 执行引擎**
   - FileSystem trait 使用 `#[async_trait]`
   - 执行引擎保持 sync（复用现有 interpreter 代码）
   - 使用 `SyncFsAdapter` + `block_in_place` 桥接

2. **执行流程**
   ```
   Bash::exec(script)
     → parse(script)
     → block_in_place {
         SyncFsAdapter::new(fs, handle)
         ExecutionEngine::execute_script(state, ast)
       }
     → ExecResult
   ```

3. **ExecutionEngine 结构**
   ```
   execute_script(ScriptNode)
     → execute_statement(StatementNode)  // &&, ||, ;
       → execute_pipeline_node(PipelineNode)  // |, !
         → execute_command(CommandNode)
           → execute_simple_command / execute_compound_command
   ```

### 10.4 测试覆盖

| 测试类型 | 数量 | 说明 |
|----------|------|------|
| InMemoryFs 单元测试 | 11 | 文件操作、符号链接、stat 等 |
| SyncFsAdapter 测试 | 7 | 读写、目录、glob 等 |
| ExecutionEngine 测试 | 10 | echo, if, for, while, subshell 等 |
| Bash 集成测试 | 15 | 完整执行流程测试 |
| **新增总计** | **43** | |
| **项目总计** | **760** | 全部通过 |

### 10.5 已实现功能

**命令**:
- `echo` - 基础输出
- `cd` - 切换目录
- `pwd` - 显示当前目录
- `exit` - 退出脚本
- `export` - 导出变量
- `true` / `false` / `:` - 返回状态

**控制流**:
- `if/then/elif/else/fi`
- `for ... in ... do ... done`
- `while ... do ... done`
- `until ... do ... done`
- `(...)` subshell
- `{ ...; }` group

**操作符**:
- `&&` - 逻辑与
- `||` - 逻辑或
- `;` - 顺序执行

**展开**:
- `$VAR` - 变量展开
- glob 展开（通过 expand_word_with_glob）

### 10.6 待实现功能 (Phase 2+)

- `case` 语句完整实现
- `[[ ... ]]` 条件命令
- C-style for 循环 `for ((i=0; i<10; i++))`
- 命令替换 `$(...)`
- 重定向 `>`, `>>`, `<`, `2>`
- 管道 `|`
- 更多 builtin 命令
- 70+ Unix 命令 (cat, grep, sed, awk 等)
