//! 结构化 JSON 查看窗口：可折叠树 / 格式化文本两种模式，复制全部。

use egui::{Color32, FontFamily, FontId, RichText};

use crate::json_tree::{self, TreeAction};
use crate::lang::T;

#[derive(Clone, Copy, PartialEq, Eq)]
enum JsonViewMode {
    Tree,
    Pretty,
}

pub struct JsonViewWindow {
    pub id: usize,
    pub title: String,
    pub value: serde_json::Value,
    pretty: String, // 格式化文本，创建时算一次
    pub bytes: usize, // 紧凑 JSON 的字节数
    pub open: bool,
    pub maximized: bool,
    view: JsonViewMode,
    default_open: Option<bool>, // 全展开/全折叠控制
    gen: u64,                   // 代际：变化时重置所有节点的展开状态
}

impl JsonViewWindow {
    pub fn new(id: usize, title: String, value: serde_json::Value) -> Self {
        let bytes = serde_json::to_string(&value).map(|s| s.len()).unwrap_or(0);
        let pretty = serde_json::to_string_pretty(&value).unwrap_or_default();
        Self {
            id,
            title,
            value,
            pretty,
            bytes,
            open: true,
            maximized: false,
            view: JsonViewMode::Tree,
            default_open: None,
            gen: 0,
        }
    }

    /// 渲染窗口；返回用户触发的树动作（如「纯文本查看」），由调用方处理。
    pub fn show(&mut self, ctx: &egui::Context, font_size: f32, t: &T) -> Option<TreeAction> {
        if !self.open {
            return None;
        }
        let mut open = self.open;
        let mut action = None;
        let title = RichText::new(format!("📦 {}", self.title)).size(font_size + 1.0);
        // 全屏用另一套窗口 id：egui 按 id 记忆位置尺寸，
        // 还原时自动回到普通模式之前的位置和大小。
        // v2 盐：eframe 跨会话持久化窗口位置尺寸，旧记忆里存着偏右的位置和
        // 旧尺寸，default_pos/default_size 都打不进去；换盐重置，让居中默认生效
        let win_id = if self.maximized {
            egui::Id::new(("json_view_max", self.id))
        } else {
            egui::Id::new(("json_view_v2", self.id))
        };
        let mut win = egui::Window::new(title.clone())
            .id(win_id)
            .frame(crate::app::popup_frame(ctx))
            .open(&mut open);
        win = if self.maximized {
            let (pos, size) = crate::app::maximized_pos_size(ctx, &title);
            win.fixed_pos(pos)
                .fixed_size(size)
                .resizable(false)
                .collapsible(false)
        } else {
            let size = egui::vec2(1100.0, 700.0);
            win.default_size(size)
                .default_pos(ctx.screen_rect().center() - size / 2.0)
                .resizable(true)
        };
        win.show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.view, JsonViewMode::Tree, t.tab_tree);
                ui.selectable_value(&mut self.view, JsonViewMode::Pretty, t.tab_pretty);
                ui.separator();
                ui.label(
                    RichText::new(t.json_view_size(crate::app::fmt_bytes(self.bytes)))
                        .color(Color32::GRAY),
                );
                if self.view == JsonViewMode::Tree {
                    if ui.small_button(t.expand_all).clicked() {
                        self.default_open = Some(true);
                        self.gen += 1;
                    }
                    if ui.small_button(t.collapse_all).clicked() {
                        self.default_open = Some(false);
                        self.gen += 1;
                    }
                }
                if ui.button(t.copy_all).clicked() {
                    ui.ctx().copy_text(self.pretty.clone());
                }
                let max_label = if self.maximized {
                    t.win_restore
                } else {
                    t.win_maximize
                };
                if ui.button(max_label).clicked() {
                    self.maximized = !self.maximized;
                }
            });
            ui.separator();
            match self.view {
                JsonViewMode::Tree => {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            action = json_tree::show_value_tree(
                                ui,
                                &self.value,
                                &self.title,
                                font_size,
                                self.default_open,
                                self.gen,
                                t,
                            );
                        });
                }
                JsonViewMode::Pretty => {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let edit_id = egui::Id::new(("json_pretty_edit", self.id));
                            crate::app::show_editable_text(
                                ui,
                                edit_id,
                                &mut self.pretty,
                                FontId::new(font_size, FontFamily::Monospace),
                                f32::INFINITY,
                                t,
                            );
                        });
                }
            }
        });
        self.open = open;
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_frames(
        win: &mut JsonViewWindow,
        ctx: &egui::Context,
        frames: usize,
        start: usize,
        t: &crate::lang::T,
    ) {
        for i in start..start + frames {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1920.0, 1080.0),
                )),
                time: Some(i as f64 / 60.0),
                ..Default::default()
            };
            ctx.run(input, |ctx| {
                win.show(ctx, 14.0, t);
            });
        }
    }

    fn sample_request_body() -> serde_json::Value {
        let long_text = "abcdefghij 一二三四五六七八九十".repeat(200);
        let mut messages = Vec::new();
        for i in 0..20 {
            messages.push(serde_json::json!({
                "role": if i % 2 == 0 { "user" } else { "assistant" },
                "content": [
                    {"type": "text", "text": long_text},
                    {"type": "tool_use", "id": format!("toolu_{i}"), "input": {"cmd": "ls", "args": [1, 2, 3, true, null]}},
                ],
            }));
        }
        serde_json::json!({
            "model": "test-model",
            "max_tokens": 8192,
            "system": [{"type": "text", "text": long_text}],
            "messages": messages,
            "tools": [{"name": "bash", "description": "run cmd", "input_schema": {"type": "object", "properties": {"cmd": {"type": "string"}}}}],
        })
    }

    /// 复现「点全屏后有几率卡死退出」：普通/全屏/还原 + 树/Pretty 两种模式反复切换渲染
    #[test]
    fn maximize_toggle_renders_fine() {
        let ctx = egui::Context::default();
        let t = crate::lang::tr(crate::lang::Lang::Zh);
        let mut win = JsonViewWindow::new(0, "line 1 · request".into(), sample_request_body());
        let mut frame = 0;
        for view in [JsonViewMode::Tree, JsonViewMode::Pretty] {
            win.view = view;
            run_frames(&mut win, &ctx, 3, frame, t);
            frame += 3;
            for _ in 0..5 {
                win.maximized = true;
                run_frames(&mut win, &ctx, 3, frame, t);
                frame += 3;
                win.maximized = false;
                run_frames(&mut win, &ctx, 3, frame, t);
                frame += 3;
            }
        }
    }

    fn run_frame_with_events(
        win: &mut JsonViewWindow,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        frame: usize,
        t: &crate::lang::T,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1920.0, 1080.0),
            )),
            time: Some(frame as f64 / 60.0),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            win.show(ctx, 14.0, t);
        });
    }

    fn click_at(
        win: &mut JsonViewWindow,
        ctx: &egui::Context,
        pos: egui::Pos2,
        frame: &mut usize,
        t: &crate::lang::T,
    ) {
        run_frame_with_events(win, ctx, vec![egui::Event::PointerMoved(pos)], *frame, t);
        *frame += 1;
        let btn = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        run_frame_with_events(win, ctx, vec![btn(true)], *frame, t);
        *frame += 1;
        run_frame_with_events(win, ctx, vec![btn(false)], *frame, t);
        *frame += 1;
    }

    /// 用真实指针事件点「最大化」「还原」按钮：覆盖点击事件处理路径
    #[test]
    fn maximize_button_click_renders_fine() {
        let ctx = egui::Context::default();
        let t = crate::lang::tr(crate::lang::Lang::Zh);
        let mut win = JsonViewWindow::new(0, "line 1 · request".into(), sample_request_body());
        let mut frame = 0;
        run_frames(&mut win, &ctx, 3, frame, t);
        frame += 3;

        // 「最大化」在工具行右端；标题栏（含关闭按钮）在顶部 ~32px 内，扫描时避开
        let click_toggle = |win: &mut JsonViewWindow,
                            win_id: egui::Id,
                            frame: &mut usize,
                            want: bool,
                            ctx: &egui::Context,
                            t: &crate::lang::T| {
            let rect = ctx
                .memory(|m| m.area_rect(win_id))
                .expect("window area should exist");
            for dy in [42.0, 50.0, 58.0] {
                let mut dx = 40.0_f32;
                while dx < rect.width() - 40.0 {
                    click_at(win, ctx, egui::pos2(rect.right() - dx, rect.top() + dy), frame, t);
                    if win.maximized == want {
                        return;
                    }
                    dx += 15.0;
                }
            }
            panic!("toggle button not found (want maximized={want})");
        };

        click_toggle(
            &mut win,
            egui::Id::new(("json_view_v2", 0)),
            &mut frame,
            true,
            &ctx,
            t,
        );
        assert!(win.maximized);
        run_frames(&mut win, &ctx, 10, frame, t);
        frame += 10;

        click_toggle(
            &mut win,
            egui::Id::new(("json_view_max", 0)),
            &mut frame,
            false,
            &ctx,
            t,
        );
        assert!(!win.maximized);
        run_frames(&mut win, &ctx, 10, frame, t);
    }
}
