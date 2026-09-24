use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;

use egui::{Align2, Color32, Context, FontFamily, FontId, Key, Modifiers, RichText, Ui};

use crate::document::JsonlDocument;
use crate::json_tree::{self, TreeAction};
use crate::lang::{self, Lang, T};
use crate::search::{Query, SearchHandle, SearchMsg};
use crate::settings::Settings;
use crate::shell_menu;
use crate::text_view::{is_long_text, TextViewWindow};
use crate::wire::{self, CtxItem, RequestEntry, WireKind};

const SEARCH_DEBOUNCE_MS: u128 = 200;

const KIND_REQUEST: Color32 = Color32::from_rgb(0xee, 0x99, 0x28); // 黄
const KIND_TOOL_CALL: Color32 = Color32::from_rgb(0x61, 0xaf, 0xef); // 蓝
const KIND_TOOL_RESULT: Color32 = Color32::from_rgb(0x98, 0xc3, 0x79); // 绿
const KIND_THINK: Color32 = Color32::from_rgb(0xc6, 0xa0, 0xf6); // 紫
const KIND_TEXT: Color32 = Color32::from_rgb(0x9e, 0x9e, 0x9e); // 灰
const KIND_USAGE: Color32 = Color32::from_rgb(0x56, 0xc2, 0xd6); // 青
const KIND_INTERACTION: Color32 = Color32::from_rgb(0xd1, 0x9a, 0x66); // 橙
const BAD_LINE: Color32 = Color32::from_rgb(0xe0, 0x6c, 0x75); // 红
const SIZE_KB: Color32 = Color32::from_rgb(0xee, 0x99, 0x28); // KB 标橙
const SIZE_BIG: Color32 = Color32::from_rgb(0xe0, 0x6c, 0x75); // >100KB 标红

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Tree,
    Pretty,
    Raw,
    Rebuild,
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
    rebuild_cache: Option<(usize, Vec<CtxItem>, u64, usize)>,
    // 长文本窗口
    text_windows: Vec<TextViewWindow>,
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
    detail_cache: Option<(usize, Result<serde_json::Value, String>, String)>,
    // 树视图全展开/全折叠
    tree_default_open: Option<bool>,
    tree_gen: u64,
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
            text_windows: Vec::new(),
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
                        for p in &recent {
                            let name = p
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| p.display().to_string());
                            if ui
                                .button(name)
                                .on_hover_text(p.display().to_string())
                                .clicked()
                            {
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
                                self.status = match shell_menu::register() {
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(doc.path.display().to_string())
                                .color(Color32::GRAY)
                                .small(),
                        );
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
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(RichText::new("jsonl-v 0.1.0").strong());
                    ui.label(t.about_desc);
                    ui.label(t.about_tech);
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
            if self.detail_cache.as_ref().map(|(l, _, _)| *l) != Some(sel) {
                let info = doc.peek_info(sel);
                let raw = doc.raw_line(sel).to_string();
                self.detail_cache = Some((sel, info.parsed, raw));
            }
            let Some((_, parsed, raw)) = self.detail_cache.take() else {
                return;
            };
            let line_no = sel + 1;
            let fs = self.fs_detail();

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(t.line_label(line_no))
                        .font(FontId::new(fs + 1.0, FontFamily::Monospace)),
                );
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
                        egui::ScrollArea::vertical().show(ui, |ui| {
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
                    let text = match &parsed {
                        Ok(v) => serde_json::to_string_pretty(v).unwrap_or_else(|_| raw.clone()),
                        Err(_) => raw.clone(),
                    };
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(text).font(FontId::new(fs, FontFamily::Monospace)),
                            )
                            .wrap(),
                        );
                    });
                }
                DetailTab::Raw => {
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.label(RichText::new(raw.as_str()).font(FontId::new(fs, FontFamily::Monospace)));
                    });
                }
                DetailTab::Rebuild => {
                    self.show_rebuild(ui, sel);
                }
            }
            // 详情缓存放回
            self.detail_cache = Some((sel, parsed, raw));
        });
    }

    fn show_rebuild(&mut self, ui: &mut Ui, request_line: usize) {
        if self.rebuild_cache.as_ref().map(|(l, ..)| l) != Some(&request_line) {
            if let Some(doc) = &self.doc {
                let items = wire::rebuild_context(doc, request_line);
                let count = wire::estimate_message_count(&items);
                let bytes = wire::estimate_context_bytes(&items);
                self.rebuild_cache = Some((request_line, items, count, bytes));
            }
        }
        let Some((_, items, est, bytes)) = &self.rebuild_cache else {
            return;
        };
        let t = self.t();
        let declared = self
            .timeline
            .iter()
            .find(|e| e.line_idx == request_line)
            .map(|e| e.message_count);
        ui.horizontal(|ui| {
            ui.label(t.rebuilt_count(*est));
            ui.label(t.context_bytes(fmt_bytes(*bytes)));
            if let Some(d) = declared {
                let ok = d == *est;
                ui.colored_label(
                    if ok { KIND_TOOL_RESULT } else { BAD_LINE },
                    format!("llm.request.messageCount = {d} {}", if ok { "✓" } else { "✗" }),
                );
            }
        });
        ui.separator();
        let fs = self.fs_list();
        let mut pending_open: Option<(String, String)> = None;
        let mut pending_jump: Option<usize> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut msg_no = 0usize;
            for item in items {
                match item {
                    CtxItem::SystemPrompt { line_idx, text } => {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(t.sys_prompt_label).color(KIND_USAGE).strong(),
                            );
                            if ui.small_button(t.jump_to_source).clicked() {
                                pending_jump = Some(*line_idx);
                            }
                        });
                        if let Some(a) = text_with_view_button(
                            ui,
                            text,
                            &t.sys_prompt_title(line_idx + 1),
                            fs,
                            t,
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
                                RichText::new(t.tools_def_label(*tool_count))
                                    .color(KIND_USAGE)
                                    .strong(),
                            );
                            if ui.small_button(t.jump_to_source).clicked() {
                                pending_jump = Some(*line_idx);
                            }
                        });
                        if let Some(a) = text_with_view_button(
                            ui,
                            text,
                            &t.tools_def_title(line_idx + 1),
                            fs,
                            t,
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
                        ui.label(
                            RichText::new(format!("#{msg_no} {role}{origin_tag}"))
                                .color(color)
                                .strong(),
                        );
                        if let Some(a) = text_with_view_button(
                            ui,
                            text,
                            &t.msg_title(request_line + 1, msg_no),
                            fs,
                            t,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                    CtxItem::Think(txt) => {
                        ui.label(RichText::new(t.think_label).color(KIND_THINK).strong());
                        if let Some(a) = text_with_view_button(
                            ui,
                            txt,
                            &format!("L{} think", request_line + 1),
                            fs,
                            t,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                    CtxItem::Text(txt) => {
                        ui.label(RichText::new(t.text_label).color(KIND_TEXT).strong());
                        if let Some(a) = text_with_view_button(
                            ui,
                            txt,
                            &format!("L{} text", request_line + 1),
                            fs,
                            t,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                    CtxItem::ToolCall { id, name, args } => {
                        ui.label(
                            RichText::new(format!("🔧 {name}  ({})", short_id(id)))
                                .color(KIND_TOOL_CALL)
                                .strong(),
                        );
                        if let Some(a) = text_with_view_button(
                            ui,
                            args,
                            &t.tool_args_title(request_line + 1, name),
                            fs,
                            t,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                    CtxItem::ToolResult { id, output } => {
                        ui.label(
                            RichText::new(format!("  ↩ result ({})", short_id(id)))
                                .color(KIND_TOOL_RESULT),
                        );
                        if let Some(a) = text_with_view_button(
                            ui,
                            output,
                            &t.tool_result_title(request_line + 1),
                            fs,
                            t,
                        ) {
                            pending_open = Some(a);
                        }
                    }
                }
                ui.add_space(4.0);
            }
        });
        if let Some((title, content)) = pending_open {
            self.open_text_window(title, content);
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
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&self.status).color(Color32::GRAY).small());
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

fn fmt_bytes(n: usize) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1}MB", n as f64 / 1024.0 / 1024.0)
    } else if n >= 1024 {
        format!("{:.1}KB", n as f64 / 1024.0)
    } else {
        format!("{n}B")
    }
}

fn short_id(id: &str) -> String {
    if id.chars().count() > 12 {
        format!("{}…", id.chars().take(12).collect::<String>())
    } else {
        id.to_string()
    }
}
