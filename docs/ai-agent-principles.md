# AI Coding Agent 内部原理学习笔记

> 以 Kimi Code 的 `wire.jsonl` 会话日志为线索，理解一个 AI Coding Agent 如何与 LLM 交互、
> 上下文如何组织，以及背后的模型能力是怎么训练出来的。
>
> 本文内容来自对一个真实会话（约 2700 行日志、241 次 LLM 请求）的实测分析。

## 目录

1. [wire.jsonl 是什么](#1-wirejsonl-是什么)
2. [日志行类型全解](#2-日志行类型全解)
3. [上下文是如何拼出来的](#3-上下文是如何拼出来的)
4. [Agent 如何探索陌生代码库](#4-agent-如何探索陌生代码库)
5. [为什么"看得少却分析得准"](#5-为什么看得少却分析得准)
6. [当项目故意混淆时怎么办](#6-当项目故意混淆时怎么办)
7. [这些能力是怎么训练出来的](#7-这些能力是怎么训练出来的)
8. [关键认知总结](#8-关键认知总结)

---

## 1. wire.jsonl 是什么

Kimi Code 把会话中发生的**每一件事**追加为一行 JSON，形成事件日志（journal）：

- 位置：`~/.kimi-code/sessions/<工作区>/<会话>/agents/<agentId>/wire.jsonl`
- 主 agent 在 `agents/main/`；Agent 工具派生的子 agent 各有自己的 wire.jsonl
- 带 `protocol_version`（如 "1.5"），说明格式在演进
- 它是**增量日志**：不重复记录完整上下文，只记"上下文是怎么一步步拼起来的"事件

> 注意：wire.jsonl 这个格式是 Kimi Code 私有的，但它承载的数据模型
> （messages 数组、role: user/assistant/tool、tool_calls）是 OpenAI Chat Completions
> 风格的业界通用约定。

## 2. 日志行类型全解

实测一个真实会话出现的全部约 36 种记录类型，按角色分七大类。

### 2.1 会话级元信息（启动时写一次，纯本地记录，不进上下文）

| type | 内容 |
|---|---|
| `metadata` | 第 1 行。协议版本 + 创建时间 |
| `plugin.session_start` | 会话启动钩子 |
| `runtime.set_binding` | 绑定工作区 id / 运行时 |
| `permission.set_mode` | 权限模式（如 yolo） |
| `config.update` | 运行中改模型/思考强度等配置 |
| `profile.bind` | 绑定角色档案，**含 systemPrompt 原文**、环境披露、AGENTS.md 路径、可用工具清单 |
| `llm.tools_snapshot` | **工具定义原文快照**（hash + 完整 tools 数组），工具集变化时重写一条 |

`profile.bind` 和 `llm.tools_snapshot` 特殊：它们本身是本地记录，但其**内容**（系统提示词、
工具定义）是每次 LLM 请求的组成部分。

### 2.2 turn 生命周期（起止标记，不进上下文）

一次「用户提问 → Agent 干完活」叫一个 turn（`turnId: 0,1,2...`）：

- `turn.prompt`：用户输入进入队列（`promptId`，`origin.kind: "user"`）
- `agent.turn.started` / `agent.turn.ended`：turn 开始/结束（`kind: "event"`，UI 事件流）
- `turn.ended`：带 `reason` 和耗时
- `prompt.completed`：该 prompt 处理完毕
- `agent.message.appended`：消息追加的事件通知（与 `context.append_message` 同内容，
  前者是 UI 事件流，后者才是写上下文）

### 2.3 进上下文的核心记录（重点）

**发给 LLM 的 messages 数组 = 以下记录按序拼接。**

**`context.append_message`** — 一条完整消息落库进上下文。来源看 `message.origin.kind`
（实测一个真实会话的分布：user 30 条、injection 34 条、task 9 条）：

- `user`：**用户真正敲的话**
- `injection`：**Agent 框架自己塞的**（详见 2.3.1）
- `task`：**后台任务完成通知**。Agent 跑后台命令（如长时间构建）时，任务结束由宿主
  以一条 user 角色消息的形式送达，内容包在 `<notification>` 标签里
  （含 taskId、状态、输出摘要），origin 里带 `taskId`/`status`
- 压缩摘要：压缩发生后，摘要也以 append_message 形式进上下文

#### 2.3.1 injection（注入消息）详解

注入消息是**宿主框架在对话中途动态插入的、user 角色的消息**，用户看不见
（终端里不显示），但 LLM 看得见。实测本 session 里的注入内容：

| 注入内容 | 触发时机 | 例 |
|---|---|---|
| Plan 模式规则 | 进入/退出 plan 模式时 | "Plan mode is active. You MUST NOT make any edits..." |
| TodoList 提醒 | 框架发现待办清单久未更新时 | "The TodoList tool has not been updated recently..." |
| 上下文压缩通知 | 自动压缩发生后 | 告知旧内容已被摘要替代、如何回查原文 |
| 技能（skill）正文 | LLM 调用 Skill 工具时 | 技能清单在 system prompt 里只有名称和简介；调用后完整操作说明以 `<skill-loaded>` 块注入对话 |

**注入消息与系统提示词（system prompt）的区别**——这是两类完全不同的东西：

| | system prompt（profile.bind） | injection（append_message） |
|---|---|---|
| 位置 | 请求体最前面，独立字段 | messages 数组中间，假装成一条 user 消息 |
| 内容 | 角色设定、行为准则、工具用法、项目目录树 | 临时状态通知：模式切换、提醒、事件 |
| 变化频率 | 一次会话基本不变（所以能命中缓存） | 随对话进展随时插入 |
| 日志位置 | `profile.bind` 记一次原文 | 每条一条 `context.append_message` |
| 谁写的 | Kimi Code 产品方预置 | 宿主框架按当前状态即时生成 |

为什么要分成两层？因为 prompt cache 按前缀命中：system prompt 不变，每次请求都命中
缓存；而变化频繁的状态提醒放在 messages 尾部追加，不破坏前缀。如果把易变内容塞进
system prompt，每次变化都会让全量缓存失效，成本大增。

**`context.append_loop_event`** — 一次 LLM 往返中的流式事件，内嵌 `event.type` 子类型：

| 子 type | 谁产的 | 进上下文？ |
|---|---|---|
| `step.begin` / `step.end` | 宿主 | **否**，纯起止括号（turnId + step 编号） |
| `content.part`（part.type=`think`） | **LLM 返回的思考** | 是（assistant 消息一部分） |
| `content.part`（part.type=`text`） | **LLM 返回的正式回答** | 是 |
| `tool.call` | **LLM 要求调工具**（name/args/toolCallId） | 是 |
| `tool.result` | 宿主执行工具后的结果（toolCallId 与 call 配对） | 是 |

### 2.4 LLM 交互记账

- **`llm.request`**：一次请求的元数据。关键字段：`turnStep`（"4.2" = 第 4 轮第 2 次请求）、
  `messageCount`、`systemPromptHash`/`toolsHash`（对应回 profile.bind / tools_snapshot）、
  `maxTokens`。**请求原文不记，可靠 2.3 的记录重建**
- **`usage.record`**：紧跟每次 request。`inputOther`/`output`/`inputCacheRead`/`inputCacheCreation`。
  cache_read 占比高说明前缀未变、命中服务商 prompt 缓存
- `token_counting.measured`：每次请求后实测上下文 token 数
- `token_counting.turn_recorded`：turn 结束时的总量记账
- `token_counting.rebased`：压缩后重定基线

### 2.5 人机交互与权限

- `interaction.request`：Agent 要询问用户（`kind: "approval"`，挂在某个 toolCallId 上）
- `interaction.resolved`：用户的答复，**按 id 与 request 配对**
- `permission.record_approval_result`：审批结果归档

不进上下文；用户的决定通过随后允许执行产生的 `tool.result` 间接进上下文。

### 2.6 执行单元 bookkeeping（不进上下文）

- 后台任务：`task.started` / `task.terminated`（命令、状态、输出尾巴）/ `task.waitDelivered`
- 计划模式：`plan_mode.enter` / `plan_mode.exit` / `plan.revision`（计划文件版本指针）
- 文件历史：`file_history.tracked` / `file_history.checkpoint`（改动备份，用于撤销）
- `tools.update_store`：TodoList 等工具的内部存储更新

### 2.7 上下文压缩

- `full_compaction.begin` → `context.apply_compaction`（含摘要原文、压缩前后 token 数、
  丢弃/保留统计）→ `full_compaction.complete`

### 2.8 关系一张图

```
session（metadata 开头，一次会话一个 wire.jsonl）
 └─ turn（turn.prompt 开始 → turn.ended 结束，turnId 串联）
     └─ step（step.begin → step.end，一次 LLM 往返；turnStep="T.S"）
         ├─ llm.request（要发请求了，messageCount=N）
         │    请求体 = profile.bind 的 systemPrompt
         │           + llm.tools_snapshot 的 tools
         │           + 此前所有 append_message/loop_event 拼的 messages
         ├─ content.part（LLM 返回 think/text）
         ├─ tool.call（LLM 要调工具）→ 宿主执行 → tool.result
         │    （敏感操作先弹 interaction.request，用户批准才执行）
         ├─ usage.record（这次花了多少 token）
         └─ 若又调了工具 → 下一个 step 再来一圈
```

**一句话总结**：整个文件里真正会发给 LLM 的只有五样——
`context.append_message`（用户输入 + 框架注入）、`content.part`/`tool.call`/`tool.result`
（LLM 产出和工具结果）、`profile.bind`（系统提示词）、`llm.tools_snapshot`（工具定义）。
其余三十多种全是宿主的本地账：起止标记、token 统计、审批留痕、任务管理、文件备份。

## 3. 上下文是如何拼出来的

### 3.1 哪一行代表"向 LLM 发了请求"

`llm.request` 那一行。但它**不含请求原文**，只有元数据。原因：

- 真实请求体可能几百 KB，逐行记录会让日志爆炸
- 第 N 次请求 = 第 N-1 次内容 + 增量，记原文会大量重复

所以日志只记增量事件，原文可以重建。

### 3.2 重建公式

```
请求体 = systemPrompt（取请求行之前最新的 profile.bind）
       + tools（取请求行之前最新的 llm.tools_snapshot）
       + messages（请求行之前所有 context.append_message
                  和 append_loop_event 的 content/tool 事件，按序拼接）
```

拼接规则：每条 append_message 原样一条消息；连续的 think/text/tool.call 拼成一条
assistant 消息；每个 tool.result 一条 tool 消息。重建出的消息数应等于
`llm.request.messageCount`——这可以反过来验证重建的正确性。

### 3.3 为什么每次请求的输入 token 大部分命中缓存

`usage.record` 里常见 `inputCacheRead` 占输入的 90%+。因为上下文只追加不修改，
前缀完全不变，服务商的 prompt cache 直接命中——这既省钱又让"每次都发全量上下文"
这件事实际开销远没有看起来大。

### 3.4 system prompt 里有什么

以 Coding Agent 为例，system prompt 里包含：角色与行为准则、工具用法说明、
沟通风格要求、安全规则，以及**自动注入的项目两层目录树**和 AGENTS.md 内容——
这是 LLM 对项目的初始认知来源。

## 4. Agent 如何探索陌生代码库

### 4.1 启动时就有"地图"，不用主动要

Kimi Code 在每次请求的 system prompt 里自动注入工作区的**两层目录树**。
LLM 从第一轮起就知道项目根下有什么，一次工具调用都不用花。

### 4.2 为什么只给两层，而不是全量文件树

全量树是陷阱：真实项目可能有上万个文件（node_modules、.git、构建产物），
塞进上下文既贵又淹没关键信息。策略是**给骨架、按需展开**：

- 自动给的：两层目录树 + AGENTS.md（项目作者留给 AI 的"交接文档"）
- LLM 按需取的：三个"探针"
  1. **Glob**（按文件名找）——相当于翻目录
  2. **Grep**（按内容找）——结果自带行号，是"第几行"的来源
  3. **Read**（按行号区间读）——只取目标附近一段

### 4.3 面对已有项目改 bug 的典型路径

1. 读 system prompt 里的目录树 → 知道大概结构
2. 读 README / Cargo.toml / package.json 等"门面文件" → 项目干什么、怎么构建
3. 按 bug 描述的关键词 Grep → 拿到精确文件和行号
4. Read 相关片段 → 改 → 跑测试验证

和人类工程师接手老项目的流程完全一致。

### 4.4 辅助机制

Glob/Grep 默认尊重 `.gitignore`，自动跳过 target/、node_modules/ 等噪声目录，
"按内容搜"不会被无关文件干扰。

## 5. 为什么"看得少却分析得准"

看日志会发现 Agent 每次只读局部片段，却分析得很到位。三个原因叠加：

### 5.1 先验知识：这些代码模式"见过"几百万次

训练数据里有海量公开代码和讨论。看到 `wire.rs` 文件名 + `classify` 函数名，
唤起的不是"陌生文本"而是"这类解析器通常长什么样"的完整模板。
**局部片段的作用不是提供全部信息，而是确认"这是哪个已知模式"。**

例：症状"小文件正常、大文件跳转落空"本身就是强信号——"小问题大坏" ≈ 随规模
累积的误差 ≈ 每行差一点。再看到 `row * row_h`，假设立刻成型。

### 5.2 读的都是"判决性证据"

每次读取都是定向验证：怀疑行距算错，就直接去读 UI 框架 `show_rows` 的实现那 20 行，
因为答案只可能在那里。看得少，但信息密度极高。而且**假设必须验证**：
翻框架源码确认、跑全部测试，不盲目自信。

### 5.3 代码冗余度极高，局部足以重建整体

代码是强约束文本：import 语句、类型签名、命名、编译错误信息都在交叉印证同一件事。
20% 的代码 + 类型系统 + 报错信息，通常足够推出剩下 80% 在干嘛。

**边界**：这套方式对模式化代码（UI、解析、CRUD、构建配置）特别有效；
对独特的业务逻辑、私有协议，先验帮不上忙，就得老老实实多读多问——
这也是测试和用户验证重要的原因。

## 6. 当项目故意混淆时怎么办

文件名、函数名都没意义时，先验捷径失效。但代码有个本质特点：
**名字可以骗人，行为骗不了人**。程序必须老老实实告诉机器做什么，
真实意图会从藏不住的地方漏出来：

1. **入口和接线藏不住**：main()、路由注册、消息循环——不管函数叫什么，
   程序必须从真实入口开始跑。调用关系图本身就是语义
2. **字符串基本不骗人**：界面文字、报错信息、日志、配置 key 是给用户看的，
   通常明文。Grep 一句报错就能定位逻辑
3. **类型和数据流比名字可靠**：参数是 `Vec<Order>`、返回 `Result<Payment>`，
   数据从哪来到哪去，这条链绕不开
4. **测试是活文档**：测试的调用方式和断言期望值直接暴露"这段代码应该干嘛"
5. **跑起来看**：静态读不动就动态观察，行为是最终真相
6. **git 历史**：commit message 保留作者思路碎片

### 6.1 怎么发现情况"糟"

靠**预期落空**报警：README 缺失 → 文件名无语义 → Grep 按常理必有的关键词零命中
→ 入口函数全叫 `do_thing()`。通常几个廉价探针内就能确认，不用读很多。

### 6.2 策略上做什么不同

核心转变：**从"假设驱动"退到"测绘驱动"**。

- 读法变了：从切片变整读，从定点跳读变**按调用链机械遍历**（不是随机读，
  随机读没有结构；顺着调用图、import、类型引用这些没法混淆的硬连接走）
- 边读边给自己写地图，把结论记下来（比如补写 AGENTS.md）
- 更多依赖运行时证据：跑测试、加日志；没有测试就先写**特征化测试**
  （不断言"应该怎样"，只断言"现在怎样"），把现状钉死再改
- 步幅变小、验证变多，因为理解的可信度低
- 更频繁地问人——领域知识这时特别值钱

代价：成本大幅上升，和人接手混淆代码处境一样，只是读得快。
极端情况（商业级混淆、加壳二进制）应明确建议换思路。

**反向启示**：好的命名、清晰的结构、完善的测试不仅是给人看的，也是给 AI 看的。
README、AGENTS.md 这些"写给读者的文档"，现在读者里包括 AI。

## 7. 这些能力是怎么训练出来的

方向：预训练 + 监督微调（SFT）+ 强化学习（RL）一层层堆出来。
（各家具体配方是商业机密，以下是业界公开的大致做法。）

### 7.1 第一层：预训练——"见过"几千万个项目

基础模型读了 GitHub 几乎所有公开代码、Stack Overflow、文档、issue。
纯自监督（预测下一个词），不需要人教。"看到某个 API 就联想到常见 bug 模式"
这种直觉就来自这里——**前面说的"先验"，本质就是这一层**。

### 7.2 第二层：监督微调——学会"干活的样子"

教模型接到任务后的正确操作序列（先 Glob 看结构 → Grep 定位 → Read 片段 →
改代码 → 跑测试）。轨迹数据来源：

- **人写**：早期靠人写示范，贵且慢，只够做"种子"
- **模型生成 + 人筛**：已有模型生成大量轨迹，人只负责挑好坏
- **更强模型蒸馏**：强模型当老师教小模型

### 7.3 第三层：强化学习——编程是 AI 的"天堂领域"

编程能力进步快的真正原因：**代码任务的奖励可以全自动验证，不需要人**。

1. 准备海量真实编程任务（如从 GitHub 扒 "issue 描述 + 修复 commit + 配套测试"）
2. 模型在真实沙箱里干活：读代码、改代码、跑测试
3. 测试通过 = 奖励，编译失败/测试红 = 惩罚
4. 一次训练跑成千上万局，全自动，24 小时不停

没有人陪它对话——**单元测试就是那个不知疲倦的裁判**。
这叫"可验证奖励的强化学习"（RLVR）。"先看懂再改"、"改坏了自己发现"、
"小步验证"这些习惯，都是在这个循环里被奖励出来的：成功率高的轨迹被强化。

### 7.4 鸡生蛋问题：第一个模型要堆多少人？

比想象中少得多，因为有清晰的接力链：

1. **第一棒确实是人**：几千到几万条人写的高质量示范，就够把基础模型"启动"成
   会基本干活的模型。量级是几十到几百人团队几个月，不是百万大军
2. **第二棒机器互相教**：启动后的模型能生成海量合成数据，人低成本抽检
3. **第三棒几乎无人**：RL 靠测试/编译器自动打分，不需要人陪练
4. **滚雪球**：这一代模型生成的轨迹用来训练下一代——"第一个模型需要人"只发生一次

这就是为什么编程能力近几年涨得特别快：编程恰好是"验证成本为零"的领域
（数学同理）；而"这封邮件写得好不好"没有自动裁判，进步就慢。

### 7.5 日志里看到的行为对应哪一层

| 观察到的行为 | 来源 |
|---|---|
| 知道 Rust/框架怎么写 | 预训练 |
| 收到任务先定计划、先测试后交付 | SFT + RL |
| 改坏了不死磕、换思路 | RL（死磕的轨迹在沙箱里都失败了） |
| 不盲目自信、让用户验证 | 人类反馈（RLHF） |

## 8. 关键认知总结

1. **Agent = LLM + 工具 + 循环**。LLM 本身只会"输入文本输出文本"；宿主程序
   （Agent 运行时）提供工具、执行工具、把结果拼回上下文、再请求，循环直到任务完成
2. **上下文是全部**。LLM 没有记忆，每次请求都带全量上下文；日志是增量的，
   上下文可精确重建
3. **探索靠"骨架 + 探针"**，不是全量加载。目录树给地图，Glob/Grep/Read 按需取证
4. **先验知识决定效率**，验证机制兜底正确性
5. **名字会骗人，行为不会**：混淆提高成本但挡不住结构分析
6. **编程能力来自可验证奖励的强化学习**——测试通过就是免费的裁判，
   这是编程区别于其他 AI 应用的独特优势
