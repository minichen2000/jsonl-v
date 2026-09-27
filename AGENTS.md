# jsonl-v 项目说明（供 AI 助手参考）

## 仓库与远程

- Gitee: https://gitee.com/minichen2000/jsonl-v （远程名 `origin`）
- GitHub: https://github.com/minichen2000/jsonl-v （远程名 `github`）
- 每次改动提交后双推：`git push origin main && git push github main`

## 构建与发布

- 每次改动后本地构建验证：`cargo build --release --locked`，产物 `target/release/jsonl-v.exe`
- 只发布绿色单 exe/裸二进制
- Release 由 GitHub Actions 构建：`.github/workflows/release.yml` 在推送 `v*` tag 时触发，构建 Windows/macOS/Linux 三平台产物并自动创建 GitHub Release；平时推 main 不触发 CI
- 发版流程：
  1. 更新 `Cargo.toml` 的 `version`（`Cargo.lock` 同步）
  2. 提交并双推 main；打 tag 并双推：`git tag vX.Y.Z && git push origin vX.Y.Z && git push github vX.Y.Z`
  3. tag 推到 github 后 CI 自动出三平台 Release，用 `gh run list -R minichen2000/jsonl-v` 确认通过

## 技术栈

- 纯 Rust + egui，单文件 exe
- Windows 下图标由 `build.rs` 内嵌（`#[cfg(windows)]` 门控；`assets/icon.ico` 缺失时跳过）

## 记录文件维护约定

每次完成任务/有改动时，同步更新相关记录文件，不要只改代码：

- `PROGRESS.md`（中文）：进度、待办、已知坑
- `CHANGELOG.md`（英文）：影响用户的改动记入 Unreleased 段
- `AGENTS.md`：架构决策、新依赖、新踩坑、流程变化
- `README.md` / `README.zh-CN.md`：功能特性、用法变化时同步
- `BUILDING.md` / `BUILDING.zh-CN.md`：构建、测试、发版流程变化时同步

## egui 踩坑记录

- **accesskit 崩溃 = 无声闪退**：egui 0.31 的无障碍树 diff 有 bug，UIA 客户端（输入法/读屏/自动化工具）挂着时，窗口 id 变化会触发 `accesskit_consumer` unwrap None panic。本项目已在 Cargo.toml 关掉 eframe 的 `accesskit` 特性（勿改回默认特性）。UI 线程 panic 由 `main.rs::install_panic_hook` 写 `%APPDATA%/jsonl-v/crash.log`。
- **显示器尺寸用 `ViewportInfo::monitor_size`**，`ctx.screen_rect()` 是窗口自身客户区；主窗口启动居中见 `app.rs::fit_main_window_at_startup`。
- 拖选文本到滚动区边缘不持续滚动：内置滚动只在选区变化的帧触发且无重绘循环。修法：`app.rs::drag_edge_autoscroll` 在拖选期间每帧按指针超出边缘的距离补滚动量并 `request_repaint`。
- TextEdit 右键按下会清空选区：见 `app.rs::show_editable_text` 的预垫高亮 + 还原选区处理。
- `Window::fixed_size` 设的是内容区尺寸：全屏窗口用 `app.rs::maximized_pos_size` 扣减标题栏与边框。
