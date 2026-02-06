# just-bash TypeScript to Rust 迁移路线图

## 迁移状态总结

### 已完成迁移 (Rust 项目中已有)
- **parser/** - 完成 (11 文件, 8,105 行)
- **interpreter/** - 完成 (90 文件, ~38,000 行)
  - builtins/ - 28 个内置命令
  - expansion/ - 参数展开
  - helpers/ - 辅助工具
  - **execution_engine.rs** - ✅ 新增：核心执行引擎
  - **sync_fs_adapter.rs** - ✅ 新增：Async/Sync 文件系统适配器
- **ast/** - 完成 (2 文件, 1,161 行)
- **fs/** - ✅ 完成 (3 文件, ~1,200 行)
  - types.rs - FileSystem trait 和类型定义
  - in_memory_fs.rs - 内存文件系统实现
- **bash.rs** - ✅ 完成 (~400 行) - 主环境类

### 尚未迁移的模块

| 模块 | 文件数 | 代码行数 | 优先级 | 说明 |
|------|--------|----------|--------|------|
| ~~**filesystem/**~~ | ~~18~~ | ~~7,550~~ | ✅ 完成 | 虚拟文件系统抽象层 |
| **commands/** | 364 | 97,146 | 🔴 高 | 70+ Unix 命令实现 |
| ~~**Bash.ts**~~ | ~~1~~ | ~~~20,000~~ | ✅ 完成 | 主环境类，整合所有模块 |
| **shell/** (glob) | 2 | 1,387 | 🟡 中 | glob 模式匹配 |
| **network/** | 9 | 2,629 | 🟡 中 | 安全网络访问 |
| **sandbox/** | 4 | 642 | 🟢 低 | Vercel 兼容 API |
| **cli/** | 6 | 1,367 | 🟢 低 | 命令行接口 |
| **测试代码** | ~70 | ~16,000 | 🟢 低 | 各类测试 |

---

## 迁移阶段规划

### 第一阶段：核心基础设施 ✅ 完成

#### 1.1 filesystem/ - 虚拟文件系统 ✅
**状态**: 已完成

实现的文件:
- `src/fs/mod.rs` - 模块导出
- `src/fs/types.rs` - FileSystem trait, FsError, FsStat, 编码工具
- `src/fs/in_memory_fs.rs` - InMemoryFs 完整实现（符号链接、权限、目录操作等）

#### 1.2 Bash.ts - 主环境类 ✅
**状态**: 已完成

实现的文件:
- `src/bash.rs` - Bash 主类
- `src/interpreter/execution_engine.rs` - 执行引擎
- `src/interpreter/sync_fs_adapter.rs` - Async/Sync 适配器

已实现功能:
- ✅ `Bash::new()` - 创建环境，初始化文件系统
- ✅ `Bash::exec()` - 执行脚本，返回 ExecResult
- ✅ 变量展开 (`$VAR`)
- ✅ 控制流 (if/for/while/until)
- ✅ 基本命令 (echo, cd, pwd, exit, export, true, false)
- ✅ 逻辑操作符 (`&&`, `||`)
- ✅ subshell `(...)` 和 group `{ ...; }`

---

### 第二阶段：命令实现 (待开始)

#### 批次 A - 基础命令 (最常用)
| 命令 | 行数 | 状态 | 说明 |
|------|------|------|------|
| echo | ~100 | ✅ 基础实现 | 输出文本 |
| cat | ~200 | ⏳ 待实现 | 连接文件 |
| head | ~150 | ⏳ 待实现 | 显示开头 |
| tail | ~200 | ⏳ 待实现 | 显示结尾 |
| wc | ~150 | ⏳ 待实现 | 字数统计 |
| ls | ~400 | ⏳ 待实现 | 列出目录 |
| mkdir | ~100 | ⏳ 待实现 | 创建目录 |
| rm | ~150 | ⏳ 待实现 | 删除文件 |
| cp | ~250 | ⏳ 待实现 | 复制文件 |
| mv | ~150 | ⏳ 待实现 | 移动文件 |
| touch | ~100 | ⏳ 待实现 | 更新时间戳 |
| pwd | ~50 | ✅ 已实现 | 当前目录 |
| basename | ~80 | ⏳ 待实现 | 提取文件名 |
| dirname | ~80 | ⏳ 待实现 | 提取目录名 |
| grep | ~675 | ⏳ 待实现 | 模式匹配 |
| test | ~200 | ⏳ 待实现 | 条件测试 |
| true/false | ~20 | ✅ 已实现 | 返回状态 |

#### 批次 B - 文本处理
| 命令 | 行数 | 说明 |
|------|------|------|
| sed | ~569 | 流编辑器 |
| awk | ~2000+ | AWK 解释器 |
| cut | ~200 | 切割字段 |
| sort | ~300 | 排序 |
| uniq | ~150 | 去重 |
| tr | ~200 | 字符转换 |
| paste | ~100 | 合并行 |
| join | ~200 | 连接行 |
| nl | ~100 | 行号 |

#### 批次 C - 数据格式
| 命令 | 行数 | 说明 |
|------|------|------|
| jq | ~348 | JSON 处理 |
| yq | ~500+ | YAML/TOML/XML |

#### 批次 D - 其他
| 命令 | 行数 | 说明 |
|------|------|------|
| find | ~400 | 查找文件 |
| xargs | ~200 | 构建命令 |
| diff | ~300 | 比较文件 |
| tar | ~400 | 归档 |
| gzip | ~200 | 压缩 |
| base64 | ~100 | 编码 |
| curl | ~300 | HTTP 客户端 |

---

### 第三阶段：辅助模块

#### 3.1 shell/glob - Glob 模式匹配
- `glob.ts` (1,043 行)
- `glob-to-regex.ts` (344 行)

#### 3.2 network/ - 网络访问控制
- URL 白名单验证
- 安全 fetch 包装
- 重定向处理

---

### 第四阶段：接口层

#### 4.1 cli/ - 命令行接口
- 脚本执行
- 交互式 shell
- 安全选项

#### 4.2 sandbox/ - Vercel 兼容 API
- `runCommand()`
- `writeFiles()`
- `readFile()`

---

## 当前进度

- [x] 第一阶段规划
- [x] 第一阶段实现 ✅ **完成于 2026-02-06**
  - [x] filesystem/ - InMemoryFs 完整实现
  - [x] Bash.ts 主类 - 执行引擎接入
  - [x] 760 个测试全部通过
- [ ] 第二阶段 - 命令实现
- [ ] 第三阶段 - 辅助模块
- [ ] 第四阶段 - 接口层

---

## 测试统计

| 模块 | 测试数量 | 状态 |
|------|----------|------|
| parser | 11 | ✅ |
| interpreter | 725 | ✅ |
| fs | 11 | ✅ |
| bash | 15 | ✅ |
| sync_fs_adapter | 7 | ✅ |
| execution_engine | 10 | ✅ |
| **总计** | **760** | ✅ |
