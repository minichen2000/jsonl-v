use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

const CACHE_CAP: usize = 64;

/// 只读快照，可安全发送到后台线程做全文搜索。
#[derive(Clone)]
pub struct DocSnapshot {
    buf: Arc<Vec<u8>>,
    line_offsets: Arc<Vec<usize>>,
}

impl DocSnapshot {
    pub fn line_count(&self) -> usize {
        self.line_offsets.len().saturating_sub(1)
    }

    pub fn raw_line(&self, i: usize) -> &str {
        raw_line_at(&self.buf, &self.line_offsets, i)
    }
}

pub struct JsonlDocument {
    pub path: PathBuf,
    buf: Arc<Vec<u8>>,
    line_offsets: Arc<Vec<usize>>,
    cache: LruCache<usize, LineInfo>,
    mtime: Option<std::time::SystemTime>,
}

pub struct LineInfo {
    pub parsed: Result<serde_json::Value, String>,
    pub summary: String,
    pub byte_len: usize,
}

impl JsonlDocument {
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let buf = std::fs::read(&path)?;
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let line_offsets = split_lines(&buf);
        Ok(Self {
            path,
            buf: Arc::new(buf),
            line_offsets: Arc::new(line_offsets),
            cache: LruCache::new(CACHE_CAP),
            mtime,
        })
    }

    pub fn snapshot(&self) -> DocSnapshot {
        DocSnapshot {
            buf: Arc::clone(&self.buf),
            line_offsets: Arc::clone(&self.line_offsets),
        }
    }

    pub fn line_count(&self) -> usize {
        self.line_offsets.len().saturating_sub(1)
    }

    pub fn raw_line(&self, i: usize) -> &str {
        raw_line_at(&self.buf, &self.line_offsets, i)
    }

    pub fn line_info(&mut self, i: usize) -> &LineInfo {
        if !self.cache.contains(&i) {
            let raw = self.raw_line(i);
            let byte_len = raw.len();
            let parsed = serde_json::from_str::<serde_json::Value>(raw).map_err(|e| e.to_string());
            let summary = make_summary(&parsed, raw);
            self.cache.insert(
                i,
                LineInfo {
                    parsed,
                    summary,
                    byte_len,
                },
            );
        }
        self.cache.get(&i).unwrap()
    }

    /// Peek without touching the LRU cache (used by background scans that
    /// would otherwise thrash the cache).
    pub fn peek_info(&self, i: usize) -> LineInfo {
        let raw = self.raw_line(i);
        let byte_len = raw.len();
        let parsed = serde_json::from_str::<serde_json::Value>(raw).map_err(|e| e.to_string());
        let summary = make_summary(&parsed, raw);
        LineInfo {
            parsed,
            summary,
            byte_len,
        }
    }

    /// Reload if the file changed on disk (mtime or size). Returns true if reloaded.
    pub fn reload(&mut self) -> io::Result<bool> {
        let meta = std::fs::metadata(&self.path)?;
        let mtime = meta.modified().ok();
        let size = meta.len();
        if mtime == self.mtime && size == self.buf.len() as u64 {
            return Ok(false);
        }
        self.buf = Arc::new(std::fs::read(&self.path)?);
        self.mtime = mtime;
        self.line_offsets = Arc::new(split_lines(&self.buf));
        self.cache.clear();
        Ok(true)
    }

    pub fn total_bytes(&self) -> usize {
        self.buf.len()
    }

    pub fn raw_bytes(&self) -> &[u8] {
        &self.buf
    }
}

fn raw_line_at<'a>(buf: &'a [u8], offsets: &[usize], i: usize) -> &'a str {
    let start = offsets[i];
    let end = offsets[i + 1];
    let bytes = &buf[start..end];
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    std::str::from_utf8(bytes).unwrap_or("<invalid utf-8>")
}

fn split_lines(buf: &[u8]) -> Vec<usize> {
    if buf.is_empty() {
        return vec![0];
    }
    let mut offsets = Vec::with_capacity(1024);
    offsets.push(0);
    for pos in memchr::memchr_iter(b'\n', buf) {
        let next = pos + 1;
        if next < buf.len() {
            offsets.push(next);
        }
    }
    offsets.push(buf.len());
    offsets
}

fn make_summary(parsed: &Result<serde_json::Value, String>, raw: &str) -> String {
    match parsed {
        Ok(serde_json::Value::Object(map)) => {
            if let Some(serde_json::Value::String(t)) = map.get("type") {
                return t.clone();
            }
            if let Some(serde_json::Value::String(r)) = map.get("role") {
                return format!("(role) {r}");
            }
            truncate(raw, 40)
        }
        Ok(_) => truncate(raw, 40),
        Err(_) => {
            if raw.trim().is_empty() {
                "<empty>".to_string()
            } else {
                "<bad json>".to_string()
            }
        }
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_chars).collect::<String>())
    }
}

struct LruCache<K: std::hash::Hash + Eq + Clone, V> {
    cap: usize,
    map: HashMap<K, V>,
    order: VecDeque<K>, // front = most recently used
}

impl<K: std::hash::Hash + Eq + Clone, V> LruCache<K, V> {
    fn new(cap: usize) -> Self {
        Self {
            cap,
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn contains(&self, k: &K) -> bool {
        self.map.contains_key(k)
    }

    fn get(&mut self, k: &K) -> Option<&V> {
        if !self.map.contains_key(k) {
            return None;
        }
        self.touch(k);
        self.map.get(k)
    }

    fn insert(&mut self, k: K, v: V) {
        if self.map.contains_key(&k) {
            self.map.insert(k.clone(), v);
            self.touch(&k);
            return;
        }
        while self.map.len() >= self.cap {
            if let Some(old) = self.order.pop_back() {
                self.map.remove(&old);
            } else {
                break;
            }
        }
        self.map.insert(k.clone(), v);
        self.order.push_front(k);
    }

    fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    fn touch(&mut self, k: &K) {
        if let Some(pos) = self.order.iter().position(|x| x == k) {
            self.order.remove(pos);
        }
        self.order.push_front(k.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(name: &str, content: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("jsonl_v_test_{name}"));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }

    #[test]
    fn empty_file() {
        let path = write_temp("empty.jsonl", b"");
        let doc = JsonlDocument::open(path).unwrap();
        assert_eq!(doc.line_count(), 0);
    }

    #[test]
    fn single_line_no_trailing_newline() {
        let path = write_temp("single.jsonl", br#"{"a":1}"#);
        let mut doc = JsonlDocument::open(path).unwrap();
        assert_eq!(doc.line_count(), 1);
        assert_eq!(doc.raw_line(0), r#"{"a":1}"#);
        assert!(doc.line_info(0).parsed.is_ok());
    }

    #[test]
    fn trailing_newline_does_not_create_phantom_line() {
        let path = write_temp("trailing.jsonl", b"{\"a\":1}\n{\"a\":2}\n");
        let doc = JsonlDocument::open(path).unwrap();
        assert_eq!(doc.line_count(), 2);
    }

    #[test]
    fn empty_lines() {
        let path = write_temp("emptylines.jsonl", b"{\"a\":1}\n\n{\"a\":2}\n");
        let mut doc = JsonlDocument::open(path).unwrap();
        assert_eq!(doc.line_count(), 3);
        assert_eq!(doc.raw_line(1), "");
        let info = doc.line_info(1);
        assert!(info.parsed.is_err());
        assert_eq!(info.summary, "<empty>");
    }

    #[test]
    fn bad_json_line() {
        let path = write_temp("bad.jsonl", b"{not json}\n{\"ok\":true}\n");
        let mut doc = JsonlDocument::open(path).unwrap();
        assert_eq!(doc.line_count(), 2);
        assert!(doc.line_info(0).parsed.is_err());
        assert_eq!(doc.line_info(0).summary, "<bad json>");
        assert!(doc.line_info(1).parsed.is_ok());
    }

    #[test]
    fn unicode_and_escapes() {
        let line = r#"{"msg":"中文 emoji 🎉 \n换行\t制表","type":"test"}"#;
        let path = write_temp("unicode.jsonl", format!("{line}\n").as_bytes());
        let mut doc = JsonlDocument::open(path).unwrap();
        let info = doc.line_info(0);
        assert_eq!(info.summary, "test");
        let v = info.parsed.as_ref().unwrap();
        let msg = v["msg"].as_str().unwrap();
        assert!(msg.contains("中文"));
        assert!(msg.contains("🎉"));
        assert!(msg.contains('\n'));
        assert!(msg.contains('\t'));
    }

    #[test]
    fn very_long_line() {
        let big = "x".repeat(100_000);
        let line = format!(r#"{{"type":"big","data":"{big}"}}"#);
        let path = write_temp("longline.jsonl", line.as_bytes());
        let mut doc = JsonlDocument::open(path).unwrap();
        assert_eq!(doc.line_count(), 1);
        let info = doc.line_info(0);
        assert_eq!(info.summary, "big");
        assert!(info.byte_len > 100_000);
    }

    #[test]
    fn many_lines_index_correctness() {
        let mut content = String::new();
        for i in 0..10_000 {
            content.push_str(&format!(r#"{{"type":"line","n":{i}}}"#));
            content.push('\n');
        }
        let path = write_temp("many.jsonl", content.as_bytes());
        let mut doc = JsonlDocument::open(path).unwrap();
        assert_eq!(doc.line_count(), 10_000);
        for i in [0, 1, 4999, 9999] {
            let info = doc.line_info(i);
            let v = info.parsed.as_ref().unwrap();
            assert_eq!(v["n"].as_u64().unwrap(), i as u64);
        }
    }

    #[test]
    fn summary_priority() {
        let path = write_temp(
            "summary.jsonl",
            concat!(
                r#"{"type":"t","role":"user"}"#,
                "\n",
                r#"{"role":"assistant","content":"hi"}"#,
                "\n",
                r#"{"other":123}"#,
                "\n"
            )
            .as_bytes(),
        );
        let mut doc = JsonlDocument::open(path).unwrap();
        assert_eq!(doc.line_info(0).summary, "t");
        assert_eq!(doc.line_info(1).summary, "(role) assistant");
        assert!(doc.line_info(2).summary.starts_with(r#"{"other":123}"#));
    }

    #[test]
    fn crlf_handling() {
        let path = write_temp("crlf.jsonl", b"{\"a\":1}\r\n{\"a\":2}\r\n");
        let mut doc = JsonlDocument::open(path).unwrap();
        assert_eq!(doc.line_count(), 2);
        assert!(doc.line_info(0).parsed.is_ok());
    }

    #[test]
    fn lru_eviction() {
        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!(r#"{{"n":{i}}}"#));
            content.push('\n');
        }
        let path = write_temp("lru.jsonl", content.as_bytes());
        let mut doc = JsonlDocument::open(path).unwrap();
        for i in 0..100 {
            doc.line_info(i);
        }
        // capacity 64: earliest entries evicted, latest retained
        assert!(!doc.cache.contains(&0));
        assert!(doc.cache.contains(&99));
        assert_eq!(doc.cache.map.len(), 64);
    }

    #[test]
    fn reload_detects_append() {
        let path = write_temp("reload.jsonl", b"{\"a\":1}\n");
        let mut doc = JsonlDocument::open(path.clone()).unwrap();
        assert_eq!(doc.line_count(), 1);
        // size change alone triggers reload even if mtime resolution is coarse
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{\"a\":2}\n")
            .unwrap();
        assert!(doc.reload().unwrap());
        assert_eq!(doc.line_count(), 2);
        // no further change
        assert!(!doc.reload().unwrap());
    }

    #[test]
    #[ignore = "perf: run explicitly with --ignored"]
    fn open_100mb_under_1s() {
        let path = std::env::temp_dir().join("jsonl_v_test_100mb.jsonl");
        {
            let mut f = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
            let filler = "x".repeat(950);
            let mut written = 0usize;
            let mut i = 0u64;
            while written < 100 * 1024 * 1024 {
                let line = format!(r#"{{"type":"perf","n":{i},"data":"{filler}"}}"#,);
                writeln!(f, "{line}").unwrap();
                written += line.len() + 1;
                i += 1;
            }
        }
        let start = std::time::Instant::now();
        let doc = JsonlDocument::open(path).unwrap();
        let elapsed = start.elapsed();
        assert!(doc.line_count() > 100_000);
        assert!(
            elapsed.as_secs_f64() < 1.0,
            "open 100MB took {:.2}s, want <1s",
            elapsed.as_secs_f64()
        );
    }
}
