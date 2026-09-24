//! 结构化 JSON 查看窗口：可折叠树 / 纯美化文本两种模式，复制全部。

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
    pretty: String, // 美化文本，创建时算一次
    pub bytes: usize, // 紧凑 JSON 的字节数
    pub open: bool,
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
        egui::Window::new(title)
            .id(egui::Id::new(("json_view", self.id)))
            .default_size([900.0, 700.0])
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
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
                                ui.label(
                                    RichText::new(&self.pretty)
                                        .font(FontId::new(font_size, FontFamily::Monospace)),
                                );
                            });
                    }
                }
            });
        self.open = open;
        action
    }
}
