#![windows_subsystem = "windows"]

mod app;
mod document;
mod icon;
mod json_tree;
mod json_view;
mod lang;
mod search;
mod settings;
mod shell_menu;
mod text_view;
mod wire;

use std::path::PathBuf;
use std::sync::Arc;

use app::JsonlApp;

fn main() -> eframe::Result<()> {
    let initial_path = std::env::args().nth(1).map(PathBuf::from);
    let mut options = eframe::NativeOptions::default();
    options.viewport = egui::ViewportBuilder::default()
        .with_title("jsonl-v — JSONL Viewer")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([640.0, 400.0])
        .with_icon(Arc::new(egui::IconData {
            rgba: icon::render_rgba(64),
            width: 64,
            height: 64,
        }));
    eframe::run_native(
        "jsonl-v",
        options,
        Box::new(move |cc| {
            load_cjk_font(&cc.egui_ctx);
            Ok(Box::new(JsonlApp::new(initial_path)))
        }),
    )
}

/// egui 默认字体不含 CJK 字形，从系统加载一个中文字体作为回退。
/// 只尝试纯 TTF/OTF（魔数校验），跳过 ttc 合集以免解析失败。
fn load_cjk_font(ctx: &egui::Context) {
    let candidates = [
        "C:/Windows/Fonts/simhei.ttf",   // 黑体，几乎必有
        "C:/Windows/Fonts/Deng.ttf",     // 等线
        "C:/Windows/Fonts/msjh.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/System/Library/Fonts/PingFang.ttc",
    ];
    for path in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let magic = bytes.get(..4).unwrap_or(&[]);
        let is_ttf = magic == [0x00, 0x01, 0x00, 0x00] || magic == b"OTTO" || magic == b"true";
        if !is_ttf {
            continue; // ttc 合集跳过
        }
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("cjk".into(), Arc::new(egui::FontData::from_owned(bytes)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            if let Some(list) = fonts.families.get_mut(&family) {
                list.push("cjk".into());
            }
        }
        ctx.set_fonts(fonts);
        return;
    }
    eprintln!("jsonl-v: no system CJK font found; CJK characters may render as boxes");
}
