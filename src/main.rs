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
            load_fonts(&cc.egui_ctx);
            Ok(Box::new(JsonlApp::new(initial_path)))
        }),
    )
}

/// 字体装配：
/// - egui 自带的 NotoEmoji 是子集，缺 🤖 等 emoji——内嵌完整 Noto Emoji
///   （黑白线条风，OFL 许可，见 assets/NotoEmoji-OFL.txt）作为 emoji 回退；
/// - egui 默认正文字体 Ubuntu-Light 字重太轻，英文界面发虚——从系统加载
///   常规字重字体顶替 Proportional 首位；
/// - egui 默认字体不含 CJK 字形，再从系统加载一个中文字体作回退。
///   系统字体只尝试纯 TTF/OTF（魔数校验），跳过 ttc 合集以免解析失败。
fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "noto-emoji".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/NotoEmoji-Regular.ttf"
        ))),
    );
    // 粗体子集（仅 🖥/🤖）：分侧徽标用粗线条更醒目；挂到独立字体族按需取用
    fonts.font_data.insert(
        "noto-emoji-bold".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/NotoEmoji-Bold-icons.ttf"
        ))),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(list) = fonts.families.get_mut(&family) {
            list.push("noto-emoji".into());
        }
    }
    let mut bold_family = vec!["noto-emoji-bold".to_owned(), "noto-emoji".to_owned()];
    if let Some(list) = fonts.families.get(&egui::FontFamily::Proportional) {
        bold_family.extend(list.iter().cloned());
    }
    fonts.families.insert(
        egui::FontFamily::Name(crate::app::EMOJI_BOLD_FAMILY.into()),
        bold_family,
    );
    // 读系统字体文件，魔数校验纯 TTF/OTF（ttc 合集跳过）
    let read_ttf = |path: &str| -> Option<Vec<u8>> {
        let bytes = std::fs::read(path).ok()?;
        let magic = bytes.get(..4).unwrap_or(&[]);
        let is_ttf = magic == [0x00, 0x01, 0x00, 0x00] || magic == b"OTTO" || magic == b"true";
        is_ttf.then_some(bytes)
    };
    // 常规字重：顶替 Proportional 首位，找不到则维持 Ubuntu-Light
    let ui_regular = [
        "C:/Windows/Fonts/segoeui.ttf", // Segoe UI，Windows 必有
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
    ];
    for path in ui_regular {
        let Some(bytes) = read_ttf(path) else {
            continue;
        };
        fonts
            .font_data
            .insert("ui-regular".into(), Arc::new(egui::FontData::from_owned(bytes)));
        if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            list.insert(0, "ui-regular".into());
        }
        break;
    }
    // CJK 回退
    let cjk_candidates = [
        "C:/Windows/Fonts/simhei.ttf", // 黑体，几乎必有
        "C:/Windows/Fonts/Deng.ttf",   // 等线
        "C:/Windows/Fonts/msjh.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/System/Library/Fonts/PingFang.ttc",
    ];
    let mut cjk_loaded = false;
    for path in cjk_candidates {
        let Some(bytes) = read_ttf(path) else {
            continue;
        };
        fonts
            .font_data
            .insert("cjk".into(), Arc::new(egui::FontData::from_owned(bytes)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            if let Some(list) = fonts.families.get_mut(&family) {
                list.push("cjk".into());
            }
        }
        cjk_loaded = true;
        break;
    }
    if !cjk_loaded {
        eprintln!("jsonl-v: no system CJK font found; CJK characters may render as boxes");
    }
    ctx.set_fonts(fonts);
}
