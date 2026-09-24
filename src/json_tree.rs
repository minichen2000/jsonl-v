//! serde_json::Value → egui 可折叠树，键/字符串/数字/布尔/null 着色。

use egui::{Color32, FontFamily, FontId, RichText, Ui};
use serde_json::Value;

use crate::lang::T;
use crate::text_view;

pub const KEY_COLOR: Color32 = Color32::from_rgb(0x7d, 0xc4, 0xe4); // 浅蓝
pub const STR_COLOR: Color32 = Color32::from_rgb(0xa6, 0xda, 0x95); // 绿
pub const NUM_COLOR: Color32 = Color32::from_rgb(0xf5, 0xa9, 0x7f); // 橙
pub const BOOL_COLOR: Color32 = Color32::from_rgb(0xc6, 0xa0, 0xf6); // 紫
pub const NULL_COLOR: Color32 = Color32::GRAY;

/// 渲染动作：目前只有「打开纯文本查看」。
pub enum TreeAction {
    OpenText { title: String, content: String },
}

/// 渲染整个 JSON 树（根节点）。返回用户触发的动作（每帧至多一个）。
/// `default_open`：Some(b) 时所有节点默认展开/折叠（全展开/全折叠），
/// None 时仅第一层展开。`gen` 变化会重置所有节点的展开状态。
pub fn show_value_tree(
    ui: &mut Ui,
    root: &Value,
    path_prefix: &str,
    font_size: f32,
    default_open: Option<bool>,
    gen: u64,
    t: &T,
) -> Option<TreeAction> {
    let mut action = None;
    show_node(ui, root, path_prefix, None, 0, font_size, default_open, gen, t, &mut action);
    action
}

fn short(s: &str, max: usize) -> String {
    let s = s.replace('\n', "\\n").replace('\t', "\\t");
    if s.chars().count() <= max {
        s
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

fn mono(s: impl Into<String>, font_size: f32) -> RichText {
    RichText::new(s.into()).font(FontId::new(font_size, FontFamily::Monospace))
}

#[allow(clippy::too_many_arguments)]
fn scalar_ui(
    ui: &mut Ui,
    path: &str,
    key_label: Option<&str>,
    value_text: String,
    color: Color32,
    string_content: Option<&str>,
    font_size: f32,
    t: &T,
    action: &mut Option<TreeAction>,
) {
    ui.horizontal(|ui| {
        if let Some(k) = key_label {
            ui.label(mono(k, font_size).color(KEY_COLOR));
            ui.label(mono(":", font_size));
        }
        ui.label(mono(value_text, font_size).color(color));
        if let Some(s) = string_content {
            if text_view::is_long_text(s) && ui.small_button(t.view_text_btn).clicked() {
                *action = Some(TreeAction::OpenText {
                    title: path.to_string(),
                    content: s.to_string(),
                });
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn show_node(
    ui: &mut Ui,
    v: &Value,
    path: &str,
    key: Option<String>,
    depth: usize,
    font_size: f32,
    default_open: Option<bool>,
    gen: u64,
    t: &T,
    action: &mut Option<TreeAction>,
) {
    let key_label = key.clone();
    let open_state = default_open.unwrap_or(depth < 1);
    match v {
        Value::Null => scalar_ui(
            ui,
            path,
            key_label.as_deref(),
            "null".into(),
            NULL_COLOR,
            None,
            font_size,
            t,
            action,
        ),
        Value::Bool(b) => scalar_ui(
            ui,
            path,
            key_label.as_deref(),
            b.to_string(),
            BOOL_COLOR,
            None,
            font_size,
            t,
            action,
        ),
        Value::Number(n) => scalar_ui(
            ui,
            path,
            key_label.as_deref(),
            n.to_string(),
            NUM_COLOR,
            None,
            font_size,
            t,
            action,
        ),
        Value::String(s) => {
            let shown = format!("\"{}\"", short(s, 120));
            scalar_ui(
                ui,
                path,
                key_label.as_deref(),
                shown,
                STR_COLOR,
                Some(s.as_str()),
                font_size,
                t,
                action,
            );
        }
        Value::Array(arr) => {
            let header = match &key_label {
                Some(k) => format!("{k}: [ {} ]", t.n_items(arr.len())),
                None => format!("[ {} ]", t.n_items(arr.len())),
            };
            let id = ui.make_persistent_id((path, depth, gen));
            egui::CollapsingHeader::new(mono(header, font_size).color(KEY_COLOR))
                .id_salt(id)
                .default_open(open_state)
                .show(ui, |ui| {
                    for (i, item) in arr.iter().enumerate() {
                        show_node(
                            ui,
                            item,
                            &format!("{path}[{i}]"),
                            Some(format!("[{i}]")),
                            depth + 1,
                            font_size,
                            default_open,
                            gen,
                            t,
                            action,
                        );
                    }
                });
        }
        Value::Object(map) => {
            let header = match &key_label {
                Some(k) => format!("{k}: {{ {} }}", t.n_keys(map.len())),
                None => format!("{{ {} }}", t.n_keys(map.len())),
            };
            let id = ui.make_persistent_id((path, depth, gen));
            let mut body = |ui: &mut Ui| {
                for (k, val) in map {
                    show_node(
                        ui,
                        val,
                        &format!("{path}.{k}"),
                        Some(k.clone()),
                        depth + 1,
                        font_size,
                        default_open,
                        gen,
                        t,
                        action,
                    );
                }
            };
            if depth == 0 && key_label.is_none() {
                // 根对象直接展开，不多包一层
                body(ui);
            } else {
                egui::CollapsingHeader::new(mono(header, font_size).color(KEY_COLOR))
                    .id_salt(id)
                    .default_open(open_state)
                    .show(ui, body);
            }
        }
    }
}
