# jsonl-v 项目说明（供 AI 助手参考）

## 仓库与远程

- Gitee: https://gitee.com/minichen2000/jsonl-v （远程名 `origin`）
- GitHub: https://github.com/minichen2000/jsonl-v （远程名 `github`）
- 每次改动提交后双推：`git push origin main && git push github main`

## 构建与发布（无 CI，全本地）

- 不使用 GitHub Actions；每次改动后本地构建验证：`cargo build --release --locked`，产物 `target/release/jsonl-v.exe`
- 只发布 Windows 绿色单 exe
- 发版流程：
  1. 更新 `Cargo.toml` 的 `version`
  2. 提交并双推 main；打 tag 并双推：`git tag vX.Y.Z && git push origin vX.Y.Z && git push github vX.Y.Z`
  3. 本地构建后上传绿色 exe：
     `gh release create vX.Y.Z jsonl-v-X.Y.Z-windows-x86_64.exe -R minichen2000/jsonl-v --title "jsonl-v vX.Y.Z" --notes "..."`

## 技术栈

- 纯 Rust + egui，单文件 exe
- Windows 下图标由 `build.rs` 内嵌（`#[cfg(windows)]` 门控；`assets/icon.ico` 缺失时跳过）
- 本机访问 github.com 依赖 hosts 条目 `140.82.112.3 github.com`（网络间歇性干扰，失败时重试即可）
