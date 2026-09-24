use serde_json::Value;

use crate::document::JsonlDocument;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireKind {
    LlmRequest,
    ToolCall,
    ToolResult,
    Think,
    Text,
    UsageRecord,
    Interaction,
    NotWire,
}

/// Classify a parsed line for wire-mode coloring. Returns NotWire for
/// anything that is not one of the specially-colored event types.
pub fn classify(parsed: &Result<Value, String>) -> WireKind {
    let Ok(v) = parsed else {
        return WireKind::NotWire; // 坏行由调用方单独标红
    };
    let Some(t) = v.get("type").and_then(Value::as_str) else {
        return WireKind::NotWire;
    };
    match t {
        "llm.request" => WireKind::LlmRequest,
        "usage.record" => WireKind::UsageRecord,
        "interaction.request" | "interaction.resolved" => WireKind::Interaction,
        "context.append_loop_event" => match v
            .get("event")
            .and_then(|e| e.get("type"))
            .and_then(Value::as_str)
        {
            Some("tool.call") => WireKind::ToolCall,
            Some("tool.result") => WireKind::ToolResult,
            Some("content.part") => match v
                .get("event")
                .and_then(|e| e.get("part"))
                .and_then(|p| p.get("type"))
                .and_then(Value::as_str)
            {
                Some("think") => WireKind::Think,
                Some("text") => WireKind::Text,
                _ => WireKind::NotWire,
            },
            _ => WireKind::NotWire,
        },
        _ => WireKind::NotWire,
    }
}

/// Detect whether a document is a Kimi Code wire.jsonl journal.
pub fn is_wire_file(doc: &JsonlDocument) -> bool {
    // 前 10 行内存在带 protocol_version 的 metadata 行
    let n = doc.line_count().min(10);
    for i in 0..n {
        let raw = doc.raw_line(i);
        if raw.contains("\"type\":\"metadata\"") && raw.contains("protocol_version") {
            return true;
        }
    }
    // 或任意行含 "type":"llm.request"（直接在原始字节上搜，不 parse）
    memchr::memmem::find(doc.raw_bytes(), b"\"type\":\"llm.request\"").is_some()
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_other: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
}

#[derive(Debug, Clone)]
pub struct RequestEntry {
    pub seq: usize, // 1-based
    pub line_idx: usize,
    pub turn_step: String,
    pub message_count: u64,
    pub usage: Option<Usage>, // 紧随其后的 usage.record
}

/// Extract the llm.request timeline. Only parses lines whose raw bytes
/// contain the marker substrings, so it is cheap on large files.
pub fn timeline(doc: &JsonlDocument) -> Vec<RequestEntry> {
    let mut entries: Vec<RequestEntry> = Vec::new();
    let mut pending: Option<usize> = None; // index into entries awaiting usage
    for i in 0..doc.line_count() {
        let raw = doc.raw_line(i);
        let is_req = raw.contains("\"type\":\"llm.request\"");
        let is_usage = !is_req && raw.contains("\"type\":\"usage.record\"");
        if !is_req && !is_usage {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        if is_req {
            entries.push(RequestEntry {
                seq: entries.len() + 1,
                line_idx: i,
                turn_step: v
                    .get("turnStep")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
                message_count: v.get("messageCount").and_then(Value::as_u64).unwrap_or(0),
                usage: None,
            });
            pending = Some(entries.len() - 1);
        } else if let Some(idx) = pending {
            if entries[idx].usage.is_none() {
                let u = v.get("usage");
                entries[idx].usage = Some(Usage {
                    input_other: u
                        .and_then(|x| x.get("inputOther"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    output: u
                        .and_then(|x| x.get("output"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    cache_read: u
                        .and_then(|x| x.get("inputCacheRead"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    cache_creation: u
                        .and_then(|x| x.get("inputCacheCreation"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                });
            }
            pending = None;
        }
    }
    entries
}

/// One reconstructed context item, in the order it entered the context.
#[derive(Debug, Clone)]
pub enum CtxItem {
    /// context.append_message: a persisted message (user/assistant/injection)
    Message {
        role: String,
        text: String,
        origin: Option<String>,
    },
    /// content.part think fragment
    Think(String),
    /// content.part text fragment
    Text(String),
    /// tool.call
    ToolCall {
        id: String,
        name: String,
        args: String,
    },
    /// tool.result
    ToolResult { id: String, output: String },
}

/// Rebuild the approximate messages array actually sent by the request at
/// `request_line`: all context.append_message / context.append_loop_event
/// records before that line, in order.
pub fn rebuild_context(doc: &JsonlDocument, request_line: usize) -> Vec<CtxItem> {
    let mut items = Vec::new();
    let end = request_line.min(doc.line_count());
    for i in 0..end {
        let raw = doc.raw_line(i);
        let is_msg = raw.contains("\"type\":\"context.append_message\"");
        let is_loop = !is_msg && raw.contains("\"type\":\"context.append_loop_event\"");
        if !is_msg && !is_loop {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        if is_msg {
            let m = v.get("message").cloned().unwrap_or(Value::Null);
            let role = m
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_string();
            let text = extract_content_text(&m);
            let origin = m
                .get("origin")
                .and_then(|o| o.get("kind"))
                .and_then(Value::as_str)
                .map(str::to_string);
            // 附带持久化的 toolCalls（如有）
            let mut it = vec![CtxItem::Message { role, text, origin }];
            if let Some(calls) = m.get("toolCalls").and_then(Value::as_array) {
                for c in calls {
                    it.push(CtxItem::ToolCall {
                        id: c
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        name: c
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                            .to_string(),
                        args: c
                            .get("args")
                            .map(|a| a.to_string())
                            .unwrap_or_default(),
                    });
                }
            }
            items.extend(it);
        } else {
            let e = match v.get("event") {
                Some(e) => e,
                None => continue,
            };
            match e.get("type").and_then(Value::as_str) {
                Some("content.part") => {
                    let p = e.get("part").cloned().unwrap_or(Value::Null);
                    match p.get("type").and_then(Value::as_str) {
                        Some("think") => items.push(CtxItem::Think(
                            p.get("think")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                        )),
                        Some("text") => items.push(CtxItem::Text(
                            p.get("text")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                        )),
                        _ => {}
                    }
                }
                Some("tool.call") => items.push(CtxItem::ToolCall {
                    id: e
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    name: e
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string(),
                    args: e
                        .get("args")
                        .map(|a| {
                            serde_json::to_string_pretty(a).unwrap_or_else(|_| a.to_string())
                        })
                        .unwrap_or_default(),
                }),
                Some("tool.result") => items.push(CtxItem::ToolResult {
                    id: e
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    output: e
                        .get("result")
                        .and_then(|r| r.get("output"))
                        .map(|o| {
                            o.as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| o.to_string())
                        })
                        .unwrap_or_default(),
                }),
                _ => {} // step.begin 等不进消息序列
            }
        }
    }
    items
}

/// Estimate the message count of a rebuilt context: each Message = 1,
/// a consecutive run of Think/Text/ToolCall = 1 assistant message,
/// each ToolResult = 1. Should equal llm.request.messageCount.
pub fn estimate_message_count(items: &[CtxItem]) -> u64 {
    let mut count = 0u64;
    let mut in_assistant_run = false;
    for it in items {
        match it {
            CtxItem::Message { .. } => {
                count += 1;
                in_assistant_run = false;
            }
            CtxItem::Think(_) | CtxItem::Text(_) | CtxItem::ToolCall { .. } => {
                if !in_assistant_run {
                    count += 1;
                    in_assistant_run = true;
                }
            }
            CtxItem::ToolResult { .. } => {
                count += 1;
                in_assistant_run = false;
            }
        }
    }
    count
}

fn extract_content_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| {
                if p.get("type").and_then(Value::as_str) == Some("text") {
                    p.get("text").and_then(Value::as_str).map(str::to_string)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_doc() -> JsonlDocument {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("docs")
            .join("wire.jsonl");
        JsonlDocument::open(path).unwrap()
    }

    #[test]
    fn detects_wire_file() {
        let doc = sample_doc();
        assert!(is_wire_file(&doc));
    }

    #[test]
    fn non_wire_file_not_detected() {
        let path = std::env::temp_dir().join("jsonl_v_test_notwire.jsonl");
        std::fs::write(&path, b"{\"a\":1}\n{\"type\":\"thing\"}\n").unwrap();
        let doc = JsonlDocument::open(path).unwrap();
        assert!(!is_wire_file(&doc));
    }

    #[test]
    fn timeline_matches_measured_data() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        assert_eq!(tl.len(), 14);
        let expected_counts = [2u64, 6, 9, 11, 14, 16, 19, 21, 23, 25, 27, 29, 31, 33];
        let expected_steps = [
            "0.1", "0.2", "0.3", "0.4", "0.5", "0.6", "0.7", "1.1", "1.2", "2.1", "3.1", "4.1",
            "4.2", "4.3",
        ];
        let expected_lines = [
            13usize, 25, 35, 44, 57, 66, 80, 111, 119, 137, 153, 169, 177, 185,
        ]; // 规格给的是 1-based 行号 14,26,...
        for (i, e) in tl.iter().enumerate() {
            assert_eq!(e.seq, i + 1);
            assert_eq!(e.message_count, expected_counts[i], "entry {} count", i);
            assert_eq!(e.turn_step, expected_steps[i], "entry {} step", i);
            assert_eq!(e.line_idx, expected_lines[i], "entry {} line", i);
            // 每次请求后紧跟 usage.record
            assert!(e.usage.is_some(), "entry {} missing usage", i);
        }
        // 首个请求的 usage 实测值
        let u = tl[0].usage.as_ref().unwrap();
        assert_eq!(u.output, 254);
        assert_eq!(u.cache_read, 19200);
    }

    #[test]
    fn rebuild_first_request_has_two_messages() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        let items = rebuild_context(&doc, tl[0].line_idx);
        assert_eq!(estimate_message_count(&items), 2);
        // 两条都是 append_message：用户提问 + plan mode 注入
        let msgs: Vec<_> = items
            .iter()
            .filter(|i| matches!(i, CtxItem::Message { .. }))
            .collect();
        assert_eq!(msgs.len(), 2);
        match msgs[1] {
            CtxItem::Message { origin, .. } => {
                assert_eq!(origin.as_deref(), Some("injection"));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn rebuild_counts_match_message_count_for_all_requests() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        for e in &tl {
            let items = rebuild_context(&doc, e.line_idx);
            assert_eq!(
                estimate_message_count(&items),
                e.message_count,
                "turnStep {} 重建消息数应与 messageCount 一致",
                e.turn_step
            );
        }
    }

    #[test]
    fn tool_call_result_pairing_ids_present() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        // 0.7 之前应已有若干工具调用与结果
        let items = rebuild_context(&doc, tl[6].line_idx);
        let calls: Vec<&str> = items
            .iter()
            .filter_map(|i| match i {
                CtxItem::ToolCall { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        let results: Vec<&str> = items
            .iter()
            .filter_map(|i| match i {
                CtxItem::ToolResult { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert!(!calls.is_empty());
        assert_eq!(calls.len(), results.len());
        // 每个 call 都有对应 result
        for c in &calls {
            assert!(results.contains(c));
        }
    }

    #[test]
    fn classify_events() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        let req = doc.peek_info(tl[0].line_idx);
        assert_eq!(classify(&req.parsed), WireKind::LlmRequest);
        let usage = doc.peek_info(tl[0].line_idx + 1);
        assert_eq!(classify(&usage.parsed), WireKind::UsageRecord);
        // 样例里第一个 tool.call 在 18 行（1-based），即 17（0-based）
        let tc = doc.peek_info(17);
        assert_eq!(classify(&tc.parsed), WireKind::ToolCall);
    }
}
