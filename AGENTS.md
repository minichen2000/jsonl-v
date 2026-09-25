# jsonl-v 项目说明（供 AI 助手参考）

## 仓库与远程

- Gitee: https://gitee.com/minichen2000/jsonl-v （远程名 `origin`）
- GitHub: https://github.com/minichen2000/jsonl-v （远程名 `github`）
- 日常推送需同时推两个远程：`git push origin <branch>` 和 `git push github <branch>`

## 发布流程

1. 更新版本号：`Cargo.toml` 的 `version`
2. 提交并打 tag：`git tag vX.Y.Z`
3. 推送 tag 到两个远程：`git push origin vX.Y.Z && git push github vX.Y.Z`
4. 推到 GitHub 的 `v*` tag 会触发 `.github/workflows/release.yml`，自动构建并发布 Release：
   - Windows: `jsonl-v-<tag>-windows-x86_64.zip`
   - macOS: `jsonl-v-<tag>-macos-aarch64.tar.gz`
   - Linux: `jsonl-v-<tag>-linux-x86_64.tar.gz`

## 技术栈

- 纯 Rust + egui，单文件 exe；构建用 `cargo build --release --locked`
- Windows 下图标由 `build.rs` 内嵌（`assets/icon.ico` 缺失时自动跳过）
- 本机访问 github.com 依赖 hosts 条目 `140.82.112.3 github.com`（网络间歇性干扰，失败时重试即可）
