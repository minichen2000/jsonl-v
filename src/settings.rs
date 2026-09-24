//! 设置与最近文件持久化：%APPDATA%/jsonl-v/config.json。

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub font_size: f32,
    pub dark: bool,
    pub lang: String, // "zh" | "en"
    pub recent_files: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font_size: 15.0,
            dark: false,
            lang: "zh".to_string(),
            recent_files: Vec::new(),
        }
    }
}

pub fn config_path() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("jsonl-v").join("config.json")
}

impl Settings {
    pub fn load() -> Self {
        Self::load_from(&config_path())
    }

    fn load_from(path: &PathBuf) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Self::default(); // 损坏静默回退默认
        };
        let mut s = Self::default();
        if let Some(f) = v.get("font_size").and_then(|x| x.as_f64()) {
            s.font_size = (f as f32).clamp(10.0, 24.0);
        }
        if let Some(d) = v.get("dark").and_then(|x| x.as_bool()) {
            s.dark = d;
        }
        if let Some(l) = v.get("lang").and_then(|x| x.as_str()) {
            s.lang = l.to_string();
        }
        if let Some(arr) = v.get("recent_files").and_then(|x| x.as_array()) {
            s.recent_files = arr
                .iter()
                .filter_map(|x| x.as_str().map(PathBuf::from))
                .take(10)
                .collect();
        }
        s
    }

    pub fn save(&self) {
        self.save_to(&config_path());
    }

    fn save_to(&self, path: &PathBuf) {
        let v = serde_json::json!({
            "font_size": self.font_size,
            "dark": self.dark,
            "lang": self.lang,
            "recent_files": self.recent_files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        });
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, serde_json::to_string_pretty(&v).unwrap_or_default());
    }

    /// 打开文件后调用：规范化路径（消除相对/绝对、\\?\ 前缀差异），
    /// 大小写不敏感去重后置顶，截断到 10 条。
    pub fn push_recent(&mut self, path: PathBuf) {
        let path = normalize_path(path);
        let key = path.to_string_lossy().to_lowercase();
        self.recent_files
            .retain(|p| p.to_string_lossy().to_lowercase() != key);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(10);
    }
}

/// 规范化为绝对路径并去掉 Windows verbatim 前缀，避免同一路径多种写法。
fn normalize_path(p: PathBuf) -> PathBuf {
    let c = std::fs::canonicalize(&p).unwrap_or(p);
    let s = c.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(stripped) => PathBuf::from(stripped),
        None => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let path = std::env::temp_dir().join("jsonl_v_test_config.json");
        let mut s = Settings::default();
        s.font_size = 18.0;
        s.dark = false;
        s.push_recent(PathBuf::from("C:/a.jsonl"));
        s.push_recent(PathBuf::from("C:/b.jsonl"));
        s.save_to(&path);
        let loaded = Settings::load_from(&path);
        assert_eq!(loaded, s);
    }

    #[test]
    fn missing_or_corrupt_falls_back_to_default() {
        let missing = std::env::temp_dir().join("jsonl_v_test_config_missing.json");
        assert_eq!(Settings::load_from(&missing), Settings::default());
        let bad = std::env::temp_dir().join("jsonl_v_test_config_bad.json");
        std::fs::write(&bad, b"{not json").unwrap();
        assert_eq!(Settings::load_from(&bad), Settings::default());
    }

    #[test]
    fn recent_dedup_and_truncate() {
        let mut s = Settings::default();
        for i in 0..15 {
            s.push_recent(PathBuf::from(format!("C:/f{i}.jsonl")));
        }
        assert_eq!(s.recent_files.len(), 10);
        assert_eq!(s.recent_files[0], PathBuf::from("C:/f14.jsonl"));
        // 重复打开已存在的文件会置顶且不重复
        s.push_recent(PathBuf::from("C:/f10.jsonl"));
        assert_eq!(s.recent_files.len(), 10);
        assert_eq!(s.recent_files[0], PathBuf::from("C:/f10.jsonl"));
        assert_eq!(
            s.recent_files.iter().filter(|p| **p == PathBuf::from("C:/f10.jsonl")).count(),
            1
        );
        // 大小写不同的同一路径也应去重（Windows 文件系统不区分大小写）
        s.push_recent(PathBuf::from("C:/F10.JSONL"));
        assert_eq!(
            s.recent_files
                .iter()
                .filter(|p| p.to_string_lossy().to_lowercase().contains("f10"))
                .count(),
            1
        );
    }

    #[test]
    fn recent_dedup_relative_vs_absolute() {
        // 相对路径与绝对路径指向同一文件时不应重复
        let dir = std::env::temp_dir().join("jsonl_v_test_norm");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("x.jsonl");
        std::fs::write(&file, b"{}\n").unwrap();
        let mut s = Settings::default();
        s.push_recent(file.clone());
        let abs = std::fs::canonicalize(&file).unwrap();
        s.push_recent(abs);
        assert_eq!(s.recent_files.len(), 1);
        // 规范化后不含 \\?\ 前缀
        assert!(!s.recent_files[0].to_string_lossy().starts_with(r"\\?\"));
    }
}
