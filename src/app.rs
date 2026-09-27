use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use egui::{Align2, Color32, Context, FontFamily, FontId, Key, Modifiers, RichText, Ui};

use crate::document::JsonlDocument;
use crate::json_tree::{self, TreeAction};
use crate::json_view::JsonViewWindow;
use crate::lang::{self, Lang, T};
use crate::search::{Query, SearchHandle, SearchMsg};
use crate::settings::Settings;
use crate::shell_menu;
use crate::text_view::{is_long_text, TextViewWindow};
use crate::wire::{self, CtxItem, RequestEntry, Side, WireKind};

const SEARCH_DEBOUNCE_MS: u128 = 200;

const KIND_REQUEST: Color32 = Color32::from_rgb(0xee, 0x99, 0x28); // 黄
const KIND_TOOL_CALL: Color32 = Color32::from_rgb(0x61, 0xaf, 0xef); // 蓝
const KIND_TOOL_RESULT: Color32 = Color32::from_rgb(0x98, 0xc3, 0x79); // 绿
const KIND_THINK: Color32 = Color32::from_rgb(0xc6, 0xa0, 0xf6); // 紫
const KIND_TEXT: Color32 = Color32::from_rgb(0x9e, 0x9e, 0x9e); // 灰
const KIND_USAGE: Color32 = Color32::from_rgb(0x56, 0xc2, 0xd6); // 青
const KIND_INTERACTION: Color32 = Color32::from_rgb(0xd1, 0x9a, 0x66); // 橙
const BAD_LINE: Color32 = Color32::from_rgb(0xe0, 0x6c, 0x75); // 红

/// 粗体 emoji 字体族名（在 main.rs 注册）：分侧徽标图标用粗线条版本
pub const EMOJI_BOLD_FAMILY: &str = "emoji-bold";
const SIZE_KB: Color32 = Color32::from_rgb(0xee, 0x99, 0x28); // KB 标橙
const SIZE_BIG: Color32 = Color32::from_rgb(0xe0, 0x6c, 0x75); // >100KB 标红

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Tree,
    Pretty,
    Raw,
    Rebuild,
}

/// 重建上下文时按来源侧统计的条目数与字节数
#[derive(Clone, Copy, Default)]
struct SideStat {
    host_n: usize,
    host_bytes: usize,
    llm_n: usize,
    llm_bytes: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EventFilter {
    All,
    Requests,
    ToolCalls,
    ToolResults,
    Think,
    Text,
    Usage,
    Interactions,
    ContextMsgs,
    Other,
}

impl EventFilter {
    const ALL: [EventFilter; 10] = [
        EventFilter::All,
        EventFilter::Requests,
        EventFilter::ToolCalls,
        EventFilter::ToolResults,
        EventFilter::Think,
        EventFilter::Text,
        EventFilter::Usage,
        EventFilter::Interactions,
        EventFilter::ContextMsgs,
        EventFilter::Other,
    ];

    fn label(self, t: &T) -> String {
        match self {
            EventFilter::All => t.filter_all.to_string(),
            EventFilter::Requests => "⚡ llm.request".into(),
            EventFilter::ToolCalls => "🔧 tool.call".into(),
            EventFilter::ToolResults => "↩ tool.result".into(),
            EventFilter::Think => "💭 think".into(),
            EventFilter::Text => "📝 text".into(),
            EventFilter::Usage => "📊 usage.record".into(),
            EventFilter::Interactions => "✋ interaction".into(),
            EventFilter::ContextMsgs => "📥 context.*".into(),
            EventFilter::Other => t.filter_other.to_string(),
        }
    }
}

enum RowAction {
    CopyRaw(usize),
    CopyPretty(usize),
    ViewText(usize),
}

pub struct JsonlApp {
    doc: Option<JsonlDocument>,
    selected: Option<usize>,
    // 搜索
    search_text: String,
    case_sensitive: bool,
    matches: Vec<usize>,
    search_done: bool,
    search_handle: Option<SearchHandle>,
    last_edit: Option<Instant>,
    only_matches: bool,
    focus_search_next_frame: bool,
    // wire
    is_wire: bool,
    timeline: Vec<RequestEntry>,
    event_filter: EventFilter,
    filter_indices: Option<Vec<usize>>,
    // 行可见集（过滤 + 仅看匹配后的结果），dirty 时重建
    visible: Vec<usize>,
    visible_dirty: bool,
    pending_scroll_row: Option<usize>,
    last_row_range: Option<Range<usize>>,
    // 详情
    tab: DetailTab,
    rebuild_cache: Option<(usize, Vec<CtxItem>, u64, usize, SideStat)>,
    // 重建页签：阅读游标 request_line → 条目序号（单行标记，点别的行移过去，再点取消）
    rebuild_cursor: std::collections::HashMap<usize, usize>,
    // 重建页签内搜索：只搜界面可见文本（长文本只搜截断预览）
    rebuild_search: String,
    rebuild_case: bool,
    rebuild_matches: Vec<wire::RebuildMatch>,
    rebuild_match_key: Option<(usize, String, bool)>,
    rebuild_cur: Option<usize>,
    // 重建页签滚动区：上一帧视口矩形（悬停判定）+ 待应用的键盘滚动量
    rebuild_scroll_rect: Option<egui::Rect>,
    rebuild_key_scroll: f32,
    // 长文本窗口
    text_windows: Vec<TextViewWindow>,
    // 结构化 JSON 窗口（请求体重建）
    json_windows: Vec<JsonViewWindow>,
    next_win_id: usize,
    // 设置与弹窗
    settings: Settings,
    applied_font_size: Option<f32>,
    shell_registered: Option<bool>, // 后台线程查询，绝不阻塞 UI
    shell_reg_rx: Option<std::sync::mpsc::Receiver<bool>>,
    show_about: bool,
    show_shortcuts: bool,
    status: String,
    // 详情面板缓存：选中行的解析结果，避免每帧克隆大 JSON
    detail_cache: Option<(usize, Result<serde_json::Value, String>, String, String)>,
    // 树视图全展开/全折叠
    tree_default_open: Option<bool>,
    tree_gen: u64,
    // 启动时主窗口尺寸/位置修正（一次性）
    startup_fit: bool,
}

impl JsonlApp {
    pub fn new(initial_path: Option<PathBuf>) -> Self {
        let mut app = Self {
            doc: None,
            selected: None,
            search_text: String::new(),
            case_sensitive: false,
            matches: Vec::new(),
            search_done: true,
            search_handle: None,
            last_edit: None,
            only_matches: false,
            focus_search_next_frame: false,
            is_wire: false,
            timeline: Vec::new(),
            event_filter: EventFilter::All,
            filter_indices: None,
            visible: Vec::new(),
            visible_dirty: true,
            pending_scroll_row: None,
            last_row_range: None,
            tab: DetailTab::Tree,
            rebuild_cache: None,
            rebuild_cursor: std::collections::HashMap::new(),
            rebuild_search: String::new(),
            rebuild_case: false,
            rebuild_matches: Vec::new(),
            rebuild_match_key: None,
            rebuild_cur: None,
            rebuild_scroll_rect: None,
            rebuild_key_scroll: 0.0,
            text_windows: Vec::new(),
            json_windows: Vec::new(),
            next_win_id: 0,
            settings: Settings::load(),
            applied_font_size: None,
            shell_registered: None,
            shell_reg_rx: Some(shell_menu::is_registered_async()),
            show_about: false,
            show_shortcuts: false,
            status: String::new(),
            detail_cache: None,
            tree_default_open: None,
            tree_gen: 0,
            startup_fit: false,
        };
        if let Some(p) = initial_path {
            app.open_path(p);
        }
        app
    }

    fn t(&self) -> &'static T {
        lang::tr(Lang::from_code(&self.settings.lang))
    }

    // 字号派生：列表基准 font_size，详情 +1，行标题 +2
    fn fs_list(&self) -> f32 {
        self.settings.font_size
    }
    fn fs_detail(&self) -> f32 {
        self.settings.font_size + 1.0
    }
    fn row_h(&self) -> f32 {
        self.settings.font_size + 8.0
    }

    /// 把标准控件（菜单/按钮/标签）的文字样式也跟随字号
    fn apply_font_size(&mut self, ctx: &Context) {
        if self.applied_font_size == Some(self.settings.font_size) {
            return;
        }
        self.applied_font_size = Some(self.settings.font_size);
        let fs = self.settings.font_size;
        let mut style = (*ctx.style()).clone();
        use egui::TextStyle::*;
        style
            .text_styles
            .insert(Body, FontId::new(fs, FontFamily::Proportional));
        style
            .text_styles
            .insert(Button, FontId::new(fs, FontFamily::Proportional));
        style
            .text_styles
            .insert(Small, FontId::new(fs - 2.0, FontFamily::Proportional));
        style
            .text_styles
            .insert(Monospace, FontId::new(fs, FontFamily::Monospace));
        style
            .text_styles
            .insert(Heading, FontId::new(fs + 4.0, FontFamily::Proportional));
        ctx.set_style(style);
    }

    /// 启动时把主窗口按显示器逻辑尺寸收紧并居中（略偏上），一次性。
    /// 默认 1280x800 是逻辑尺寸：1080p 屏 125%/150% 缩放时逻辑高只有 864/720，
    /// 加标题栏与任务栏后底边会出屏。注意 `ctx.screen_rect()` 是窗口自身客户区，
    /// 显示器尺寸要用 `ViewportInfo::monitor_size`（头一两帧可能还没就绪）。
    fn fit_main_window_at_startup(&mut self, ctx: &Context) {
        if self.startup_fit {
            return;
        }
        let Some(mon) = ctx.input(|i| i.viewport().monitor_size) else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        };
        self.startup_fit = true;
        let want = egui::vec2(1280.0, 800.0);
        // 竖向多留量：标题栏 ~30 + 任务栏 ~48×2——窗口显示在任务栏上方时
        // 底边再留一个任务栏高度才不贴底，中心再上浮 16 让视觉居中
        let size = egui::vec2(
            want.x.min(mon.x - 40.0).max(640.0),
            want.y.min(mon.y - 188.0).max(400.0),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        let pos = egui::pos2(
            ((mon.x - size.x) / 2.0).max(0.0),
            ((mon.y - size.y) / 2.0 - 16.0).max(0.0),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
    }

    fn open_path(&mut self, path: PathBuf) {
        match JsonlDocument::open(path.clone()) {
            Ok(doc) => {
                let lines = doc.line_count();
                let bytes = doc.total_bytes();
                self.is_wire = wire::is_wire_file(&doc);
                self.timeline = if self.is_wire {
                    wire::timeline(&doc)
                } else {
                    Vec::new()
                };
                self.doc = Some(doc);
                self.selected = if lines > 0 { Some(0) } else { None };
                self.event_filter = EventFilter::All;
                self.filter_indices = None;
                self.rebuild_cache = None;
                self.clear_rebuild_view();
                self.detail_cache = None;
                self.tab = DetailTab::Tree;
                self.visible_dirty = true;
                self.pending_scroll_row = Some(0);
                self.status = self.t().opened(&path, lines, fmt_bytes(bytes));
                self.settings.push_recent(path);
                self.settings.save();
                self.restart_search();
            }
            Err(e) => {
                self.status = self.t().open_failed(e);
            }
        }
    }

    fn reload(&mut self) {
        let Some(doc) = &mut self.doc else { return };
        match doc.reload() {
            Ok(true) => {
                let lines = doc.line_count();
                self.is_wire = wire::is_wire_file(doc);
                self.timeline = if self.is_wire {
                    wire::timeline(doc)
                } else {
                    Vec::new()
                };
                self.rebuild_cache = None;
                self.clear_rebuild_view();
                self.detail_cache = None;
                if let Some(sel) = self.selected {
                    if sel >= lines {
                        self.selected = lines.checked_sub(1);
                    }
                }
                self.visible_dirty = true;
                self.status = self.t().reloaded(lines);
                self.restart_search();
            }
            Ok(false) => self.status = self.t().no_change(),
            Err(e) => self.status = self.t().reload_failed(e),
        }
    }

    /// 换文件/重载时清空重建页签的视图状态（阅读游标、页签内搜索）
    fn clear_rebuild_view(&mut self) {
        self.rebuild_cursor.clear();
        self.rebuild_search.clear();
        self.rebuild_matches.clear();
        self.rebuild_match_key = None;
        self.rebuild_cur = None;
    }

    // ---- 搜索 ----

    fn restart_search(&mut self) {
        self.search_handle = None;
        self.matches.clear();
        self.visible_dirty = true;
        let Some(doc) = &self.doc else { return };
        let Some(query) = Query::parse(&self.search_text, self.case_sensitive) else {
            self.search_done = true;
            return;
        };
        self.search_done = false;
        self.search_handle = Some(SearchHandle::start(doc.snapshot(), query));
    }

    fn poll_search(&mut self) {
        // 防抖：编辑后等 200ms 再启动
        if let Some(t) = self.last_edit {
            if t.elapsed().as_millis() >= SEARCH_DEBOUNCE_MS {
                self.last_edit = None;
                self.restart_search();
            }
        }
        let Some(handle) = &self.search_handle else { return };
        let mut got = false;
        let mut done = false;
        while let Ok(msg) = handle.rx.try_recv() {
            match msg {
                SearchMsg::Hit(i) => {
                    self.matches.push(i);
                    got = true;
                }
                SearchMsg::Done => done = true,
            }
        }
        if got {
            self.visible_dirty = true;
        }
        if done {
            self.search_done = true;
            self.search_handle = None;
            let n = self.matches.len();
            if !self.search_text.trim().is_empty() {
                self.status = self.t().search_done(n);
            }
        }
    }

    fn jump_match(&mut self, forward: bool) {
        if self.matches.is_empty() {
            return;
        }
        let cur = self.selected.unwrap_or(0);
        let target = if forward {
            self.matches
                .iter()
                .find(|&&m| m > cur)
                .or(self.matches.first())
        } else {
            self.matches
                .iter()
                .rev()
                .find(|&&m| m < cur)
                .or(self.matches.last())
        };
        if let Some(&m) = target {
            self.select_line(m, true);
        }
    }

    // ---- 行选择 / 可见集 / 滚动 ----

    /// scroll=true 时若目标行不在可见范围内才滚动列表（点击传 false，列表不动）
    fn select_line(&mut self, line: usize, scroll: bool) {
        self.selected = Some(line);
        if scroll {
            self.scroll_line_into_view(line);
        }
        if self.tab == DetailTab::Rebuild
            && self.rebuild_cache.as_ref().map(|(l, ..)| *l) != Some(line)
        {
            self.rebuild_cache = None;
        }
    }

    fn scroll_line_into_view(&mut self, line: usize) {
        if let Some(row) = self.visible.iter().position(|&l| l == line) {
            let in_view = self
                .last_row_range
                .as_ref()
                .map(|r| r.contains(&row))
                .unwrap_or(false);
            if !in_view {
                self.pending_scroll_row = Some(row);
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.visible.is_empty() {
            return;
        }
        let cur_row = self
            .selected
            .and_then(|s| self.visible.iter().position(|&l| l == s))
            .unwrap_or(0) as isize;
        let next = (cur_row + delta).clamp(0, self.visible.len() as isize - 1) as usize;
        let line = self.visible[next];
        self.selected = Some(line);
        // 键盘导航：目标行滚出视野时才滚动，且尽量最少滚动
        let in_view = self
            .last_row_range
            .as_ref()
            .map(|r| r.contains(&next))
            .unwrap_or(false);
        if !in_view {
            self.pending_scroll_row = Some(next);
        }
        if self.tab == DetailTab::Rebuild {
            self.rebuild_cache = None;
        }
    }

    fn rebuild_visible(&mut self) {
        if !self.visible_dirty {
            return;
        }
        self.visible_dirty = false;
        let n = self.doc.as_ref().map(|d| d.line_count()).unwrap_or(0);
        let mut vis: Vec<usize> = match &self.filter_indices {
            Some(v) => v.clone(),
            None => (0..n).collect(),
        };
        if self.only_matches && !self.search_text.trim().is_empty() {
            // matches 与 vis 均升序，求交集
            let mut out = Vec::new();
            let (mut a, mut b) = (0usize, 0usize);
            while a < vis.len() && b < self.matches.len() {
                match vis[a].cmp(&self.matches[b]) {
                    std::cmp::Ordering::Less => a += 1,
                    std::cmp::Ordering::Greater => b += 1,
                    std::cmp::Ordering::Equal => {
                        out.push(vis[a]);
                        a += 1;
                        b += 1;
                    }
                }
            }
            vis = out;
        }
        self.visible = vis;
    }

    fn apply_event_filter(&mut self) {
        let Some(doc) = &self.doc else { return };
        if self.event_filter == EventFilter::All {
            self.filter_indices = None;
            self.visible_dirty = true;
            return;
        }
        let mut out = Vec::new();
        for i in 0..doc.line_count() {
            let info = doc.peek_info(i);
            let keep = match self.event_filter {
                EventFilter::All => true,
                EventFilter::Requests => wire::classify(&info.parsed) == WireKind::LlmRequest,
                EventFilter::ToolCalls => wire::classify(&info.parsed) == WireKind::ToolCall,
                EventFilter::ToolResults => wire::classify(&info.parsed) == WireKind::ToolResult,
                EventFilter::Think => wire::classify(&info.parsed) == WireKind::Think,
                EventFilter::Text => wire::classify(&info.parsed) == WireKind::Text,
                EventFilter::Usage => wire::classify(&info.parsed) == WireKind::UsageRecord,
                EventFilter::Interactions => {
                    wire::classify(&info.parsed) == WireKind::Interaction
                }
                EventFilter::ContextMsgs => info
                    .parsed
                    .as_ref()
                    .ok()
                    .and_then(|v| v.get("type"))
                    .and_then(|t| t.as_str())
                    .map(|t| t.starts_with("context."))
                    .unwrap_or(false),
                EventFilter::Other => {
                    let k = wire::classify(&info.parsed);
                    k == WireKind::NotWire
                        && !info
                            .parsed
                            .as_ref()
                            .ok()
                            .and_then(|v| v.get("type"))
                            .and_then(|t| t.as_str())
                            .map(|t| t.starts_with("context."))
                            .unwrap_or(false)
                }
            };
            if keep {
                out.push(i);
            }
        }
        let n = out.len();
        self.filter_indices = Some(out);
        self.visible_dirty = true;
        let label = self.event_filter.label(self.t());
        self.status = self.t().filter_status(&label, n);
    }

    // ---- 快捷键 ----

    fn handle_keys(&mut self, ctx: &Context) {
        // 有控件持有键盘焦点（搜索框、各文本编辑区）时全局快捷键让位，
        // 否则方向键、Ctrl+C 等会被这里消费掉，文本区收不到
        if ctx.wants_keyboard_input() {
            return;
        }
        let (open, reload, find, f3, shift_f3, copy, up, down, pgup, pgdn) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::CTRL, Key::O),
                i.consume_key(Modifiers::NONE, Key::F5),
                i.consume_key(Modifiers::CTRL, Key::F),
                i.consume_key(Modifiers::NONE, Key::F3),
                i.consume_key(Modifiers::SHIFT, Key::F3),
                i.consume_key(Modifiers::CTRL, Key::C),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::PageUp),
                i.consume_key(Modifiers::NONE, Key::PageDown),
            )
        });
        if open {
            self.open_dialog();
        }
        if reload {
            self.reload();
        }
        if find {
            self.focus_search_next_frame = true;
        }
        if f3 {
            self.jump_match(true);
        }
        if shift_f3 {
            self.jump_match(false);
        }
        if copy {
            self.copy_current_pretty(ctx);
        }
        // 指针悬停在「完整上下文」重建视图上时，方向键/翻页键滚动重建视图，
        // 而不是移动左侧行列表的选择（焦点在哪边，键就作用在哪边）
        let rebuild_hover = self.tab == DetailTab::Rebuild
            && self
                .rebuild_scroll_rect
                .is_some_and(|r| ctx.pointer_hover_pos().is_some_and(|p| r.contains(p)));
        if rebuild_hover {
            let step = self.row_h();
            let page = self
                .rebuild_scroll_rect
                .map(|r| r.height())
                .unwrap_or(400.0);
            if up {
                self.rebuild_key_scroll -= step;
            }
            if down {
                self.rebuild_key_scroll += step;
            }
            if pgup {
                self.rebuild_key_scroll -= page;
            }
            if pgdn {
                self.rebuild_key_scroll += page;
            }
        } else {
            if up {
                self.move_selection(-1);
            }
            if down {
                self.move_selection(1);
            }
            if pgup {
                self.move_selection(-30);
            }
            if pgdn {
                self.move_selection(30);
            }
        }
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("JSONL", &["jsonl", "log", "txt", "json"])
            .pick_file()
        {
            self.open_path(p);
        }
    }

    fn copy_current_pretty(&mut self, ctx: &Context) {
        let (Some(doc), Some(sel)) = (&mut self.doc, self.selected) else {
            return;
        };
        let raw = doc.raw_line(sel);
        let text = serde_json::from_str::<serde_json::Value>(raw)
            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| raw.to_string()))
            .unwrap_or_else(|_| raw.to_string());
        ctx.copy_text(text);
        self.status = self.t().copied(sel + 1, true);
    }

    // ---- 菜单栏与工具栏 ----

    fn menu_bar(&mut self, ctx: &Context) {
        let t = self.t();
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button(t.menu_file, |ui| {
                    if ui.button(t.open_file).clicked() {
                        ui.close_menu();
                        self.open_dialog();
                    }
                    let recent = self.settings.recent_files.clone();
                    ui.menu_button(t.recent_files, |ui| {
                        if recent.is_empty() {
                            ui.label(RichText::new(t.recent_empty).color(Color32::GRAY));
                        }
                        // 一行一路径不换行；超宽时省略前部（保留文件名），用满可用宽度
                        let max_w = ui.ctx().screen_rect().width() - 60.0;
                        let font_id = egui::TextStyle::Button.resolve(ui.style());
                        for p in &recent {
                            let full = p.display().to_string();
                            let label = fit_path_front(ui, &full, max_w, font_id.clone());
                            let truncated = label != full;
                            // 菜单 Ui 默认 wrap_mode=Wrap，必须显式 Extend 才不换行
                            let btn = egui::Button::new(label)
                                .wrap_mode(egui::TextWrapMode::Extend);
                            let resp = if truncated {
                                ui.add(btn).on_hover_text(&full)
                            } else {
                                ui.add(btn)
                            };
                            if resp.clicked() {
                                ui.close_menu();
                                self.open_path(p.clone());
                            }
                        }
                        if !recent.is_empty() {
                            ui.separator();
                            if ui.button(t.recent_clear).clicked() {
                                ui.close_menu();
                                self.settings.recent_files.clear();
                                self.settings.save();
                            }
                        }
                    });
                    if ui
                        .add_enabled(self.doc.is_some(), egui::Button::new(t.reload_file))
                        .clicked()
                    {
                        ui.close_menu();
                        self.reload();
                    }
                    ui.separator();
                    if ui.button(t.quit).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button(t.menu_settings, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(t.font_size);
                        if ui
                            .add(egui::Slider::new(&mut self.settings.font_size, 10.0..=24.0))
                            .changed()
                        {
                            self.settings.save();
                        }
                    });
                    let mut dark = self.settings.dark;
                    if ui.checkbox(&mut dark, t.dark_theme).changed() {
                        self.settings.dark = dark;
                        self.settings.save();
                    }
                    ui.horizontal(|ui| {
                        ui.label(t.language);
                        let mut lang = Lang::from_code(&self.settings.lang);
                        if ui
                            .selectable_value(&mut lang, Lang::Zh, "中文")
                            .changed()
                            || ui
                                .selectable_value(&mut lang, Lang::En, "English")
                                .changed()
                        {
                            self.settings.lang = lang.code().to_string();
                            self.settings.save();
                        }
                    });
                    ui.separator();
                    // 注册状态由启动时的后台线程查询；还没回来就显示检测中
                    match self.shell_registered {
                        None => {
                            ui.label(RichText::new(t.shell_checking).color(Color32::GRAY));
                        }
                        Some(true) => {
                            if ui.button(t.shell_registered).clicked() {
                                ui.close_menu();
                                self.status = match shell_menu::unregister() {
                                    Ok(()) => {
                                        self.shell_registered = Some(false);
                                        self.t().shell_unregistered_ok()
                                    }
                                    Err(e) => self.t().shell_failed(e, false),
                                };
                            }
                        }
                        Some(false) => {
                            if ui.button(t.shell_register).clicked() {
                                ui.close_menu();
                                self.status = match shell_menu::register(t.shell_open_with) {
                                    Ok(()) => {
                                        self.shell_registered = Some(true);
                                        self.t().shell_registered_ok()
                                    }
                                    Err(e) => self.t().shell_failed(e, true),
                                };
                            }
                        }
                    }
                    ui.separator();
                    if ui.button(t.open_config_dir).clicked() {
                        ui.close_menu();
                        self.settings.save(); // 确保文件存在
                        let path = crate::settings::config_path();
                        let _ = std::process::Command::new("explorer")
                            .arg(format!("/select,{}", path.display()))
                            .spawn();
                    }
                });

                ui.menu_button(t.menu_help, |ui| {
                    if ui.button(t.shortcuts).clicked() {
                        ui.close_menu();
                        self.show_shortcuts = true;
                    }
                    if ui.button(t.about).clicked() {
                        ui.close_menu();
                        self.show_about = true;
                    }
                });

                ui.separator();

                let search_ui = ui.add(
                    egui::TextEdit::singleline(&mut self.search_text)
                        .hint_text(t.search_hint)
                        .desired_width(200.0),
                );
                if self.focus_search_next_frame {
                    search_ui.request_focus();
                    self.focus_search_next_frame = false;
                }
                if search_ui.changed() {
                    self.last_edit = Some(Instant::now());
                }
                if ui
                    .selectable_label(self.case_sensitive, "Aa")
                    .on_hover_text(t.case_sensitive_tip)
                    .clicked()
                {
                    self.case_sensitive = !self.case_sensitive;
                    self.restart_search();
                }
                if !self.search_text.trim().is_empty() {
                    let label = t.matches_label(self.matches.len(), self.search_handle.is_some());
                    ui.label(RichText::new(label).color(Color32::GRAY));
                }
                if ui.checkbox(&mut self.only_matches, t.only_matches).changed() {
                    self.visible_dirty = true;
                }

                if self.is_wire {
                    ui.separator();
                    let cur = self.event_filter.label(t);
                    let mut chosen = self.event_filter;
                    egui::ComboBox::from_id_salt("event_filter")
                        .selected_text(cur)
                        .show_ui(ui, |ui| {
                            for f in EventFilter::ALL {
                                ui.selectable_value(&mut chosen, f, f.label(t));
                            }
                        });
                    if chosen != self.event_filter {
                        self.event_filter = chosen;
                        self.apply_event_filter();
                    }
                }

                if let Some(doc) = &self.doc {
                    // 右侧剩余宽度可能很窄：放不下时砍头部留尾部，避免与左侧控件重叠
                    let path = doc.path.display().to_string();
                    let shown = fit_text_tail(ui, &path, egui::TextStyle::Body);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(shown).color(Color32::GRAY));
                    });
                }
            });
        });
    }

    fn dialogs(&mut self, ctx: &Context) {
        let t = self.t();
        if self.show_shortcuts {
            let mut open = self.show_shortcuts;
            let title = RichText::new(t.shortcuts_title).size(self.fs_detail() + 1.0);
            egui::Window::new(title)
                .id(egui::Id::new("shortcuts_window"))
                .frame(popup_frame(ctx))
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    let keys = [
                        ("Ctrl+O", t.sc_open),
                        ("F5", t.sc_reload),
                        ("Ctrl+F", t.sc_find),
                        ("F3 / Shift+F3", t.sc_f3),
                        ("↑ / ↓", t.sc_arrows),
                        ("PgUp / PgDn", t.sc_page),
                        ("Ctrl+C", t.sc_copy),
                    ];
                    for (k, desc) in keys {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(k).font(FontId::new(self.fs_detail(), FontFamily::Monospace)),
                            );
                            ui.label(desc);
                        });
                    }
                });
            self.show_shortcuts = open;
        }
        if self.show_about {
            let mut open = self.show_about;
            let title = RichText::new(t.about_title).size(self.fs_detail() + 1.0);
            egui::Window::new(title)
                .id(egui::Id::new("about_window"))
                .frame(popup_frame(ctx))
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(RichText::new(format!("jsonl-v {}", env!("CARGO_PKG_VERSION"))).strong());
                    ui.label(t.about_desc);
                    ui.label(t.about_tech);
                    ui.label(t.about_license);
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "{}: {}",
                            t.about_config,
                            crate::settings::config_path().display()
                        ))
                        .small()
                        .color(Color32::GRAY),
                    );
                });
            self.show_about = open;
        }
    }

    // ---- 行列表 ----

    fn line_list_panel(&mut self, ctx: &Context) {
        let t = self.t();
        egui::SidePanel::left("lines")
            .default_width(480.0)
            .resizable(true)
            .show(ctx, |ui| {
                if self.is_wire && !self.timeline.is_empty() {
                    let n = self.timeline.len();
                    egui::CollapsingHeader::new(
                        RichText::new(t.timeline_title(n)).color(KIND_REQUEST),
                    )
                    .id_salt("timeline")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(180.0)
                            .auto_shrink([false, false]) // 撑满宽度，滚动条贴面板右缘
                            .show(ui, |ui| {
                                let timeline = self.timeline.clone();
                                let fs = self.fs_list();
                                for e in &timeline {
                                    let usage = e
                                        .usage
                                        .as_ref()
                                        .map(|u| {
                                            let creation = if u.cache_creation > 0 {
                                                format!(" +{}new", u.cache_creation)
                                            } else {
                                                String::new()
                                            };
                                            format!(
                                                " in={} out={} cache={}{}",
                                                u.input_other, u.output, u.cache_read, creation
                                            )
                                        })
                                        .unwrap_or_default();
                                    let label = format!(
                                        "#{} {} msgs={}{}",
                                        e.seq, e.turn_step, e.message_count, usage
                                    );
                                    if ui
                                        .selectable_label(
                                            self.selected == Some(e.line_idx),
                                            RichText::new(label)
                                                .font(FontId::new(fs, FontFamily::Monospace)),
                                        )
                                        .clicked()
                                    {
                                        self.select_line(e.line_idx, true);
                                    }
                                }
                            });
                    });
                    ui.separator();
                }

                self.rebuild_visible();
                // take 出来避免每帧克隆整个可见行向量，用完放回
                let rows = std::mem::take(&mut self.visible);
                let n_rows = rows.len();
                let row_h = self.row_h();
                let fs = self.fs_list();
                let mut clicked_line = None;
                let mut row_action = None;
                let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
                if let Some(row) = self.pending_scroll_row.take() {
                    // 行距 = 行高 + item_spacing.y，与 show_rows 内部计算保持一致，
                    // 否则大行号时偏移量越差越多，目标行停在视口之外
                    let stride = row_h + ui.spacing().item_spacing.y;
                    scroll = scroll.vertical_scroll_offset(row as f32 * stride);
                }
                let inner = scroll.show_rows(ui, row_h, n_rows, |ui, range| {
                    self.last_row_range = Some(range.clone());
                    for row in range {
                        let line = rows[row];
                        let (num, summary, byte_len, kind, bad) = match &mut self.doc {
                            Some(doc) => {
                                let info = doc.line_info(line);
                                (
                                    line + 1,
                                    info.summary.clone(),
                                    info.byte_len,
                                    if self.is_wire {
                                        wire::classify(&info.parsed)
                                    } else {
                                        WireKind::NotWire
                                    },
                                    info.parsed.is_err(),
                                )
                            }
                            None => continue,
                        };
                        let is_sel = self.selected == Some(line);
                        let color = if bad {
                            BAD_LINE
                        } else {
                            match kind {
                                WireKind::LlmRequest => KIND_REQUEST,
                                WireKind::ToolCall => KIND_TOOL_CALL,
                                WireKind::ToolResult => KIND_TOOL_RESULT,
                                WireKind::Think => KIND_THINK,
                                WireKind::Text => KIND_TEXT,
                                WireKind::UsageRecord => KIND_USAGE,
                                WireKind::Interaction => KIND_INTERACTION,
                                WireKind::NotWire => ui.visuals().text_color(),
                            }
                        };
                        let size_color = if byte_len > 100 * 1024 {
                            SIZE_BIG
                        } else if byte_len >= 1024 {
                            SIZE_KB
                        } else {
                            Color32::GRAY
                        };

                        // 自绘整行：满宽选中高亮 + 悬停反馈
                        let width = ui.available_width();
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(width, row_h),
                            egui::Sense::click(),
                        );
                        if is_sel {
                            ui.painter()
                                .rect_filled(rect, 0.0, ui.visuals().selection.bg_fill);
                        } else if resp.hovered() {
                            ui.painter()
                                .rect_filled(rect, 0.0, ui.visuals().widgets.hovered.bg_fill);
                        }
                        let text_color = if is_sel {
                            ui.visuals().selection.stroke.color
                        } else {
                            color
                        };
                        let font = FontId::new(fs, FontFamily::Monospace);
                        let cy = rect.center().y;
                        // 三段文字统一用 Align2::LEFT_CENTER，保证同一基线对齐
                        let num_text = format!("{num:>5}");
                        let num_w = ui.fonts(|f| {
                            f.layout_no_wrap(num_text.clone(), font.clone(), Color32::GRAY)
                                .size()
                                .x
                        });
                        ui.painter().text(
                            egui::pos2(rect.min.x + 4.0, cy),
                            Align2::LEFT_CENTER,
                            num_text,
                            font.clone(),
                            Color32::GRAY,
                        );
                        ui.painter().text(
                            egui::pos2(rect.min.x + 4.0 + num_w + 8.0, cy),
                            Align2::LEFT_CENTER,
                            &summary,
                            font.clone(),
                            text_color,
                        );
                        ui.painter().text(
                            egui::pos2(rect.max.x - 6.0, cy),
                            Align2::RIGHT_CENTER,
                            fmt_bytes(byte_len),
                            font,
                            size_color,
                        );

                        if resp.clicked() {
                            clicked_line = Some(line);
                        }
                        resp.context_menu(|ui| {
                            if ui.button(t.copy_raw).clicked() {
                                row_action = Some(RowAction::CopyRaw(line));
                                ui.close_menu();
                            }
                            if ui.button(t.copy_pretty).clicked() {
                                row_action = Some(RowAction::CopyPretty(line));
                                ui.close_menu();
                            }
                            if ui.button(t.view_text_line).clicked() {
                                row_action = Some(RowAction::ViewText(line));
                                ui.close_menu();
                            }
                        });
                    }
                });
                let _ = inner;
                self.visible = rows;
                if let Some(line) = clicked_line {
                    // 鼠标点击：不滚动列表
                    self.select_line(line, false);
                }
                if let Some(action) = row_action {
                    self.do_row_action(ctx, action);
                }
            });
    }

    fn do_row_action(&mut self, ctx: &Context, action: RowAction) {
        let Some(doc) = &self.doc else { return };
        match action {
            RowAction::CopyRaw(line) => {
                ctx.copy_text(doc.raw_line(line).to_string());
                self.status = self.t().copied(line + 1, false);
            }
            RowAction::CopyPretty(line) => {
                let raw = doc.raw_line(line);
                let text = serde_json::from_str::<serde_json::Value>(raw)
                    .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| raw.to_string()))
                    .unwrap_or_else(|_| raw.to_string());
                ctx.copy_text(text);
                self.status = self.t().copied(line + 1, true);
            }
            RowAction::ViewText(line) => {
                let raw = doc.raw_line(line).to_string();
                let title = self.t().line_raw_title(line + 1);
                self.open_text_window(title, raw);
            }
        }
    }

    // ---- 详情面板 ----

    fn detail_panel(&mut self, ctx: &Context) {
        let t = self.t();
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(sel) = self.selected else {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new(t.empty_hint).color(Color32::GRAY));
                });
                return;
            };
            let Some(doc) = &mut self.doc else { return };

            // 选中行是否为 llm.request（决定是否显示重建上下文 Tab）
            let is_request = {
                let info = doc.line_info(sel);
                wire::classify(&info.parsed) == WireKind::LlmRequest
            };
            // 解析结果按行缓存；take 出来用，避免每帧克隆大 JSON（如完整工具 schema 行）
            if self.detail_cache.as_ref().map(|(l, ..)| *l) != Some(sel) {
                let info = doc.peek_info(sel);
                let raw = doc.raw_line(sel).to_string();
                let pretty = match &info.parsed {
                    Ok(v) => serde_json::to_string_pretty(v).unwrap_or_else(|_| raw.clone()),
                    Err(_) => raw.clone(),
                };
                self.detail_cache = Some((sel, info.parsed, raw, pretty));
            }
            let Some((_, parsed, raw, mut pretty)) = self.detail_cache.take() else {
                return;
            };
            let line_no = sel + 1;
            let fs = self.fs_detail();

            ui.horizontal(|ui| {
                ui.label(t.line_label(line_no));
                ui.separator();
                ui.selectable_value(&mut self.tab, DetailTab::Tree, t.tab_tree);
                ui.selectable_value(&mut self.tab, DetailTab::Pretty, t.tab_pretty);
                ui.selectable_value(&mut self.tab, DetailTab::Raw, t.tab_raw);
                if is_request {
                    ui.selectable_value(&mut self.tab, DetailTab::Rebuild, t.tab_rebuild);
                } else if self.tab == DetailTab::Rebuild {
                    self.tab = DetailTab::Tree;
                }
                // 树视图操作紧跟 Tab，触手可及
                if self.tab == DetailTab::Tree {
                    ui.separator();
                    if ui.small_button(t.expand_all).clicked() {
                        self.tree_default_open = Some(true);
                        self.tree_gen += 1;
                    }
                    if ui.small_button(t.collapse_all).clicked() {
                        self.tree_default_open = Some(false);
                        self.tree_gen += 1;
                    }
                }
            });
            ui.separator();

            match self.tab {
                DetailTab::Tree => match &parsed {
                    Ok(v) => {
                        let mut action = None;
                        let default_open = self.tree_default_open;
                        let gen = self.tree_gen;
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            action = json_tree::show_value_tree(
                                ui,
                                v,
                                &format!("L{line_no}"),
                                fs,
                                default_open,
                                gen,
                                t,
                            );
                        });
                        if let Some(TreeAction::OpenText { title, content }) = action {
                            self.open_text_window(title, content);
                        }
                    }
                    Err(e) => {
                        ui.colored_label(BAD_LINE, format!("{}: {e}", t.json_parse_failed));
                    }
                },
                DetailTab::Pretty => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        let edit_id = egui::Id::new(("detail_pretty_edit", sel));
                        show_editable_text(
                            ui,
                            edit_id,
                            &mut pretty,
                            FontId::new(fs, FontFamily::Monospace),
                            ui.available_width(),
                            t,
                        );
                    });
                }
                DetailTab::Raw => {
                    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                        ui.label(RichText::new(raw.as_str()).font(FontId::new(fs, FontFamily::Monospace)));
                    });
                }
                DetailTab::Rebuild => {
                    self.show_rebuild(ui, sel);
                }
            }
            // 详情缓存放回
            self.detail_cache = Some((sel, parsed, raw, pretty));
        });
    }

    fn show_rebuild(&mut self, ui: &mut Ui, request_line: usize) {
        if self.rebuild_cache.as_ref().map(|(l, ..)| l) != Some(&request_line) {
            if let Some(doc) = &self.doc {
                let items = wire::rebuild_context(doc, request_line);
                let count = wire::estimate_message_count(&items);
                let bytes = wire::estimate_context_bytes(&items);
                let mut stat = SideStat::default();
                for it in &items {
                    match it.side() {
                        Side::Host => {
                            stat.host_n += 1;
                            stat.host_bytes += wire::ctx_item_bytes(it);
                        }
                        Side::Llm => {
                            stat.llm_n += 1;
                            stat.llm_bytes += wire::ctx_item_bytes(it);
                        }
                    }
                }
                self.rebuild_cache = Some((request_line, items, count, bytes, stat));
            }
        }
        let Some((_, items, est, bytes, stat)) = &self.rebuild_cache else {
            return;
        };
        let t = self.t();
        let declared = self
            .timeline
            .iter()
            .find(|e| e.line_idx == request_line)
            .map(|e| e.message_count);
        let mut pending_json = false;
        ui.horizontal(|ui| {
            ui.label(t.rebuilt_count(*est));
            ui.label(t.context_bytes(fmt_bytes(*bytes)));
            ui.label(
                RichText::new("🖥")
                    .family(egui::FontFamily::Name(EMOJI_BOLD_FAMILY.into()))
                    .color(KIND_TOOL_RESULT)
                    .strong(),
            );
            ui.label(
                RichText::new(t.side_stats_host(stat.host_n, fmt_bytes(stat.host_bytes)))
                    .color(Color32::GRAY),
            );
            ui.label(RichText::new("|").color(Color32::GRAY));
            ui.label(
                RichText::new("🤖")
                    .family(egui::FontFamily::Name(EMOJI_BOLD_FAMILY.into()))
                    .color(KIND_TOOL_CALL)
                    .strong(),
            );
            ui.label(
                RichText::new(t.side_stats_llm(stat.llm_n, fmt_bytes(stat.llm_bytes)))
                    .color(Color32::GRAY),
            );
            if let Some(d) = declared {
                let ok = d == *est;
                ui.colored_label(
                    if ok { KIND_TOOL_RESULT } else { BAD_LINE },
                    format!("llm.request.messageCount = {d} {}", if ok { "✓" } else { "✗" }),
                );
            }
            ui.separator();
            if ui.small_button(t.request_json_btn).clicked() {
                pending_json = true;
            }
        });
        ui.separator();
        // ---- 页签内搜索条（只搜界面可见文本：短条目全文 + 长条目的截断预览）----
        let mut nav: Option<bool> = None; // true=下一个 false=上一个
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.rebuild_search)
                    .hint_text(t.rebuild_search_hint)
                    .desired_width(160.0),
            );
            if !self.rebuild_search.is_empty() {
                ui.spacing_mut().item_spacing.x = 2.0;
                if ui
                    .small_button("×")
                    .on_hover_text(t.search_clear_tip)
                    .clicked()
                {
                    self.rebuild_search.clear();
                }
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                nav = Some(!ui.input(|i| i.modifiers.shift));
                resp.request_focus();
            }
            if ui
                .selectable_label(self.rebuild_case, "Aa")
                .on_hover_text(t.case_sensitive_tip)
                .clicked()
            {
                self.rebuild_case = !self.rebuild_case;
            }
            if !self.rebuild_search.trim().is_empty() {
                let cur_disp = self.rebuild_cur.map(|c| c + 1).unwrap_or(0);
                ui.label(
                    RichText::new(format!("{cur_disp}/{}", self.rebuild_matches.len()))
                        .color(Color32::GRAY),
                );
                if ui.button(" ↑ ").on_hover_text(t.prev_match_tip).clicked() {
                    nav = Some(false);
                }
                ui.add_space(10.0);
                if ui.button(" ↓ ").on_hover_text(t.next_match_tip).clicked() {
                    nav = Some(true);
                }
            }
        });
        // 查询 / 请求行 / 大小写任一变化即重算命中（可见文本量小，UI 线程同步算）
        let query = self.rebuild_search.trim().to_string();
        let key = (request_line, query.clone(), self.rebuild_case);
        if self.rebuild_match_key.as_ref() != Some(&key) {
            self.rebuild_matches = wire::find_rebuild_matches(items, &query, self.rebuild_case);
            self.rebuild_match_key = Some(key);
            self.rebuild_cur = None;
        }
        // 上/下一个：回绕定位；本帧有导航动作时渲染到目标块后 scroll_to_rect
        if let Some(forward) = nav {
            let n = self.rebuild_matches.len();
            if n > 0 {
                let c = match (self.rebuild_cur, forward) {
                    (None, true) => 0,
                    (None, false) => n - 1,
                    (Some(c), true) => (c + 1) % n,
                    (Some(c), false) => (c + n - 1) % n,
                };
                self.rebuild_cur = Some(c);
            }
        }
        if self.rebuild_matches.is_empty() {
            self.rebuild_cur = None;
        }
        let fs = self.fs_list();
        let matches = &self.rebuild_matches;
        let cur = self.rebuild_cur;
        let cursor = self.rebuild_cursor.get(&request_line).copied();
        let scroll_target = if nav.is_some() {
            cur.map(|c| matches[c].item)
        } else {
            None
        };
        let dark = ui.visuals().dark_mode;
        let mut pending_open: Option<(String, String)> = None;
        let mut pending_jump: Option<usize> = None;
        let mut pending_toggle: Option<usize> = None;
        let key_scroll = std::mem::take(&mut self.rebuild_key_scroll);
        let scroll_out = egui::ScrollArea::vertical()
            .id_salt(("rebuild_scroll", request_line))
            .auto_shrink([false, false])
            .show(ui, |ui| {
            let mut msg_no = 0usize;
            let mut prev_side: Option<Side> = None;
            for (idx, item) in items.iter().enumerate() {
                let side = item.side();
                let role = item.role();
                let (side_icon, side_short, side_color) = match side {
                    Side::Host => ("🖥", t.side_host_short, KIND_TOOL_RESULT),
                    Side::Llm => ("🤖", t.side_llm_short, KIND_TOOL_CALL),
                };
                // 行首来源徽标：彩色加粗图标 + 灰色〔role · 侧〕；Message 标题已含 role，只标侧
                let badge = if matches!(item, CtxItem::Message { .. }) {
                    format!("〔{side_short}〕")
                } else {
                    format!("〔{role} · {side_short}〕")
                };
                // 侧切换（含首条）时插入分侧标题行
                if prev_side != Some(side) {
                    let (label, color) = match side {
                        Side::Host => (t.side_host, KIND_TOOL_RESULT),
                        Side::Llm => (t.side_llm, KIND_TOOL_CALL),
                    };
                    if prev_side.is_some() {
                        ui.add_space(6.0);
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(RichText::new("────── ").color(color).strong().small());
                        ui.label(
                            RichText::new(side_icon)
                                .family(egui::FontFamily::Name(EMOJI_BOLD_FAMILY.into()))
                                .color(color)
                                .strong()
                                .small(),
                        );
                        ui.label(
                            RichText::new(format!(" {label} ──────"))
                                .color(color)
                                .strong()
                                .small(),
                        );
                    });
                    prev_side = Some(side);
                }
                // 本条目命中拆分：标题命中 → 标题整段加底色；内容命中 → 精确区间高亮
                let title_len = item.search_parts().0.len();
                let mut title_hit_cur = false;
                let mut title_hit = false;
                let mut content_hits: Vec<(usize, usize, bool)> = Vec::new();
                for (mi, m) in matches.iter().enumerate() {
                    if m.item != idx {
                        continue;
                    }
                    let is_cur = cur == Some(mi);
                    if m.start < title_len {
                        title_hit = true;
                        title_hit_cur |= is_cur;
                    } else {
                        content_hits.push((m.start - title_len - 1, m.end - title_len - 1, is_cur));
                    }
                }
                let title_bg = if title_hit {
                    Some(if title_hit_cur {
                        rebuild_cur_bg(dark)
                    } else {
                        rebuild_hit_bg(dark)
                    })
                } else {
                    None
                };
                let block_top = ui.cursor().top();
                // 背景槽：块渲染完后回填 rect_filled，底色垫在内容之下（egui Frame 同款技巧）
                let bg_slot = ui.painter().add(egui::Shape::Noop);
                // 行点击走 egui 命中测试：用上一帧块矩形提前挂交互——后画的块内按钮
                // 在同层盖住它，上层弹窗/滚动区外的搜索框也会被 egui 判为点击目标，
                // 不会像原始输入判定那样把弹窗/搜索框的点击漏击穿行
                let row_id = egui::Id::new(("rebuild_row", request_line, idx));
                let prev_rect = ui
                    .ctx()
                    .data_mut(|d| d.get_temp::<egui::Rect>(row_id));
                let row_resp =
                    prev_rect.map(|r| ui.interact(r, row_id, egui::Sense::click()));
                // 块内按钮（跳至源行/查看文本）接走的点击不算「点行」
                let pre_pending = (pending_open.is_some(), pending_jump.is_some());
                ui.horizontal_top(|ui| {
                    // 条目序号（行号）
                    ui.label(
                        RichText::new(format!("{:>2}", idx + 1))
                            .font(FontId::new(fs - 2.0, FontFamily::Monospace))
                            .color(Color32::GRAY),
                    );
                    ui.vertical(|ui| {
                match item {
                    CtxItem::SystemPrompt { line_idx, text } => {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(side_icon)
                                    .family(egui::FontFamily::Name(EMOJI_BOLD_FAMILY.into()))
                                    .color(side_color)
                                    .strong(),
                            );
                            ui.label(RichText::new(&badge).color(Color32::GRAY).small());
                            ui.label(with_bg(
                                RichText::new(t.sys_prompt_label).color(KIND_USAGE).strong(),
                                title_bg,
                            ));
                            if ui.small_button(t.jump_to_source).clicked() {
                                pending_jump = Some(*line_idx);
                            }
                        });
                        if let Some(a) = text_with_view_button_hl(
                            ui,
                            text,
                            &t.sys_prompt_title(line_idx + 1),
                            fs,
                            t,
                            &content_hits,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                    CtxItem::ToolsDef {
                        line_idx,
                        text,
                        tool_count,
                    } => {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(side_icon)
                                    .family(egui::FontFamily::Name(EMOJI_BOLD_FAMILY.into()))
                                    .color(side_color)
                                    .strong(),
                            );
                            ui.label(RichText::new(&badge).color(Color32::GRAY).small());
                            ui.label(with_bg(
                                RichText::new(t.tools_def_label(*tool_count))
                                    .color(KIND_USAGE)
                                    .strong(),
                                title_bg,
                            ));
                            if ui.small_button(t.jump_to_source).clicked() {
                                pending_jump = Some(*line_idx);
                            }
                        });
                        if let Some(a) = text_with_view_button_hl(
                            ui,
                            text,
                            &t.tools_def_title(line_idx + 1),
                            fs,
                            t,
                            &content_hits,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                    CtxItem::Message { role, text, origin } => {
                        msg_no += 1;
                        let color = match role.as_str() {
                            "user" => KIND_TOOL_RESULT,
                            "assistant" => KIND_TOOL_CALL,
                            "system" => KIND_USAGE,
                            _ => Color32::GRAY,
                        };
                        let origin_tag = match origin.as_deref() {
                            Some("injection") => t.injection_tag(),
                            Some(o) => {
                                if o == "user" {
                                    ""
                                } else {
                                    " [?]"
                                }
                            }
                            None => "",
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(side_icon)
                                    .family(egui::FontFamily::Name(EMOJI_BOLD_FAMILY.into()))
                                    .color(side_color)
                                    .strong(),
                            );
                            ui.label(RichText::new(&badge).color(Color32::GRAY).small());
                            ui.label(with_bg(
                                RichText::new(format!("#{msg_no} {role}{origin_tag}"))
                                    .color(color)
                                    .strong(),
                                title_bg,
                            ));
                        });
                        if let Some(a) = text_with_view_button_hl(
                            ui,
                            text,
                            &t.msg_title(request_line + 1, msg_no),
                            fs,
                            t,
                            &content_hits,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                    CtxItem::Think(txt) => {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(side_icon)
                                    .family(egui::FontFamily::Name(EMOJI_BOLD_FAMILY.into()))
                                    .color(side_color)
                                    .strong(),
                            );
                            ui.label(RichText::new(&badge).color(Color32::GRAY).small());
                            ui.label(with_bg(
                                RichText::new(t.think_label).color(KIND_THINK).strong(),
                                title_bg,
                            ));
                        });
                        if let Some(a) = text_with_view_button_hl(
                            ui,
                            txt,
                            &format!("L{} think", request_line + 1),
                            fs,
                            t,
                            &content_hits,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                    CtxItem::Text(txt) => {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(side_icon)
                                    .family(egui::FontFamily::Name(EMOJI_BOLD_FAMILY.into()))
                                    .color(side_color)
                                    .strong(),
                            );
                            ui.label(RichText::new(&badge).color(Color32::GRAY).small());
                            ui.label(with_bg(
                                RichText::new(t.text_label).color(KIND_TEXT).strong(),
                                title_bg,
                            ));
                        });
                        if let Some(a) = text_with_view_button_hl(
                            ui,
                            txt,
                            &format!("L{} text", request_line + 1),
                            fs,
                            t,
                            &content_hits,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                    CtxItem::ToolCall { id, name, args } => {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(side_icon)
                                    .family(egui::FontFamily::Name(EMOJI_BOLD_FAMILY.into()))
                                    .color(side_color)
                                    .strong(),
                            );
                            ui.label(RichText::new(&badge).color(Color32::GRAY).small());
                            ui.label(with_bg(
                                RichText::new(format!("🔧 {name}  ({id})"))
                                    .color(KIND_TOOL_CALL)
                                    .strong(),
                                title_bg,
                            ));
                        });
                        if let Some(a) = text_with_view_button_hl(
                            ui,
                            args,
                            &t.tool_args_title(request_line + 1, name),
                            fs,
                            t,
                            &content_hits,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                    CtxItem::ToolResult { id, name, output } => {
                        let title = match name {
                            Some(n) => format!("  ↩ {n} result ({id})"),
                            None => format!("  ↩ result ({id})"),
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(side_icon)
                                    .family(egui::FontFamily::Name(EMOJI_BOLD_FAMILY.into()))
                                    .color(side_color)
                                    .strong(),
                            );
                            ui.label(RichText::new(&badge).color(Color32::GRAY).small());
                            ui.label(with_bg(
                                RichText::new(title).color(KIND_TOOL_RESULT),
                                title_bg,
                            ));
                        });
                        if let Some(a) = text_with_view_button_hl(
                            ui,
                            output,
                            &t.tool_result_title(request_line + 1),
                            fs,
                            t,
                            &content_hits,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                }
                ui.add_space(4.0);
                    });
                });
                let clip = ui.clip_rect();
                let block_rect = egui::Rect::from_min_max(
                    egui::pos2(clip.left(), block_top),
                    egui::pos2(clip.right(), ui.cursor().top()),
                );
                // 阅读游标行底色优先；当前命中所在条目给淡底色
                if cursor == Some(idx) {
                    ui.painter().set(
                        bg_slot,
                        egui::Shape::rect_filled(block_rect, 2.0, rebuild_cursor_bg(dark)),
                    );
                } else if cur.is_some_and(|c| matches[c].item == idx) {
                    ui.painter().set(
                        bg_slot,
                        egui::Shape::rect_filled(block_rect, 2.0, rebuild_cur_item_bg(dark)),
                    );
                }
                if scroll_target == Some(idx) {
                    ui.scroll_to_rect(block_rect, Some(egui::Align::Center));
                }
                // 记录块矩形供下一帧挂交互
                ui.ctx().data_mut(|d| d.insert_temp(row_id, block_rect));
                // 点文本：egui 0.31 的 label 默认可选（click_and_drag 传感），文本上的
                // 点击判给 label 而非行控件——所以除行控件自身 clicked 外，再看本帧
                // 被点击的控件是否落在本行矩形内（且同属本层面板，上层弹窗排除）；
                // 拖选文本是 drag 不是 click，不会误触发标记
                let clicked_widget_in_row = ui
                    .ctx()
                    .interaction_snapshot(|s| s.clicked)
                    .and_then(|id| ui.ctx().read_response(id))
                    .is_some_and(|r| {
                        r.layer_id == ui.layer_id() && block_rect.contains(r.rect.center())
                    });
                let clicked_here = (row_resp.is_some_and(|r| r.clicked())
                    || clicked_widget_in_row)
                    && pre_pending == (pending_open.is_some(), pending_jump.is_some());
                if clicked_here {
                    pending_toggle = Some(idx);
                }
            }
        });
        // 记下视口矩形（悬停判方向键用）；应用本帧键盘滚动量
        self.rebuild_scroll_rect = Some(scroll_out.inner_rect);
        if key_scroll != 0.0 {
            if let Some(mut state) =
                egui::containers::scroll_area::State::load(ui.ctx(), scroll_out.id)
            {
                let max_y = (scroll_out.content_size.y - scroll_out.inner_rect.height()).max(0.0);
                state.offset.y = (state.offset.y + key_scroll).clamp(0.0, max_y);
                state.store(ui.ctx(), scroll_out.id);
            }
        }
        if let Some(idx) = pending_toggle {
            if self.rebuild_cursor.get(&request_line) == Some(&idx) {
                self.rebuild_cursor.remove(&request_line);
            } else {
                self.rebuild_cursor.insert(request_line, idx);
            }
        }
        if let Some((title, content)) = pending_open {
            self.open_text_window(title, content);
        }
        if pending_json {
            let body = self
                .doc
                .as_ref()
                .and_then(|doc| wire::build_request_body(doc, request_line));
            match body {
                Some(body) => {
                    let step = self
                        .timeline
                        .iter()
                        .find(|e| e.line_idx == request_line)
                        .map(|e| e.turn_step.clone())
                        .unwrap_or_default();
                    let title = t.request_json_title(request_line + 1, &step);
                    let id = self.next_win_id;
                    self.next_win_id += 1;
                    self.json_windows.push(JsonViewWindow::new(id, title, body));
                }
                None => self.status = t.request_json_failed.into(),
            }
        }
        if let Some(line) = pending_jump {
            self.select_line(line, true);
        }
    }

    fn open_text_window(&mut self, title: String, content: String) {
        let id = self.next_win_id;
        self.next_win_id += 1;
        self.text_windows.push(TextViewWindow::new(id, title, content));
    }

    fn status_bar(&mut self, ctx: &Context) {
        let t = self.t();
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(doc) = &self.doc {
                    ui.label(t.status_lines(doc.line_count()));
                    ui.separator();
                    ui.label(fmt_bytes(doc.total_bytes()));
                    if let Some(sel) = self.selected {
                        ui.separator();
                        ui.label(t.current_line(sel + 1));
                    }
                    if self.is_wire {
                        ui.separator();
                        ui.colored_label(KIND_REQUEST, t.wire_mode(self.timeline.len()));
                    }
                    if !self.search_text.trim().is_empty() {
                        ui.separator();
                        ui.label(t.search_status(self.matches.len(), self.search_done));
                    }
                } else {
                    ui.label(t.no_file);
                }
                let status = fit_text_tail(ui, &self.status, egui::TextStyle::Small);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(status).color(Color32::GRAY).small());
                });
            });
        });
    }
}

impl eframe::App for JsonlApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        ctx.set_theme(if self.settings.dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        });
        self.apply_font_size(ctx);
        self.fit_main_window_at_startup(ctx);

        // 拖放打开文件
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(p) = dropped.into_iter().next() {
            self.open_path(p);
        }

        self.poll_search();
        // 后台注册表查询结果回收
        if let Some(rx) = &self.shell_reg_rx {
            if let Ok(b) = rx.try_recv() {
                self.shell_registered = Some(b);
                self.shell_reg_rx = None;
            }
        }
        self.handle_keys(ctx);
        self.menu_bar(ctx);
        self.status_bar(ctx);
        self.line_list_panel(ctx);
        self.detail_panel(ctx);
        self.dialogs(ctx);

        // 长文本窗口
        let fs = self.fs_detail();
        let t = self.t();
        for w in &mut self.text_windows {
            w.show(ctx, fs, t);
        }
        self.text_windows.retain(|w| w.open);

        // 结构化 JSON 窗口（请求体重建）；树内长字符串可再开纯文本窗口
        let mut pending_text: Option<(String, String)> = None;
        for w in &mut self.json_windows {
            if let Some(TreeAction::OpenText { title, content }) = w.show(ctx, fs, t) {
                pending_text = Some((title, content));
            }
        }
        self.json_windows.retain(|w| w.open);
        if let Some((title, content)) = pending_text {
            self.open_text_window(title, content);
        }

        // 搜索进行时持续重绘
        if self.search_handle.is_some() || self.last_edit.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}

/// 长文本：截断预览 + 「纯文本查看」按钮；短文本直接显示。
/// 返回用户请求打开的 (标题, 内容)。
fn text_with_view_button(
    ui: &mut Ui,
    text: &str,
    title: &str,
    font_size: f32,
    t: &T,
) -> Option<(String, String)> {
    if is_long_text(text) {
        let mut open = None;
        ui.horizontal(|ui| {
            // 按可用宽度截断预览，保证按钮在小窗口下也可见可点
            let reserve = font_size * 10.0; // 按钮约占宽度
            let avail = (ui.available_width() - reserve).max(font_size * 8.0);
            let max_chars = ((avail / font_size) as usize).min(160);
            let preview: String = text.chars().take(max_chars).collect();
            ui.label(
                RichText::new(format!("{}…", preview.replace('\n', "\\n")))
                    .font(FontId::new(font_size - 1.0, FontFamily::Monospace))
                    .color(Color32::GRAY),
            );
            if ui.small_button(t.view_text_btn).clicked() {
                open = Some((title.to_string(), text.to_string()));
            }
        });
        open
    } else if !text.is_empty() {
        ui.label(RichText::new(text).font(FontId::new(font_size - 1.0, FontFamily::Monospace)));
        None
    } else {
        None
    }
}

/// RichText 条件加背景色（搜索命中标题整段标底色用）
fn with_bg(rt: RichText, bg: Option<Color32>) -> RichText {
    match bg {
        Some(bg) => rt.background_color(bg),
        None => rt,
    }
}

/// 重建页签：阅读游标行底色
fn rebuild_cursor_bg(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x2e, 0x3f, 0x5c)
    } else {
        Color32::from_rgb(0xd7, 0xe5, 0xf7)
    }
}

/// 重建页签搜索：普通命中底色
fn rebuild_hit_bg(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x4a, 0x3f, 0x1a)
    } else {
        Color32::from_rgb(0xff, 0xee, 0xa9)
    }
}

/// 重建页签搜索：当前命中底色
fn rebuild_cur_bg(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x7a, 0x5c, 0x14)
    } else {
        Color32::from_rgb(0xff, 0xd2, 0x4d)
    }
}

/// 重建页签搜索：当前命中所在条目整行底色（比游标淡）
fn rebuild_cur_item_bg(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x33, 0x30, 0x20)
    } else {
        Color32::from_rgb(0xfb, 0xf3, 0xd5)
    }
}

fn hl_format(font: &FontId, color: Color32, background: Color32) -> egui::TextFormat {
    egui::TextFormat {
        font_id: font.clone(),
        color,
        background,
        ..Default::default()
    }
}

/// 短文本内容：按命中区间切段的 LayoutJob（当前命中用更亮底色）。
/// hits 为 (起, 止, 是否当前命中)，升序不重叠，字节下标会先收拢到字符边界。
fn highlighted_job(
    text: &str,
    hits: &[(usize, usize, bool)],
    font: FontId,
    color: Color32,
    dark: bool,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let mut pos = 0usize;
    for &(s, e, is_cur) in hits {
        let s = wire::clamp_char_boundary(text, s).max(pos);
        let e = wire::clamp_char_boundary(text, e);
        if e <= s {
            continue;
        }
        if s > pos {
            job.append(
                &text[pos..s],
                0.0,
                hl_format(&font, color, Color32::TRANSPARENT),
            );
        }
        let bg = if is_cur {
            rebuild_cur_bg(dark)
        } else {
            rebuild_hit_bg(dark)
        };
        job.append(&text[s..e], 0.0, hl_format(&font, color, bg));
        pos = e;
    }
    if pos < text.len() {
        job.append(
            &text[pos..],
            0.0,
            hl_format(&font, color, Color32::TRANSPARENT),
        );
    }
    job
}

/// 生成 text_with_view_button 同款单行预览（\n 显示为 \\n），
/// 并返回原文字节下标 → 预览串字节下标的单调映射（含结尾边界），
/// 供把命中区间换算到预览串坐标。
fn build_preview(text: &str, max_chars: usize) -> (String, Vec<(usize, usize)>) {
    let mut s = String::new();
    let mut map = Vec::new();
    for (ob, ch) in text.char_indices().take(max_chars) {
        map.push((ob, s.len()));
        if ch == '\n' {
            s.push_str("\\n");
        } else {
            s.push(ch);
        }
    }
    let end_ob = text
        .char_indices()
        .nth(max_chars)
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    map.push((end_ob, s.len()));
    (s, map)
}

/// 原文字节下标 → 预览串字节下标；超出预览范围的收拢到预览末尾。
fn map_offset(map: &[(usize, usize)], ob: usize) -> usize {
    match map.binary_search_by_key(&ob, |&(o, _)| o) {
        Ok(i) => map[i].1,
        Err(i) => {
            if i == 0 {
                0
            } else {
                map[i - 1].1
            }
        }
    }
}

/// text_with_view_button 的搜索高亮版：hits 为空时完全走原逻辑；
/// 长文本只高亮落在预览串内的命中（被截掉的全文本来也不参与搜索）。
fn text_with_view_button_hl(
    ui: &mut Ui,
    text: &str,
    title: &str,
    font_size: f32,
    t: &T,
    hits: &[(usize, usize, bool)],
) -> Option<(String, String)> {
    if hits.is_empty() {
        return text_with_view_button(ui, text, title, font_size, t);
    }
    let dark = ui.visuals().dark_mode;
    let font = FontId::new(font_size - 1.0, FontFamily::Monospace);
    if is_long_text(text) {
        let mut open = None;
        ui.horizontal(|ui| {
            // 与 text_with_view_button 相同的截断策略
            let reserve = font_size * 10.0;
            let avail = (ui.available_width() - reserve).max(font_size * 8.0);
            let max_chars = ((avail / font_size) as usize).min(wire::SEARCH_PREVIEW_CHARS);
            let (preview, map) = build_preview(text, max_chars);
            let mut job = egui::text::LayoutJob::default();
            let mut pos = 0usize;
            for &(s, e, is_cur) in hits {
                let ds = map_offset(&map, wire::clamp_char_boundary(text, s));
                let de = map_offset(&map, wire::clamp_char_boundary(text, e));
                if de <= ds {
                    continue; // 命中完全在预览之外
                }
                if ds > pos {
                    job.append(
                        &preview[pos..ds],
                        0.0,
                        hl_format(&font, Color32::GRAY, Color32::TRANSPARENT),
                    );
                }
                let bg = if is_cur {
                    rebuild_cur_bg(dark)
                } else {
                    rebuild_hit_bg(dark)
                };
                job.append(&preview[ds..de], 0.0, hl_format(&font, Color32::GRAY, bg));
                pos = de;
            }
            if pos < preview.len() {
                job.append(
                    &preview[pos..],
                    0.0,
                    hl_format(&font, Color32::GRAY, Color32::TRANSPARENT),
                );
            }
            job.append(
                "…",
                0.0,
                hl_format(&font, Color32::GRAY, Color32::TRANSPARENT),
            );
            ui.label(job);
            if ui.small_button(t.view_text_btn).clicked() {
                open = Some((title.to_string(), text.to_string()));
            }
        });
        open
    } else if !text.is_empty() {
        ui.label(highlighted_job(
            text,
            hits,
            font,
            ui.visuals().text_color(),
            dark,
        ));
        None
    } else {
        None
    }
}

/// 路径等长文本放不下时砍头部、留尾部（加省略号）。
/// 用字体引擎真实测量宽度，既不溢出也不浪费空间。
fn fit_text_tail(ui: &Ui, text: &str, style: egui::TextStyle) -> String {
    let avail = ui.available_width() - 4.0;
    let font = style.resolve(ui.style());
    ui.fonts(|f| {
        let fits = |s: String| {
            f.layout_no_wrap(s, font.clone(), Color32::WHITE).size().x <= avail
        };
        if fits(text.to_string()) {
            return text.to_string();
        }
        if avail <= 0.0 {
            return String::new();
        }
        // 二分找最小砍头量：砍得越多越窄，fits 关于 skip 单调
        let n = text.chars().count();
        let cut = |skip: usize| -> String {
            std::iter::once('…').chain(text.chars().skip(skip)).collect()
        };
        let (mut lo, mut hi) = (1usize, n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if fits(cut(mid)) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        cut(lo)
    })
}

pub fn fmt_bytes(n: usize) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1}MB", n as f64 / 1024.0 / 1024.0)
    } else if n >= 1024 {
        format!("{:.1}KB", n as f64 / 1024.0)
    } else {
        format!("{n}B")
    }
}

/// 全屏窗口的内容区位置与大小。
/// egui `Window::fixed_size` 设的是内容区尺寸，标题栏与窗口边框边距会再加在外面，
/// 需按 window.rs 同样的公式精确扣减，否则全屏窗口右边/下边会超出屏幕。
pub fn maximized_pos_size(ctx: &egui::Context, title: &RichText) -> (egui::Pos2, egui::Vec2) {
    let screen = ctx.screen_rect();
    let style = ctx.style();
    let frame = egui::Frame::window(&style);
    let title_font_h = ctx
        .fonts(|fonts| title.font_height(fonts, &style))
        .max(style.spacing.interact_size.y);
    let title_bar_height = title_font_h + frame.inner_margin.sum().y;
    let chrome = frame.total_margin().sum()
        + egui::vec2(0.0, title_bar_height + frame.stroke.width);
    (screen.min, screen.size() - chrome)
}

/// 读出文本编辑区当前选区（字符索引，升序；空选区为 None）。
pub fn text_edit_selection(ctx: &egui::Context, edit_id: egui::Id) -> Option<(usize, usize)> {
    egui::text_edit::TextEditState::load(ctx, edit_id)
        .and_then(|s| s.cursor.char_range())
        .map(|r| {
            let [a, b] = r.sorted();
            (a.index.min(b.index), a.index.max(b.index))
        })
        .filter(|(a, b)| a < b)
}

/// egui 的 TextEdit 会把右键按下也当成一次新点选、清空既有选区
/// （text_cursor_state.rs 的 pointer_interaction 用的是 any_pressed）。
/// 用法：ui.add(TextEdit) 前先 text_edit_selection 快照；
/// add 之后若 response.secondary_clicked()，调本函数把选区还原。
pub fn restore_text_edit_selection(
    ctx: &egui::Context,
    edit_id: egui::Id,
    sel: Option<(usize, usize)>,
) {
    let Some((a, b)) = sel else { return };
    if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, edit_id) {
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor { index: a, prefer_next_row: true },
            egui::text::CCursor { index: b, prefer_next_row: true },
        )));
        state.store(ctx, edit_id);
    }
    // 选区高亮只在文本区持有焦点时绘制
    ctx.memory_mut(|m| m.request_focus(edit_id));
}

/// 给文本编辑区挂右键菜单：仅「拷贝」（复制当前选中内容，无选中时禁用）。
pub fn text_edit_copy_menu(
    resp: &egui::Response,
    ctx: &egui::Context,
    edit_id: egui::Id,
    content: &str,
    t: &T,
) {
    resp.context_menu(|ui| {
        // 菜单宽度按内容估算，默认可换行会把短文案（如 "Copy"）折行；禁止换行
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        let sel = text_edit_selection(ctx, edit_id);
        if ui
            .add_enabled(sel.is_some(), egui::Button::new(t.copy_selection))
            .clicked()
        {
            if let Some((a, b)) = sel {
                let s: String = content.chars().skip(a).take(b - a).collect();
                ui.ctx().copy_text(s);
            }
            ui.close_menu();
            // 防御：点「拷贝」后选区若被 egui 内部路径动过（菜单关闭、焦点
            // 流转等），当帧强制还原，避免高亮闪一下
            restore_text_edit_selection(ui.ctx(), edit_id, sel);
        }
        // 选区高亮只在文本区持有焦点时绘制；右键菜单（以及点「拷贝」时
        // 菜单按钮获得的焦点）会夺走焦点，逐帧把焦点还给文本区
        ui.ctx().memory_mut(|m| m.request_focus(edit_id));
    });
}

/// 三处「格式化文本/纯文本」编辑区的统一显示：可编辑（不保存）、右键「拷贝」菜单。
///
/// 内含右键高亮防闪烁处理：egui 在绘制前就清空选区，事后还原必然闪一帧，
/// 所以右键当帧提前用上一帧缓存的排版结果把高亮垫到文本底下（详见函数内注释）。
pub fn show_editable_text(
    ui: &mut Ui,
    edit_id: egui::Id,
    text: &mut String,
    font: FontId,
    desired_width: f32,
    t: &T,
) -> egui::Response {
    let ctx = ui.ctx().clone();
    let prev_sel = text_edit_selection(&ctx, edit_id);
    // egui 的 TextEdit 会把右键按下当成一次新点选，在绘制之前就清空选区
    // （text_cursor_state.rs 的 pointer_interaction 用的是 any_pressed），
    // 清选区与文本绘制同帧原子完成，事后还原/补绘必然差一帧。所以反过来：
    // 右键这帧在 TextEdit 绘制之前，先用上一帧缓存的排版结果把高亮垫在
    // 即将绘制的文本底下，让 egui 照常清选区、照常画文字——高亮从字底下透出来。
    let cache_id = edit_id.with("sel_galley");
    if let Some(sel) = prev_sel {
        if ctx.input(|i| i.pointer.secondary_pressed()) {
            let cached = ctx.data_mut(|d| {
                d.get_temp::<(Arc<egui::Galley>, egui::Pos2, egui::Rect)>(cache_id)
            });
            if let Some((galley, galley_pos, clip)) = cached {
                let on_text = ctx
                    .input(|i| i.pointer.interact_pos().map_or(false, |p| clip.contains(p)));
                if on_text {
                    paint_selection_highlight(ui, &galley, galley_pos, clip, sel);
                }
            }
        }
    }
    let output = egui::TextEdit::multiline(text)
        .id(edit_id)
        .font(font)
        .frame(false)
        .desired_width(desired_width)
        .show(ui);
    let resp = output.response.clone();
    if (resp.hovered() && ctx.input(|i| i.pointer.secondary_pressed()))
        || resp.secondary_clicked()
    {
        restore_text_edit_selection(&ctx, edit_id, prev_sel);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            cache_id,
            (output.galley.clone(), output.galley_pos, output.text_clip_rect),
        )
    });
    text_edit_copy_menu(&resp, &ctx, edit_id, text, t);
    drag_edge_autoscroll(ui, &ctx, edit_id);
    resp
}

/// 拖选文本时指针压到滚动区边缘要持续自动滚动。
/// egui 内置只在选区变化的帧滚一次（text_edit/builder.rs 的 scroll_to_rect），
/// 指针停在边缘不动时选区不再变化、滚动就停了；这里每帧按指针超出边缘的
/// 距离补滚动量，并 request_repaint 维持滚动循环，鼠标按住不动也会一直滚。
fn drag_edge_autoscroll(ui: &Ui, ctx: &egui::Context, edit_id: egui::Id) {
    if !ctx.is_being_dragged(edit_id) {
        return;
    }
    let Some(pointer) = ctx.pointer_interact_pos() else {
        return;
    };
    let clip = ui.clip_rect(); // 所在 ScrollArea 的可见区域
    const EDGE: f32 = 24.0; // 边缘感应区宽度
    // 基础速度 + 随超出距离加速，封顶防飞
    let speed = |overshoot: f32| 4.0 + overshoot.min(200.0) * 0.15;
    let mut delta = egui::Vec2::ZERO;
    if pointer.y > clip.bottom() - EDGE {
        delta.y = speed(pointer.y - (clip.bottom() - EDGE));
    } else if pointer.y < clip.top() + EDGE {
        delta.y = -speed((clip.top() + EDGE) - pointer.y);
    }
    if pointer.x > clip.right() - EDGE {
        delta.x = speed(pointer.x - (clip.right() - EDGE));
    } else if pointer.x < clip.left() + EDGE {
        delta.x = -speed((clip.left() + EDGE) - pointer.x);
    }
    if delta != egui::Vec2::ZERO {
        // 让「可见区域平移 delta」后的矩形可见：最小滚动量正好是 delta，
        // 冒泡到父 ScrollArea；滚动后 galley 位置变化，选区随指针延伸
        ui.scroll_to_rect(clip.translate(delta), None);
        ctx.request_repaint();
    }
}

/// 按 egui 画选区的算法（visuals.rs 的 paint_text_selection）把高亮矩形画到
/// 指定位置。用于右键当帧在 TextEdit 绘制之前预先把高亮垫到文本底下。
fn paint_selection_highlight(
    ui: &Ui,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    clip: egui::Rect,
    (a, b): (usize, usize),
) {
    let ccursor = |index: usize| egui::text::CCursor { index, prefer_next_row: true };
    let (ca, cb) = (galley.from_ccursor(ccursor(a)), galley.from_ccursor(ccursor(b)));
    let (min, max) = if (ca.rcursor.row, ca.rcursor.column) <= (cb.rcursor.row, cb.rcursor.column) {
        (ca.rcursor, cb.rcursor)
    } else {
        (cb.rcursor, ca.rcursor)
    };
    let fill = ui.visuals().selection.bg_fill;
    let painter = ui.painter().with_clip_rect(clip);
    let offset = galley_pos.to_vec2();
    for ri in min.row..=max.row {
        let Some(row) = galley.rows.get(ri) else { break };
        let left = if ri == min.row { row.x_offset(min.column) } else { row.rect.left() };
        let right = if ri == max.row {
            row.x_offset(max.column)
        } else if row.ends_with_newline {
            // 让行尾的换行符也显得被选中（同 egui 内部做法）
            row.rect.right() + row.height() / 2.0
        } else {
            row.rect.right()
        };
        let rect = egui::Rect::from_min_max(
            egui::pos2(left, row.min_y()),
            egui::pos2(right, row.max_y()),
        );
        painter.rect_filled(rect.translate(offset), 0.0, fill);
    }
}

/// 弹窗用窗口框架：浅色模式加深阴影（默认太淡）；深色模式黑阴影融进背景
/// 看不出层次，改用亮色描边 + 淡白泛光营造浮起感。
pub fn popup_frame(ctx: &egui::Context) -> egui::Frame {
    let mut frame = egui::Frame::window(&ctx.style());
    if ctx.style().visuals.dark_mode {
        frame.stroke = egui::Stroke::new(1.0, Color32::from_gray(72));
        frame.shadow = egui::epaint::Shadow {
            offset: [0, 0],
            blur: 24,
            spread: 1,
            color: Color32::from_white_alpha(14),
        };
    } else {
        frame.shadow = egui::epaint::Shadow {
            offset: [6, 10],
            blur: 20,
            spread: 2,
            color: Color32::from_black_alpha(96),
        };
    }
    frame
}

/// 路径超宽时省略前部为「…」，保留文件名所在的尾部，宽度用满 max_w。
fn fit_path_front(ui: &Ui, path: &str, max_w: f32, font_id: FontId) -> String {
    let width = |s: &str| {
        ui.fonts(|f| f.layout_no_wrap(s.to_string(), font_id.clone(), Color32::WHITE).rect.width())
    };
    if width(path) <= max_w {
        return path.to_string();
    }
    // 二分：找能放下的最长尾部（连同前缀「…」）
    let chars: Vec<char> = path.chars().collect();
    let n = chars.len();
    let mut lo = 0usize; // 已知放不下
    let mut hi = n; // 已知能放下（空串 + …）
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let s: String = chars[n - mid..].iter().collect();
        if width(&format!("…{s}")) <= max_w {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    format!("…{}", chars[n - hi..].iter().collect::<String>())
}
