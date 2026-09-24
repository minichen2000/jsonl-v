//! 长字符串解转义查看：多行排版、等宽字体、自动换行、复制。

use egui::{Color32, FontFamily, FontId, RichText};

use crate::lang::T;

/// 判定一个字符串值是否值得提供「纯文本查看」。
/// 此时字符串已被 serde_json 解码，原文里的 `\n`/`\t` 转义已是真实字符。
pub fn is_long_text(s: &str) -> bool {
    s.len() > 200 || s.contains('\n') || s.contains('\t')
}

#[derive(Clone)]
pub struct TextViewWindow {
    pub id: usize,       // 窗口唯一序号
    pub title: String,   // 如 "line 5 · systemPrompt"
    pub content: String, // 已解转义的文本
    pub wrap: bool,
    pub open: bool,
}

impl TextViewWindow {
    pub fn new(id: usize, title: String, content: String) -> Self {
        Self {
            id,
            title,
            content,
            wrap: true,
            open: true,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, font_size: f32, t: &T) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        // 标题用与正文协调的字号，避免默认标题栏过大
        let title = RichText::new(format!("📄 {}", self.title)).size(font_size + 1.0);
        egui::Window::new(title)
            .id(egui::Id::new(("text_view", self.id)))
            .default_size([700.0, 500.0])
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                let lines = self.content.lines().count();
                let chars = self.content.chars().count();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t.text_stats(lines, chars)).color(Color32::GRAY));
                    ui.checkbox(&mut self.wrap, t.wrap_toggle);
                    if ui.button(t.copy_all).clicked() {
                        ui.ctx().copy_text(self.content.clone());
                    }
                });
                ui.separator();
                let text = RichText::new(&self.content)
                    .font(FontId::new(font_size, FontFamily::Monospace));
                egui::ScrollArea::both().show(ui, |ui| {
                    if self.wrap {
                        ui.add(egui::Label::new(text).wrap());
                    } else {
                        ui.label(text);
                    }
                });
            });
        self.open = open;
    }
}
