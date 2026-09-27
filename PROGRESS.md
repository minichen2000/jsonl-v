# jsonl-v 进度与待办

> 每次完成任务或发现新待办/新坑时同步更新本文件（约定见 AGENTS.md）。

## 当前进度

当前版本 0.1.4，已实现：

- JSONL 查看：行列表（行号/摘要/字节数）、JSON 可折叠树、格式化文本/原始行页签
- 长字符串「纯文本查看」弹窗、请求体「JSON 查看」弹窗（树/美化双模式），弹窗均可最大化
- 搜索：输入即搜（防抖+后台线程）、`key:value` 语法、`F3` 跳转、仅看匹配
- wire.jsonl 增强模式：事件着色、请求时间线、完整上下文（还原）页签（分侧统计与徽标）、事件过滤
- 文本区可编辑（不保存）、右键「拷贝」菜单、内嵌 Noto Emoji 字体
- 设置：字号/主题/语言、资源管理器右键菜单注册、最近文件
- GitHub Actions 三平台 Release（tag 触发）

## 待办

- 暂无（「全屏卡死」已定位为 accesskit 崩溃并修复，见下）

## 已知坑 / 注意事项

- **egui 0.31 AccessKit 崩溃（本次「全屏卡死退出」的根因）**：有 UIA 客户端挂着时（输入法/读屏/自动化工具），窗口 id 变化（如全屏切换）会让 `accesskit_consumer` unwrap None panic；UI 线程 panic + `windows_subsystem="windows"` = 无声闪退。修复：Cargo.toml 里 eframe 关默认特性、裁掉 `accesskit`（代价：无屏幕阅读器支持）。排查方法：panic 钩子写 `%APPDATA%/jsonl-v/crash.log`。
- **显示器尺寸 ≠ `ctx.screen_rect()`**：后者是窗口自身客户区；显示器逻辑尺寸用 `ctx.input(|i| i.viewport().monitor_size)`，头一两帧可能是 None。主窗口启动尺寸/居中修正见 `app.rs::fit_main_window_at_startup`。
- **eframe 跨会话持久化窗口位置/尺寸**：egui 按窗口 id 记忆，旧的记忆会盖掉 `default_size`/`default_pos`。想让新的默认生效必须换 id 盐（如 `json_view_v2`）重置记忆。
- **egui 拖选到滚动区边缘不持续滚动**：内置 `scroll_to_rect` 只在选区变化的帧触发且无重绘循环，指针停在边缘即停。已在 `app.rs::drag_edge_autoscroll` 每帧补滚动量 + `request_repaint` 解决。
- **egui TextEdit 右键按下会清空选区**（`any_pressed` 判定）：`show_editable_text` 里用上一帧缓存 galley 预垫高亮 + 事后还原选区防闪烁，改动该区域代码前先读懂注释。
- **egui `Window::fixed_size` 是内容区尺寸**：全屏窗口位置尺寸要用 `app.rs::maximized_pos_size` 按公式扣掉标题栏和边框，否则右边/下边超出屏幕。
- **深色模式黑阴影无层次**：`popup_frame` 在深色下改用亮描边 + 淡白泛光。
- Windows 图标由 `build.rs` 内嵌，`assets/icon.ico` 缺失时静默跳过。
- Release 只认 GitHub 的 tag CI；平时推 main 不出包。
