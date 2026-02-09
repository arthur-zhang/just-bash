# just-bash

Rust 实现的安全沙箱化 Bash 解释器，专为 AI 代理和自动化脚本执行设计。

[English](README.md)

## 概述

just-bash 是一个用 Rust 实现的完整 Bash 解析器、解释器和执行环境。它具备虚拟内存文件系统、70+ Unix 命令实现、网络安全控制以及 Vercel 兼容的 Sandbox API。

## 特性

- **完整的 Bash 解析器** — 词法分析器和递归下降解析器
- **执行引擎** — 变量展开、流程控制、管道、重定向、子 shell
- **虚拟文件系统** — 内存文件系统，支持目录、文件、符号链接和权限
- **70+ Unix 命令** — cat、grep、sed、awk、jq、yq、curl、find、tar 等
- **jq 兼容查询引擎** — 150+ 内置函数用于 JSON 处理
- **网络安全** — URL 白名单控制网络访问
- **Sandbox API** — Vercel 兼容接口，支持编程式执行
- **CLI 工具** — 命令行运行 bash 脚本，支持 JSON 输出

## 快速开始

```bash
# 构建
cargo build --release

# 运行脚本
./target/release/just-bash -c 'echo "Hello, World!"'

# 从文件运行
./target/release/just-bash script.sh

# JSON 输出模式
./target/release/just-bash -c 'echo hello' --json
```

## CLI 选项

| 选项 | 说明 |
|------|------|
| `-c <script>` | 从参数执行脚本 |
| `<file>` | 从文件执行脚本 |
| `--json` | 以 JSON 格式输出（stdout、stderr、exitCode） |
| `--cwd <path>` | 设置工作目录 |
| `-e, --errexit` | 遇到错误立即退出 |

## 作为库使用

```rust
use just_bash::bash::{Bash, BashOptions};

#[tokio::main]
async fn main() {
    let mut bash = Bash::new(BashOptions::default()).await;
    let result = bash.exec("echo 'Hello'", None).await;
    println!("{}", result.stdout);
}
```

## 使用 Sandbox API

```rust
use just_bash::sandbox::{Sandbox, SandboxOptions};

#[tokio::main]
async fn main() {
    let mut sandbox = Sandbox::create(None).await;
    let result = sandbox.run_command("echo 'test'", None).await;
    println!("Exit code: {}", result.exit_code);
}
```

## 支持的命令

| 类别 | 命令 |
|------|------|
| 文件操作 | `cat`, `head`, `tail`, `wc`, `ls`, `mkdir`, `rm`, `cp`, `mv`, `touch`, `basename`, `dirname`, `grep`, `test`/`[` |
| 文本处理 | `uniq`, `cut`, `nl`, `tr`, `paste`, `join`, `sort`, `sed`, `awk` |
| 数据处理 | `jq`, `yq` |
| 高级工具 | `base64`, `diff`, `gzip`/`gunzip`/`zcat`, `find`, `tar`, `xargs`, `curl` |
| 内置命令 | `cd`, `echo`, `printf`, `read`, `export`, `declare`, `local`, `set`, `eval`, `source` 等 |

## 许可证

MIT
