# JSONL Viewer（jsonl-v）开发规格说明书

> 本文档是**自包含**的：交给任何一台机器上的 Kimi Code（或其他 AI 编码助手），无需额外背景即可开工。
> 目标产物：一个 Windows 优先的桌面 GUI 程序，用来浏览 JSONL 文件，并对 Kimi Code 的 `wire.jsonl` 会话日志做增强展示。
> 唯一执行动作（本机）：把本文档保存为 `C:\data\gitrepo\gitee\jsonl-v\docs\jsonl-viewer-spec.md`，供另一台电脑使用。**不在本机开始编码。**

---

## 第一部分：背景 —— wire.jsonl 是什么

`wire.jsonl` 是 Kimi Code 运行时的事件日志（journal），位于会话目录下（`C:/Users/<user>/.kimi-code/sessions/<workspace>/<session>/agents/main/wire.jsonl`）。它记录的是运行时内部发生的**事件流**，而不是「每次 HTTP 请求的原始报文转储」。

### 核心结论：请求是怎么发的

**不是每行发一次，也不是固定的几行合起来发一次。** 规则是：

1. 每个 `"type":"llm.request"` 行 = **一次真实发往 LLM 的请求**。该行只含元信息（模型、messageCount、systemPromptHash、toolsHash、turnStep），**不含请求体**。
2. 每次请求都是**全量上下文重发**：system prompt + 截至当时累积的全部 messages。样例文件里 `messageCount` 从 2 单调增长到 33（14 次请求），印证了这一点。
3. 第 N 次请求的实际内容 = `profile.bind`（system prompt）+ `llm.tools_snapshot`（工具 schema）+ 第 N 次 `llm.request` 之前累积的 `context.append_message` / `context.append_loop_event` 事件。要「看某次请求发了什么」，需要从这些事件**重建**。
4. 相邻两个 `llm.request` 之间的行，是上一轮 LLM 响应（think/text 流式片段、tool.call）和工具执行结果（tool.result）的记录。

### 样例文件的实测数据（200 行 / 216KB，5 个用户 turn）

14 次 LLM 请求一览（行号 / turnStep / messageCount）：

| 行号 | turnStep | messageCount | 说明 |
|---|---|---|---|
| 14 | 0.1 | 2 | 第 0 轮第 1 步：system 之外仅 2 条消息（用户提问 + plan mode 注入提醒） |
| 26 | 0.2 | 6 | 加了 assistant 回复 + 3 个工具结果 |
| 36 | 0.3 | 9 | |
| 45 | 0.4 | 11 | |
| 58 | 0.5 | 14 | |
| 67 | 0.6 | 16 | |
| 81 | 0.7 | 19 | 第 0 轮最后一步 |
| 112 | 1.1 | 21 | 第 1 轮开始（注意：轮次间上下文持续增长） |
| 120 | 1.2 | 23 | |
| 138 | 2.1 | 25 | |
| 154 | 3.1 | 27 | |
| 170 | 4.1 | 29 | |
| 178 | 4.2 | 31 | |
| 186 | 4.3 | 33 | |

配套现象：每次 `llm.request` 后紧跟 `usage.record`（token 用量，含 `inputCacheRead` 缓存命中——全量重发之所以可行，靠的就是 prompt cache）和 `token_counting.measured`。

### 事件类型完整清单（按样例文件出现顺序）

进入 LLM 上下文的：
- `profile.bind` — system prompt 全文（`systemPrompt` 字段）、模型别名、激活工具名列表
- `llm.tools_snapshot` — 完整工具 JSON Schema 数组（`tools` 字段）+ 其哈希
- `context.append_message` — 持久化的消息（role: user/assistant；含注入消息，如 plan mode 提醒，`origin.kind: "injection"`）
- `context.append_loop_event` — loop 内事件，`event.type` 子类型：`step.begin` / `content.part`（think 或 text 片段）/ `tool.call`（工具名+参数）/ `tool.result`（工具输出）

不进 LLM 上下文的运行时事件：
- `metadata`（第 1 行，协议版本）、`runtime.set_binding`、`permission.set_mode`、`plan_mode.enter` / `plan_mode.exit`、`plugin.session_start`
- `llm.request`（请求标记，含 `turnStep`/`messageCount`/`systemPromptHash`/`toolsHash`）
- `usage.record`（`usage: {inputOther, output, inputCacheRead, inputCacheCreation}`）
- `token_counting.measured` / `token_counting.turn_recorded`
- `turn.prompt`（用户输入入队）、`agent.turn.started` / `agent.turn.ended` / `turn.ended`（含 durationMs、traceId）、`prompt.completed`
- `agent.message.appended`（用户消息进入消息总线）
- `plan.revision`（计划文件版本，含 sha256/bytes）
- `interaction.request` / `interaction.resolved`（用户审批交互，如 ExitPlanMode 审批，`request.display` 里甚至内嵌完整计划文档）
- `permission.record_approval_result`
- `file_history.tracked` / `file_history.checkpoint`（文件编辑历史快照）

---

## 第二部分：现有工具调研（为什么自研）

| 工具 | 类型 | 对本场景的不足 |
|---|---|---|
| jsonlify.com/jsonl-viewer、jsonlviewer.com 等 | 在线网页 | wire.jsonl 含完整 system prompt 和项目内部信息，**不宜上传第三方网站** |
| fx (fx.wtf) | 终端 TUI | 面向单 JSON 文档的交互查询，逐行浏览 JSONL 不是强项 |
| github.com/wangyuchenbtla/jsonl-viewer | 终端 TUI | 通用列表+树；对单行 10 万+ 字符、嵌套转义长文本无专门优化 |
| VS Code「JSON Lines」类扩展 | 编辑器插件 | 只有格式化/校验，无逐行导航摘要，大文件卡 |
| jq / jnv | CLI | 命令行过滤强大，但「随便翻看」体验差 |

**共性缺口（= 我们的核心差异化）**：wire.jsonl 里一个字段值可能是几万字符、带大量 `\n` 转义的 system prompt 或思考内容，所有现有工具都只把它当普通字符串显示。我们需要「**解转义 + 多行排版 + 等宽字体**」地把长字符串渲染成人能读的文本。

---

## 第三部分：开发方案

### 3.1 技术选型（已确定，不要再改）

- **语言：Rust**（stable toolchain）。单 exe、启动 <200ms、无外部运行时。
- **GUI：eframe/egui 0.31**。纯 Rust 立即模式 GUI；release 单文件 exe 约 10MB；`ScrollArea::show_rows` 提供虚拟滚动，百万行不卡；`CollapsingHeader` 天然适合 JSON 树。
- **JSON：serde_json**。懒解析策略：打开文件时只做切行和字节偏移索引，不 parse；用户选中某行才 parse 该行。
- **切行：memchr**（SIMD 加速找 `\n`）。
- **文件对话框：rfd**；**剪贴板：arboard**（egui 自带复制也可，二选一）。
- **语法高亮**：不引 syntect（拖慢启动），手写 JSON 键/字符串/数字/布尔着色即可。
- 平台：Windows 优先，eframe 天然跨平台（macOS/Linux 可直接编译）。

### 3.2 环境与构建

```bash
# 一次性环境（另一台电脑若无 Rust）
# Windows: winget install Rustlang.Rustup   或跑 rustup-init.exe
rustup default stable

# 项目初始化（在空目录 jsonl-v 下）
cargo init --name jsonl-v
```

`Cargo.toml` 完整内容：

```toml
[package]
name = "jsonl-v"
version = "0.1.0"
edition = "2021"

[dependencies]
eframe = "0.31"
egui = "0.31"
serde_json = "1"
memchr = "2"
rfd = "0.15"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

### 3.3 项目结构

```
jsonl-v/
├── Cargo.toml
└── src/
    ├── main.rs       # eframe 入口：窗口标题/尺寸/图标，run_native
    ├── app.rs        # App 状态机 + 三栏布局（左：行列表；右：详情；底：状态栏）+ 快捷键
    ├── document.rs   # JsonlDocument：mmap 或一次性读入 + memchr 切行 + 行偏移索引 + LRU 解析缓存
    ├── json_tree.rs  # serde_json::Value → egui 可折叠树；键/字符串/数字/布尔/null 着色
    ├── text_view.rs  # 长字符串解转义渲染：多行、自动换行、等宽字体、复制按钮
    ├── search.rs     # 后台线程全文搜索，channel 增量回传命中行号
    └── wire.rs       # wire.jsonl 增强：事件类型识别/着色、llm.request 时间线、上下文重建
```

### 3.4 核心模块设计

**document.rs**

```rust
pub struct JsonlDocument {
    pub path: PathBuf,
    buf: Vec<u8>,                  // 全量字节（先不做 mmap，100MB 内 Vec 足够快）
    line_offsets: Vec<usize>,      // 每行起始偏移；行 i 内容 = buf[offsets[i]..offsets[i+1]]
    cache: LruCache<usize, LineInfo>, // 容量 64，选中才填充
}
pub struct LineInfo {
    pub parsed: Result<serde_json::Value, String>, // 坏行存错误信息
    pub summary: String,           // 列表摘要：type/role 等首个有意义字段 + 字节数
}
// 关键 API：
//   open(path) -> io::Result<Self>   // 只切行建索引，目标 100MB < 1s
//   line_count(&self) -> usize
//   line_info(&mut self, i) -> &LineInfo  // 懒解析 + 缓存
//   raw_line(&self, i) -> &str
//   reload(&mut self)              // 文件被外部追加/修改后重载（按 mtime+size 判断）
```

**app.rs — UI 布局**

```
┌──────────────────────────────────────────────────────────────┐
│ 菜单栏: [打开文件] [重新加载]  🔍搜索框   ☑仅看匹配  事件过滤▼ │
├───────────────────────┬──────────────────────────────────────┤
│ 行列表（虚拟滚动）      │ 详情面板（Tab: 树视图│美化文本│原始行）│
│ #  摘要         大小   │ ┌──────────────────────────────────┐ │
│ 1  metadata      62B  │ │ 可折叠 JSON 树，键类型着色        │ │
│ 5  profile.bind  38KB │ │ 长字符串值显示 [📄纯文本查看]按钮  │ │
│ 14 ⚡llm.request  0.1 │ │                                  │ │
│ ...                   │ └──────────────────────────────────┘ │
├───────────────────────┴──────────────────────────────────────┤
│ 状态栏: 200 行 | 216KB | 当前第 14 行 | wire 模式: 14 次请求  │
└──────────────────────────────────────────────────────────────┘
```

- 快捷键：`↑/↓/PgUp/PgDn` 移动选中行，`Ctrl+F` 聚焦搜索，`F3/Shift+F3` 下一个/上一个命中，`Ctrl+C` 复制当前行美化文本，`F5` 重载文件，`Ctrl+O` 打开。
- 行列表摘要规则：优先取 `type` 字段；无则取 `role`；再无则取前 40 字符。尾部显示该行字节数（KB 单位标橙，>100KB 标红）。
- 「美化文本」Tab：`serde_json::to_string_pretty`，等宽字体，可选自动换行。
- 「原始行」Tab：未经解析的一行原文（验证解析保真用）。

**text_view.rs — 本工具的灵魂**

判定规则：字符串值含 `\n` / `\t` 转义序列，或长度 > 200 字符 → 在树节点旁显示「纯文本查看」按钮。点击后弹窗或内嵌面板：
- 展示**解转义后**的真实文本（`\n` → 换行，`\\` → `\`，unicode 转义还原，中文正常显示）
- 等宽字体、自动换行开关、显示行数/字符数、一键复制
- 典型受益字段：`systemPrompt`、`content[].text`、`think`、`args.command`、`result.output`、`request.display.plan`

**wire.rs — wire.jsonl 增强**

检测条件（满足即自动启用，状态栏显示「wire 模式」）：文件前 10 行内存在 `"protocol_version"` 的 `metadata` 行，或任意行含 `"type":"llm.request"`。

功能：
1. **事件着色**：llm.request=黄、tool.call=蓝、tool.result=绿、content.part(think)=紫、content.part(text)=灰、usage.record=青、interaction.*=橙、错误/坏行=红。
2. **请求时间线面板**（可折叠，置于行列表上方）：列出全部 `llm.request`，每项显示 `#序号 turnStep messageCount`，点击跳转到对应行。
3. **上下文重建**：选中某个 `llm.request` 行时，详情面板多一个「重建上下文」Tab——向前扫描收集 `context.append_message` 和 `context.append_loop_event`，按消息序列渲染（role 标签 + 内容 + 工具调用配对），近似还原该次请求实际发送的 messages 数组。tool.call / tool.result 按 `toolCallId` 配对缩进显示。
4. **请求体 JSON 查看**：「重建上下文」Tab 工具行有「📦 请求体 JSON」按钮——把重建结果组装成 OpenAI Chat Completions 风格的完整请求体（`model`/`max_tokens` 取自该 `llm.request` 行；`messages[0]` 为 system 提示词，连续的 think/text/tool.call 合并为一条 assistant 消息，tool.result 为带 `tool_call_id` 的 tool 消息；`tools` 取自最新 `llm.tools_snapshot`），在独立大窗口（900×700）中以可折叠 JSON 树展示，支持全展开/全折叠、复制全部、树内长字符串再开纯文本窗口。
5. **usage 小结**：时间线每项旁显示紧随的 `usage.record` 的 input/output/cacheRead token 数。

**search.rs**

- 输入即搜（防抖 200ms），在后台线程对**原始字节**做 `memchr` 预筛，命中行才 parse 验证，结果经 channel 增量回传。
- 支持：普通子串（默认不区分大小写）、`key:value` 形式（如 `type:tool.call`）、正则（可选开关，regex crate 可选加，不加则用子串）。

### 3.5 测试计划

- `document.rs` 单测：空文件、单行无换行结尾、空行、坏 JSON 行、含中文/emoji/转义的行、10 万字符超长行、1 万行文件的索引正确性。
- `wire.rs` 单测：事件类型分类、llm.request 提取、上下文重建的消息计数与样例文件实测值一致（见第一部分表格：14 次请求，messageCount 序列 2,6,9,11,14,16,19,21,23,25,27,29,31,33）。
- 测试夹具：直接用样例 `wire.jsonl`（200 行）+ 手工构造的 edge-case 小文件。

### 3.6 实施步骤（按序执行，每步可独立验证）

1. `cargo init`，写入 3.2 的 Cargo.toml，`cargo build` 通过。
2. 实现 `document.rs` + 单测，`cargo test` 通过。
3. `main.rs` + `app.rs` 骨架：打开文件（命令行参数拖入 / Ctrl+O / rfd 对话框）、行列表虚拟滚动、状态栏。用样例 wire.jsonl 人工验证滚动流畅。
4. `json_tree.rs` 详情树 + 着色 + 三个 Tab 切换。
5. `text_view.rs` 长文本解转义查看（**重点验收**：打开样例第 5 行 profile.bind，systemPrompt 能按真实换行阅读）。
6. `search.rs` 搜索 + 跳转。
7. `wire.rs` 全部增强功能（验收：时间线列出 14 次请求；点 turnStep=0.1 的「重建上下文」应显示 2 条消息，与 messageCount 一致）。
8. `cargo build --release`，确认 exe 单文件可运行、冷启动 <200ms、打开 216KB 样例即时、构造 100MB 测试文件打开 <1s。

### 3.7 验收清单

- [ ] `cargo test` 全绿
- [ ] `cargo build --release` 产出单 exe（Windows），双击可运行
- [ ] 打开样例 wire.jsonl：左侧 200 行摘要正确，状态栏显示「wire 模式: 14 次请求」
- [ ] 第 5 行 systemPrompt 长文本可解转义多行阅读、可复制
- [ ] 选中行 14（llm.request turnStep 0.1）→ 重建上下文 Tab 显示 2 条消息
- [ ] 搜索 `tool.call` 能列出全部命中行并跳转
- [ ] 文件被外部追加后按 F5 重载正常
- [ ] 100MB 合成文件：打开 <1s，滚动不卡

### 3.8 非目标（本期不做）

- 编辑/保存 JSONL
- 多文件对比、合并
- 网络加载（URL 打开）
- 主题系统（先用 egui 默认深色，提供一个浅/深切换即可）
