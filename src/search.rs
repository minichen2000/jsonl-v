use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::document::DocSnapshot;

#[derive(Debug, Clone)]
pub struct Query {
    pub raw: String,
    pub case_sensitive: bool,
    /// `key:value` 形式解析结果
    pub key_value: Option<(String, String)>,
}

impl Query {
    pub fn parse(raw: &str, case_sensitive: bool) -> Option<Self> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        // key:value：冒号前是合法键名（字母/数字/._-）才按此解析
        let key_value = raw.split_once(':').and_then(|(k, v)| {
            let k = k.trim();
            let v = v.trim();
            if !k.is_empty()
                && !v.is_empty()
                && k.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            {
                Some((k.to_string(), v.to_string()))
            } else {
                None
            }
        });
        Some(Self {
            raw: raw.to_string(),
            case_sensitive,
            key_value,
        })
    }

    fn line_matches(&self, line: &str) -> bool {
        if let Some((key, value)) = &self.key_value {
            // 先子串预筛，命中才 parse 验证
            let needle_ok = if self.case_sensitive {
                line.contains(value.as_str())
            } else {
                line.to_lowercase().contains(&value.to_lowercase())
            };
            if !needle_ok {
                return false;
            }
            return serde_json::from_str::<serde_json::Value>(line)
                .map(|v| json_has_kv(&v, key, value, self.case_sensitive))
                .unwrap_or(false);
        }
        if self.case_sensitive {
            line.contains(&self.raw)
        } else {
            line.to_lowercase().contains(&self.raw.to_lowercase())
        }
    }
}

fn json_has_kv(v: &serde_json::Value, key: &str, value: &str, case_sensitive: bool) -> bool {
    match v {
        serde_json::Value::Object(map) => map.iter().any(|(k, val)| {
            let key_eq = if case_sensitive {
                k == key
            } else {
                k.eq_ignore_ascii_case(key)
            };
            if key_eq && value_matches(val, value, case_sensitive) {
                return true;
            }
            json_has_kv(val, key, value, case_sensitive)
        }),
        serde_json::Value::Array(arr) => arr.iter().any(|x| json_has_kv(x, key, value, case_sensitive)),
        _ => false,
    }
}

fn value_matches(v: &serde_json::Value, needle: &str, case_sensitive: bool) -> bool {
    let hay = match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if case_sensitive {
        hay.contains(needle)
    } else {
        hay.to_lowercase().contains(&needle.to_lowercase())
    }
}

pub enum SearchMsg {
    Hit(usize),
    Done,
}

pub struct SearchHandle {
    pub stop: Arc<AtomicBool>,
    pub rx: Receiver<SearchMsg>,
    handle: Option<JoinHandle<()>>,
}

impl SearchHandle {
    pub fn start(snap: DocSnapshot, query: Query) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel();
        let stop2 = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            for i in 0..snap.line_count() {
                if stop2.load(Ordering::Relaxed) {
                    break;
                }
                if query.line_matches(snap.raw_line(i)) && tx.send(SearchMsg::Hit(i)).is_err() {
                    return;
                }
            }
            let _ = tx.send(SearchMsg::Done);
        });
        Self {
            stop,
            rx,
            handle: Some(handle),
        }
    }

    pub fn cancel(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for SearchHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::JsonlDocument;
    use std::path::PathBuf;

    fn sample_snapshot() -> DocSnapshot {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("docs")
            .join("wire.jsonl");
        JsonlDocument::open(path).unwrap().snapshot()
    }

    #[test]
    fn substring_search_finds_tool_call_lines() {
        let snap = sample_snapshot();
        let q = Query::parse("tool.call", false).unwrap();
        // "tool.call" 里没有冒号分隔的合法 kv（有 ": "…），这里含点号键名 "type:tool.call" 才走 kv
        assert!(q.key_value.is_none() || q.raw == "tool.call");
        let mut h = SearchHandle::start(snap, q);
        let mut hits = Vec::new();
        let mut done = false;
        while let Ok(msg) = h.rx.recv() {
            match msg {
                SearchMsg::Hit(i) => hits.push(i),
                SearchMsg::Done => {
                    done = true;
                    break;
                }
            }
        }
        assert!(done);
        assert!(hits.len() > 10, "tool.call 应命中多行，实际 {}", hits.len());
        h.cancel();
    }

    #[test]
    fn key_value_search() {
        let snap = sample_snapshot();
        let q = Query::parse("type:usage.record", false).unwrap();
        assert_eq!(
            q.key_value,
            Some(("type".to_string(), "usage.record".to_string()))
        );
        let h = SearchHandle::start(snap, q);
        let mut hits = Vec::new();
        while let Ok(msg) = h.rx.recv() {
            match msg {
                SearchMsg::Hit(i) => hits.push(i),
                SearchMsg::Done => break,
            }
        }
        assert_eq!(hits.len(), 14, "usage.record 应恰好 14 行");
    }

    #[test]
    fn case_insensitive_by_default() {
        let q = Query::parse("LLM.REQUEST", false).unwrap();
        assert!(q.line_matches(r#"{"type":"llm.request"}"#));
        let q2 = Query::parse("LLM.REQUEST", true).unwrap();
        assert!(!q2.line_matches(r#"{"type":"llm.request"}"#));
    }

    #[test]
    fn empty_query_is_none() {
        assert!(Query::parse("   ", false).is_none());
    }

    #[test]
    fn cancellation() {
        let snap = sample_snapshot();
        let q = Query::parse("e", false).unwrap();
        let mut h = SearchHandle::start(snap, q);
        h.cancel(); // 不应挂起
    }
}
