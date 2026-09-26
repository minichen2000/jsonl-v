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
    /// profile.bind 里的系统提示词原文（请求体的 system 部分）
    SystemPrompt { line_idx: usize, text: String },
    /// llm.tools_snapshot 里的工具定义原文（请求体的 tools 部分）
    ToolsDef {
        line_idx: usize,
        text: String,
        tool_count: usize,
    },
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
    ToolResult {
        id: String,
        /// 配对 tool.call 的工具名（重建时查表得到，可能缺失）
        name: Option<String>,
        output: String,
    },
}

/// 上下文条目的来源侧：宿主/用户一侧，还是 LLM 产出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Host,
    Llm,
}

impl CtxItem {
    /// 该条目在请求体 messages 里的 role（ToolsDef 不在 messages 中，归为 "tools"）。
    pub fn role(&self) -> &str {
        match self {
            CtxItem::SystemPrompt { .. } => "system",
            CtxItem::ToolsDef { .. } => "tools",
            CtxItem::Message { role, .. } => role.as_str(),
            CtxItem::Think(_) | CtxItem::Text(_) | CtxItem::ToolCall { .. } => "assistant",
            CtxItem::ToolResult { .. } => "tool",
        }
    }

    /// 该条目来自宿主/用户一侧还是 LLM 一侧。
    pub fn side(&self) -> Side {
        match self {
            CtxItem::Think(_) | CtxItem::Text(_) | CtxItem::ToolCall { .. } => Side::Llm,
            CtxItem::Message { role, .. } if role == "assistant" => Side::Llm,
            _ => Side::Host,
        }
    }
}

/// Rebuild the approximate messages array actually sent by the request at
/// `request_line`: all context.append_message / context.append_loop_event
/// records before that line, in order.
pub fn rebuild_context(doc: &JsonlDocument, request_line: usize) -> Vec<CtxItem> {
    let mut items = Vec::new();
    let mut system_prompt: Option<CtxItem> = None; // 最新的 profile.bind
    let mut tools_def: Option<CtxItem> = None; // 最新的 llm.tools_snapshot
    let mut call_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new(); // toolCallId → 工具名
    let end = request_line.min(doc.line_count());
    for i in 0..end {
        let raw = doc.raw_line(i);
        let is_msg = raw.contains("\"type\":\"context.append_message\"");
        let is_loop = !is_msg && raw.contains("\"type\":\"context.append_loop_event\"");
        let is_profile = !is_msg && !is_loop && raw.contains("\"type\":\"profile.bind\"");
        let is_tools = !is_msg && !is_loop && !is_profile
            && raw.contains("\"type\":\"llm.tools_snapshot\"");
        if is_profile {
            if let Ok(v) = serde_json::from_str::<Value>(raw) {
                if let Some(sp) = v.get("systemPrompt").and_then(Value::as_str) {
                    system_prompt = Some(CtxItem::SystemPrompt {
                        line_idx: i,
                        text: sp.to_string(),
                    });
                }
            }
            continue;
        }
        if is_tools {
            if let Ok(v) = serde_json::from_str::<Value>(raw) {
                if let Some(tools) = v.get("tools").and_then(Value::as_array) {
                    tools_def = Some(CtxItem::ToolsDef {
                        line_idx: i,
                        tool_count: tools.len(),
                        text: serde_json::to_string_pretty(&Value::Array(tools.clone()))
                            .unwrap_or_default(),
                    });
                }
            }
            continue;
        }
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
                    let id = c
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let name = c
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string();
                    call_names.insert(id.clone(), name.clone());
                    it.push(CtxItem::ToolCall {
                        id,
                        name,
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
                Some("tool.call") => {
                    let id = e
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let name = e
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_string();
                    call_names.insert(id.clone(), name.clone());
                    items.push(CtxItem::ToolCall {
                        id,
                        name,
                        args: e
                            .get("args")
                            .map(|a| {
                                serde_json::to_string_pretty(a).unwrap_or_else(|_| a.to_string())
                            })
                            .unwrap_or_default(),
                    });
                }
                Some("tool.result") => {
                    let id = e
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let name = call_names.get(&id).cloned();
                    items.push(CtxItem::ToolResult {
                        id,
                        name,
                        output: e
                            .get("result")
                            .and_then(|r| r.get("output"))
                            .map(|o| {
                                o.as_str()
                                    .map(str::to_string)
                                    .unwrap_or_else(|| o.to_string())
                            })
                            .unwrap_or_default(),
                    });
                }
                _ => {} // step.begin 等不进消息序列
            }
        }
    }
    // system prompt 与工具定义在请求体里位于 messages 之前
    let mut head = Vec::with_capacity(2);
    if let Some(sp) = system_prompt {
        head.push(sp);
    }
    if let Some(td) = tools_def {
        head.push(td);
    }
    head.extend(items);
    head
}

/// Build the full request body (OpenAI chat-completions style) for the
/// llm.request at `request_line`: model / max_tokens from the request record,
/// messages rebuilt from context events, tools from the tools snapshot.
pub fn build_request_body(doc: &JsonlDocument, request_line: usize) -> Option<Value> {
    let raw = doc.raw_line(request_line);
    if !raw.contains("\"type\":\"llm.request\"") {
        return None;
    }
    let req: Value = serde_json::from_str(raw).ok()?;
    let items = rebuild_context(doc, request_line);

    let mut messages: Vec<Value> = Vec::new();
    let mut tools: Option<Value> = None;
    // 连续的 Think/Text/ToolCall 累积为一条 assistant 消息
    let mut parts: Vec<Value> = Vec::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    macro_rules! flush_assistant {
        () => {
            if !parts.is_empty() || !tool_calls.is_empty() {
                let mut m =
                    serde_json::json!({"role": "assistant", "content": std::mem::take(&mut parts)});
                if !tool_calls.is_empty() {
                    m["tool_calls"] = Value::Array(std::mem::take(&mut tool_calls));
                }
                messages.push(m);
            }
        };
    }
    for item in items {
        match item {
            CtxItem::SystemPrompt { text, .. } => {
                messages.push(serde_json::json!({"role": "system", "content": text}));
            }
            CtxItem::ToolsDef { text, .. } => {
                tools = serde_json::from_str(&text).ok();
            }
            CtxItem::Message { role, text, .. } => {
                flush_assistant!();
                messages.push(serde_json::json!({"role": role, "content": text}));
            }
            CtxItem::Think(t) => parts.push(serde_json::json!({"type": "think", "think": t})),
            CtxItem::Text(t) => parts.push(serde_json::json!({"type": "text", "text": t})),
            CtxItem::ToolCall { id, name, args } => {
                tool_calls.push(serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": {"name": name, "arguments": args},
                }));
            }
            CtxItem::ToolResult { id, output, .. } => {
                flush_assistant!();
                messages.push(
                    serde_json::json!({"role": "tool", "tool_call_id": id, "content": output}),
                );
            }
        }
    }
    flush_assistant!();

    let mut body = serde_json::Map::new();
    body.insert("model".into(), req.get("model").cloned().unwrap_or(Value::Null));
    body.insert(
        "max_tokens".into(),
        req.get("maxTokens").cloned().unwrap_or(Value::Null),
    );
    body.insert("messages".into(), Value::Array(messages));
    if let Some(t) = tools {
        body.insert("tools".into(), t);
    }
    Some(Value::Object(body))
}

/// Estimate the message count of a rebuilt context: each Message = 1,
/// a consecutive run of Think/Text/ToolCall = 1 assistant message,
/// each ToolResult = 1. Should equal llm.request.messageCount.
pub fn estimate_message_count(items: &[CtxItem]) -> u64 {
    let mut count = 0u64;
    let mut in_assistant_run = false;
    for it in items {
        match it {
            // system prompt / 工具定义不属于 messages 数组，不计数
            CtxItem::SystemPrompt { .. } | CtxItem::ToolsDef { .. } => {}
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

/// Estimate the byte size of the full request body: system prompt + tools
/// + every message/thought/tool payload.
pub fn estimate_context_bytes(items: &[CtxItem]) -> usize {
    items.iter().map(ctx_item_bytes).sum()
}

/// Single context item's estimated byte size.
pub fn ctx_item_bytes(it: &CtxItem) -> usize {
    match it {
        CtxItem::SystemPrompt { text, .. } => text.len(),
        CtxItem::ToolsDef { text, .. } => text.len(),
        CtxItem::Message { text, .. } => text.len(),
        CtxItem::Think(s) | CtxItem::Text(s) => s.len(),
        CtxItem::ToolCall { args, .. } => args.len(),
        CtxItem::ToolResult { output, .. } => output.len(),
    }
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
    fn rebuild_includes_system_prompt_and_tools_head() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        let items = rebuild_context(&doc, tl[0].line_idx);
        // 前两项应是系统提示词与工具定义（来自样例第 5 行起的 profile.bind / tools_snapshot）
        match &items[0] {
            CtxItem::SystemPrompt { line_idx, text } => {
                assert_eq!(*line_idx, 4); // 第 5 行（1-based）
                assert!(text.contains("Kimi Code"));
            }
            other => panic!("首项应为 SystemPrompt，实际 {other:?}"),
        }
        match &items[1] {
            CtxItem::ToolsDef {
                tool_count, text, ..
            } => {
                assert!(*tool_count > 0);
                assert!(text.contains("\"name\""));
            }
            other => panic!("第二项应为 ToolsDef，实际 {other:?}"),
        }
        // 二者不计入消息数
        assert_eq!(estimate_message_count(&items), tl[0].message_count);
    }

    #[test]
    fn estimate_context_bytes_counts_all_parts() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        let items = rebuild_context(&doc, tl[0].line_idx);
        let bytes = estimate_context_bytes(&items);
        // 至少包含系统提示词（约 10KB 量级）
        assert!(bytes > 1000, "bytes={bytes}");
    }

    #[test]
    fn request_body_message_count_matches_for_all_requests() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        for e in &tl {
            let body = build_request_body(&doc, e.line_idx).expect("build_request_body");
            let messages = body["messages"].as_array().expect("messages array");
            // system 消息（messages[0]）不计入 messageCount
            assert_eq!(
                messages.len() as u64,
                e.message_count + 1,
                "turnStep {} 请求体消息数应为 messageCount + 1（system）",
                e.turn_step
            );
        }
    }

    #[test]
    fn request_body_structure_first_request() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        let body = build_request_body(&doc, tl[0].line_idx).expect("build_request_body");
        assert_eq!(body["model"].as_str(), Some("k3-256k"));
        assert_eq!(body["max_tokens"].as_u64(), Some(262144));
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"].as_str(), Some("system"));
        assert!(messages[0]["content"].as_str().unwrap().contains("Kimi Code"));
        let tools = body["tools"].as_array().expect("tools array");
        assert!(!tools.is_empty());
    }

    #[test]
    fn request_body_tool_results_pair_with_tool_calls() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        // 0.7 之前应已有若干工具调用与结果
        let body = build_request_body(&doc, tl[6].line_idx).expect("build_request_body");
        let messages = body["messages"].as_array().unwrap();
        let mut call_ids: Vec<&str> = Vec::new();
        let mut result_ids: Vec<&str> = Vec::new();
        for m in messages {
            match m["role"].as_str() {
                Some("assistant") => {
                    if let Some(calls) = m["tool_calls"].as_array() {
                        call_ids.extend(calls.iter().filter_map(|c| c["id"].as_str()));
                    }
                }
                Some("tool") => {
                    result_ids.push(m["tool_call_id"].as_str().expect("tool_call_id"));
                }
                _ => {}
            }
        }
        assert!(!call_ids.is_empty());
        assert_eq!(call_ids.len(), result_ids.len());
        for r in &result_ids {
            assert!(call_ids.contains(r), "tool_call_id {r} 无配对");
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

    #[test]
    fn ctx_item_role_and_side() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        let items = rebuild_context(&doc, tl[6].line_idx);
        let mut saw_think = false;
        let mut saw_text = false;
        let mut saw_call = false;
        let mut saw_result = false;
        for it in &items {
            match it {
                CtxItem::SystemPrompt { .. } => {
                    assert_eq!(it.role(), "system");
                    assert_eq!(it.side(), Side::Host);
                }
                CtxItem::ToolsDef { .. } => {
                    assert_eq!(it.role(), "tools");
                    assert_eq!(it.side(), Side::Host);
                }
                CtxItem::Think(_) => {
                    saw_think = true;
                    assert_eq!(it.role(), "assistant");
                    assert_eq!(it.side(), Side::Llm);
                }
                CtxItem::Text(_) => {
                    saw_text = true;
                    assert_eq!(it.role(), "assistant");
                    assert_eq!(it.side(), Side::Llm);
                }
                CtxItem::ToolCall { .. } => {
                    saw_call = true;
                    assert_eq!(it.role(), "assistant");
                    assert_eq!(it.side(), Side::Llm);
                }
                CtxItem::ToolResult { .. } => {
                    saw_result = true;
                    assert_eq!(it.role(), "tool");
                    assert_eq!(it.side(), Side::Host);
                }
                CtxItem::Message { role, .. } => {
                    assert_eq!(it.role(), role.as_str());
                    let expect = if role == "assistant" { Side::Llm } else { Side::Host };
                    assert_eq!(it.side(), expect);
                }
            }
        }
        assert!(saw_think && saw_text && saw_call && saw_result);
    }

    #[test]
    fn tool_result_carries_paired_tool_name() {
        let doc = sample_doc();
        let tl = timeline(&doc);
        let items = rebuild_context(&doc, tl[6].line_idx);
        let mut names: std::collections::HashMap<&str, &str> =
            std::collections::HashMap::new();
        for it in &items {
            match it {
                CtxItem::ToolCall { id, name, .. } => {
                    names.insert(id.as_str(), name.as_str());
                }
                CtxItem::ToolResult { id, name, .. } => {
                    let expect = names.get(id.as_str()).copied().expect("result 应有配对 call");
                    assert_eq!(name.as_deref(), Some(expect), "result 应带上配对工具名");
                }
                _ => {}
            }
        }
        assert!(!names.is_empty());
    }
}
