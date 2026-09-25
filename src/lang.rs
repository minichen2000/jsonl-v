//! 界面语言：中文 / English。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn from_code(s: &str) -> Self {
        match s {
            "en" => Lang::En,
            _ => Lang::Zh,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }
}

pub struct T {
    // 菜单
    pub menu_file: &'static str,
    pub menu_settings: &'static str,
    pub menu_help: &'static str,
    pub open_file: &'static str,
    pub recent_files: &'static str,
    pub recent_empty: &'static str,
    pub recent_clear: &'static str,
    pub reload_file: &'static str,
    pub quit: &'static str,
    pub font_size: &'static str,
    pub dark_theme: &'static str,
    pub shell_registered: &'static str,
    pub shell_register: &'static str,
    pub shell_open_with: &'static str,
    pub shell_checking: &'static str,
    pub open_config_dir: &'static str,
    pub shortcuts: &'static str,
    pub about: &'static str,
    pub language: &'static str,
    // 搜索
    pub search_hint: &'static str,
    pub case_sensitive_tip: &'static str,
    pub only_matches: &'static str,
    // 行列表 / 右键
    pub copy_raw: &'static str,
    pub copy_pretty: &'static str,
    pub view_text_line: &'static str,
    // 详情
    pub tab_tree: &'static str,
    pub tab_pretty: &'static str,
    pub tab_raw: &'static str,
    pub tab_rebuild: &'static str,
    pub expand_all: &'static str,
    pub collapse_all: &'static str,
    pub json_parse_failed: &'static str,
    pub empty_hint: &'static str,
    pub view_text_btn: &'static str,
    // 状态栏
    pub no_file: &'static str,
    // 完整上下文（还原）
    pub think_label: &'static str,
    pub text_label: &'static str,
    pub sys_prompt_label: &'static str,
    pub jump_to_source: &'static str,
    pub request_json_btn: &'static str,
    pub request_json_failed: &'static str,
    // 文本查看窗口
    pub wrap_toggle: &'static str,
    pub copy_all: &'static str,
    // 弹窗
    pub about_title: &'static str,
    pub about_desc: &'static str,
    pub about_tech: &'static str,
    pub about_config: &'static str,
    pub shortcuts_title: &'static str,
    // 快捷键说明
    pub sc_open: &'static str,
    pub sc_reload: &'static str,
    pub sc_find: &'static str,
    pub sc_f3: &'static str,
    pub sc_arrows: &'static str,
    pub sc_page: &'static str,
    pub sc_copy: &'static str,
    // 事件过滤
    pub filter_all: &'static str,
    pub filter_other: &'static str,
}

const ZH: T = T {
    menu_file: "文件",
    menu_settings: "设置",
    menu_help: "帮助",
    open_file: "📂 打开文件  Ctrl+O",
    recent_files: "🕘 最近的文件",
    recent_empty: "（空）",
    recent_clear: "清除记录",
    reload_file: "🔄 重新加载  F5",
    quit: "退出",
    font_size: "字体大小",
    dark_theme: "深色主题",
    shell_registered: "✔ 已注册资源管理器右键菜单（点击取消）",
    shell_register: "注册到资源管理器右键菜单",
    shell_open_with: "用 jsonl-v 打开",
    shell_checking: "右键菜单：检测中…",
    open_config_dir: "打开配置文件所在目录",
    shortcuts: "⌨ 快捷键",
    about: "ℹ 关于",
    language: "语言 / Language",
    search_hint: "搜索 (Ctrl+F)",
    case_sensitive_tip: "区分大小写",
    only_matches: "仅看匹配",
    copy_raw: "复制原始行",
    copy_pretty: "复制美化文本",
    view_text_line: "📄 纯文本查看该行",
    tab_tree: "🌲 树视图",
    tab_pretty: "✨ 美化文本",
    tab_raw: "📄 原始行",
    tab_rebuild: "🧩 完整上下文（还原）",
    expand_all: "全展开",
    collapse_all: "全折叠",
    json_parse_failed: "JSON 解析失败",
    empty_hint: "Ctrl+O 打开 JSONL 文件，或将文件拖入窗口",
    view_text_btn: "📄纯文本查看",
    no_file: "未打开文件",
    think_label: "💭 think",
    text_label: "📝 text",
    sys_prompt_label: "🧾 系统提示词",
    jump_to_source: "跳转到源行",
    request_json_btn: "📦 请求体 JSON",
    request_json_failed: "请求体重建失败",
    wrap_toggle: "自动换行",
    copy_all: "复制全部",
    about_title: "ℹ 关于 jsonl-v",
    about_desc: "JSONL 文件查看器，对 Kimi Code wire.jsonl 会话日志增强展示。",
    about_tech: "单文件 exe · Rust + egui",
    about_config: "配置文件",
    shortcuts_title: "⌨ 快捷键",
    sc_open: "打开文件",
    sc_reload: "重新加载文件",
    sc_find: "聚焦搜索框",
    sc_f3: "下一个 / 上一个搜索命中",
    sc_arrows: "移动选中行",
    sc_page: "翻页移动选中行",
    sc_copy: "复制当前行（美化文本）",
    filter_all: "全部事件",
    filter_other: "其他",
};

const EN: T = T {
    menu_file: "File",
    menu_settings: "Settings",
    menu_help: "Help",
    open_file: "📂 Open File  Ctrl+O",
    recent_files: "🕘 Recent Files",
    recent_empty: "(empty)",
    recent_clear: "Clear",
    reload_file: "🔄 Reload  F5",
    quit: "Quit",
    font_size: "Font size",
    dark_theme: "Dark theme",
    shell_registered: "✔ Explorer context menu registered (click to remove)",
    shell_register: "Register Explorer context menu",
    shell_open_with: "Open with jsonl-v",
    shell_checking: "Context menu: checking…",
    open_config_dir: "Open config file folder",
    shortcuts: "⌨ Shortcuts",
    about: "ℹ About",
    language: "Language / 语言",
    search_hint: "Search (Ctrl+F)",
    case_sensitive_tip: "Case sensitive",
    only_matches: "Matches only",
    copy_raw: "Copy raw line",
    copy_pretty: "Copy pretty-printed",
    view_text_line: "📄 View line as text",
    tab_tree: "🌲 Tree",
    tab_pretty: "✨ Pretty",
    tab_raw: "📄 Raw",
    tab_rebuild: "🧩 Full Context (Rebuilt)",
    expand_all: "Expand all",
    collapse_all: "Collapse all",
    json_parse_failed: "JSON parse failed",
    empty_hint: "Ctrl+O to open a JSONL file, or drag one into the window",
    view_text_btn: "📄 View as text",
    no_file: "No file opened",
    think_label: "💭 think",
    text_label: "📝 text",
    sys_prompt_label: "🧾 System Prompt",
    jump_to_source: "Jump to source line",
    request_json_btn: "📦 Request Body JSON",
    request_json_failed: "Failed to rebuild request body",
    wrap_toggle: "Word wrap",
    copy_all: "Copy all",
    about_title: "ℹ About jsonl-v",
    about_desc: "A JSONL file viewer with enhanced support for Kimi Code wire.jsonl session logs.",
    about_tech: "Single-file exe · Rust + egui",
    about_config: "Config file",
    shortcuts_title: "⌨ Shortcuts",
    sc_open: "Open file",
    sc_reload: "Reload file",
    sc_find: "Focus search box",
    sc_f3: "Next / previous search hit",
    sc_arrows: "Move selection",
    sc_page: "Move selection by page",
    sc_copy: "Copy current line (pretty)",
    filter_all: "All events",
    filter_other: "Other",
};

pub fn tr(lang: Lang) -> &'static T {
    match lang {
        Lang::Zh => &ZH,
        Lang::En => &EN,
    }
}

/// 带参数的界面文案
impl T {
    pub fn opened(&self, path: &std::path::Path, lines: usize, size: String) -> String {
        match self.menu_file {
            "文件" => format!("已打开 {}（{lines} 行，{size}）", path.display()),
            _ => format!("Opened {} ({lines} lines, {size})", path.display()),
        }
    }
    pub fn open_failed(&self, e: std::io::Error) -> String {
        match self.menu_file {
            "文件" => format!("打开失败: {e}"),
            _ => format!("Open failed: {e}"),
        }
    }
    pub fn reloaded(&self, lines: usize) -> String {
        match self.menu_file {
            "文件" => format!("已重载（{lines} 行）"),
            _ => format!("Reloaded ({lines} lines)"),
        }
    }
    pub fn no_change(&self) -> String {
        match self.menu_file {
            "文件" => "文件无变化".into(),
            _ => "No changes".into(),
        }
    }
    pub fn reload_failed(&self, e: std::io::Error) -> String {
        match self.menu_file {
            "文件" => format!("重载失败: {e}"),
            _ => format!("Reload failed: {e}"),
        }
    }
    pub fn search_done(&self, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("搜索完成：{n} 个命中行"),
            _ => format!("Search done: {n} hit(s)"),
        }
    }
    pub fn matches_label(&self, n: usize, searching: bool) -> String {
        match (self.menu_file, searching) {
            ("文件", true) => format!("{n} 命中…"),
            ("文件", false) => format!("{n} 命中"),
            (_, true) => format!("{n} hits…"),
            (_, false) => format!("{n} hits"),
        }
    }
    pub fn copied(&self, line: usize, pretty: bool) -> String {
        match (self.menu_file, pretty) {
            ("文件", true) => format!("已复制第 {line} 行（美化）"),
            ("文件", false) => format!("已复制第 {line} 行（原始）"),
            (_, true) => format!("Copied line {line} (pretty)"),
            (_, false) => format!("Copied line {line} (raw)"),
        }
    }
    pub fn filter_status(&self, label: &str, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("事件过滤：{label} 命中 {n} 行"),
            _ => format!("Filter: {label} → {n} lines"),
        }
    }
    pub fn shell_registered_ok(&self) -> String {
        match self.menu_file {
            "文件" => "已注册：右键任意文件 → 用 jsonl-v 打开".into(),
            _ => "Registered: right-click any file → Open with jsonl-v".into(),
        }
    }
    pub fn shell_unregistered_ok(&self) -> String {
        match self.menu_file {
            "文件" => "已取消资源管理器右键菜单".into(),
            _ => "Explorer context menu removed".into(),
        }
    }
    pub fn shell_failed(&self, e: String, register: bool) -> String {
        match (self.menu_file, register) {
            ("文件", true) => format!("注册失败: {e}"),
            ("文件", false) => format!("取消注册失败: {e}"),
            (_, true) => format!("Register failed: {e}"),
            (_, false) => format!("Unregister failed: {e}"),
        }
    }
    pub fn status_lines(&self, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("{n} 行"),
            _ => format!("{n} lines"),
        }
    }
    pub fn current_line(&self, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("当前第 {n} 行"),
            _ => format!("Line {n}"),
        }
    }
    pub fn wire_mode(&self, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("wire 模式: {n} 次请求"),
            _ => format!("wire mode: {n} requests"),
        }
    }
    pub fn search_status(&self, n: usize, done: bool) -> String {
        let suffix = if done { "" } else { "…" };
        match self.menu_file {
            "文件" => format!("搜索: {n} 命中{suffix}"),
            _ => format!("Search: {n} hits{suffix}"),
        }
    }
    pub fn timeline_title(&self, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("⚡ 请求时间线（{n} 次）"),
            _ => format!("⚡ Request timeline ({n})"),
        }
    }
    pub fn line_label(&self, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("第 {n} 行"),
            _ => format!("Line {n}"),
        }
    }
    pub fn rebuilt_count(&self, n: u64) -> String {
        match self.menu_file {
            "文件" => format!("还原消息数: {n}"),
            _ => format!("Rebuilt messages: {n}"),
        }
    }
    pub fn context_bytes(&self, size: String) -> String {
        match self.menu_file {
            "文件" => format!("估算大小: {size}"),
            _ => format!("Estimated size: {size}"),
        }
    }
    pub fn line_raw_title(&self, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("第 {n} 行原始文本"),
            _ => format!("Line {n} raw text"),
        }
    }
    pub fn msg_title(&self, line: usize, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("L{line} 消息#{n}"),
            _ => format!("L{line} msg #{n}"),
        }
    }
    pub fn tool_args_title(&self, line: usize, name: &str) -> String {
        match self.menu_file {
            "文件" => format!("L{line} {name} 参数"),
            _ => format!("L{line} {name} args"),
        }
    }
    pub fn tool_result_title(&self, line: usize) -> String {
        match self.menu_file {
            "文件" => format!("L{line} 工具结果"),
            _ => format!("L{line} tool result"),
        }
    }
    pub fn injection_tag(&self) -> &'static str {
        match self.menu_file {
            "文件" => " [注入]",
            _ => " [injected]",
        }
    }
    pub fn tools_def_label(&self, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("🛠 工具定义（{n} 个工具）"),
            _ => format!("🛠 Tool Definitions ({n} tools)"),
        }
    }
    pub fn sys_prompt_title(&self, line: usize) -> String {
        match self.menu_file {
            "文件" => format!("系统提示词（源: 第 {line} 行 profile.bind）"),
            _ => format!("System Prompt (source: line {line} profile.bind)"),
        }
    }
    pub fn tools_def_title(&self, line: usize) -> String {
        match self.menu_file {
            "文件" => format!("工具定义（源: 第 {line} 行 llm.tools_snapshot）"),
            _ => format!("Tool Definitions (source: line {line} llm.tools_snapshot)"),
        }
    }
    pub fn text_stats(&self, lines: usize, chars: usize) -> String {
        match self.menu_file {
            "文件" => format!("{lines} 行 | {chars} 字符"),
            _ => format!("{lines} lines | {chars} chars"),
        }
    }
    pub fn request_json_title(&self, line: usize, turn_step: &str) -> String {
        match self.menu_file {
            "文件" => format!("L{line} 请求体 (turnStep {turn_step})"),
            _ => format!("L{line} request body (turnStep {turn_step})"),
        }
    }
    pub fn json_view_size(&self, size: String) -> String {
        match self.menu_file {
            "文件" => format!("约 {size}"),
            _ => format!("~{size}"),
        }
    }
    pub fn n_items(&self, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("{n} 项"),
            _ => format!("{n} items"),
        }
    }
    pub fn n_keys(&self, n: usize) -> String {
        match self.menu_file {
            "文件" => format!("{n} 键"),
            _ => format!("{n} keys"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_codes_roundtrip() {
        assert_eq!(Lang::from_code("zh"), Lang::Zh);
        assert_eq!(Lang::from_code("en"), Lang::En);
        assert_eq!(Lang::from_code("anything"), Lang::Zh);
        assert_eq!(Lang::Zh.code(), "zh");
        assert_eq!(Lang::En.code(), "en");
    }

    #[test]
    fn all_strings_non_empty() {
        for lang in [Lang::Zh, Lang::En] {
            let t = tr(lang);
            assert!(!t.menu_file.is_empty());
            assert!(!t.open_file.is_empty());
            assert!(!t.tab_tree.is_empty());
            assert!(!t.expand_all.is_empty());
            assert!(!t.about_desc.is_empty());
        }
    }
}
