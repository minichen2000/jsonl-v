# jsonl-v 构建说明书

[English](BUILDING.md) | 简体中文

从 0 开始在一台新机器上构建 jsonl-v。

## 产物形态

单个 exe 文件（Windows 下约 4.5MB），**无任何运行时依赖**：
不需要 .NET、不需要 VC++ 运行库、不需要 Java、不需要安装任何东西，拷贝 exe 双击即用。

## 唯一前置依赖：Rust 工具链

构建只需要 Rust（stable）：

```powershell
# Windows（二选一）
winget install Rustlang.Rustup
# 或下载运行 https://rustup.rs 的 rustup-init.exe

rustup default stable
```

> Windows 上 rustc 默认使用 MSVC 链接器，需要 Visual Studio Build Tools
> （勾选「使用 C++ 的桌面开发」工作负荷）。rustup 安装时如果检测到缺失会提示。
>
> exe 图标内嵌需要 Windows SDK 自带的 `rc.exe`（随上面的 Build Tools 一起装）。
> 如果没有 rc.exe，构建会在图标步骤报错；此时把 `assets/icon.ico` 移走即可跳过
> （`build.rs` 检测到图标文件不存在会自动跳过，程序功能不受影响）。

## 构建

```bash
git clone <仓库地址> jsonl-v   # 或直接把源码目录拷过去
cd jsonl-v
cargo build --release
```

产物：`target/release/jsonl-v.exe`

> 国内网络如果拉取 crates.io 依赖超时：仓库已自带 `.cargo/config.toml`，
> 配置了 rsproxy 国内镜像源，开箱即用。海外网络好可以删掉它。

## Rust 依赖（cargo 自动下载，无需手动安装）

| 依赖 | 用途 |
|---|---|
| eframe / egui 0.31 | GUI 框架（纯 Rust，无系统依赖） |
| serde_json 1 | JSON 解析 |
| memchr 2 | SIMD 加速切行 |
| rfd 0.15 | 系统文件打开对话框 |
| sys-locale 0.3 | 系统语言检测（决定默认界面语言） |
| embed-resource 2（仅构建期，仅 Windows） | 把 assets/icon.ico 嵌进 exe |

运行期不依赖任何 DLL / 外部命令（注册表右键菜单功能调系统自带 reg.exe，不用也可正常使用）。

## 其他常用命令

```bash
cargo test                        # 37 个单元测试
cargo test -- --ignored           # 加跑 100MB 大文件性能测试
cargo run -- docs/wire.jsonl      # 直接跑 debug 版并打开样例
cargo run --example make_icon     # 重新生成 assets/icon.ico（改图标后）
```

## 跨平台

macOS / Linux 上同一份代码可直接 `cargo build --release`：
- 中文字体回退列表已包含 Linux/macOS 常见路径
- 资源管理器右键菜单注册是 Windows 专属功能，其他平台自动降级为不可用提示
- exe 图标内嵌仅 Windows 生效

## 目录结构

```
jsonl-v/
├── Cargo.toml
├── README.md           # 使用说明
├── README.zh-CN.md     # 使用说明（中文）
├── BUILDING.md         # 构建说明（英文）
├── BUILDING.zh-CN.md   # 本文档
├── build.rs              # 仅 Windows：内嵌 exe 图标
├── assets/
│   ├── icon.rc           # 资源脚本
│   └── icon.ico          # 由 examples/make_icon.rs 生成
├── docs/
│   ├── jsonl-viewer-spec.md  # 原始开发规格
│   └── wire.jsonl            # 测试样例
├── examples/make_icon.rs # 图标生成器
└── src/
    ├── main.rs           # 入口：窗口、图标、中文字体加载
    ├── app.rs            # 界面状态机：菜单/三栏布局/快捷键
    ├── document.rs       # JSONL 切行索引 + 懒解析 + LRU 缓存
    ├── json_tree.rs      # JSON 可折叠树渲染
    ├── text_view.rs      # 长文本解转义查看窗口
    ├── search.rs         # 后台线程全文搜索
    ├── wire.rs           # wire.jsonl 增强（时间线/上下文重建/着色）
    ├── settings.rs       # 设置与最近文件持久化
    ├── shell_menu.rs     # 资源管理器右键菜单注册（Windows）
    ├── lang.rs           # 中英文界面文案
    └── icon.rs           # 程序图标像素绘制
```
