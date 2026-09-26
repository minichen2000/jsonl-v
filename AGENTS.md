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
