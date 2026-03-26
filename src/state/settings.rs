use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub page_no: usize,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub dark_mode: bool,
    pub invert_pdf: bool,
    #[serde(default)]
    pub bookmarks: HashMap<String, Vec<Bookmark>>,
    #[serde(default)]
    pub recent_files: Vec<PathBuf>,
    /// Remember last page per file (file_key -> page_no)
    #[serde(default)]
    pub last_page: HashMap<String, usize>,
    pub window_width: f32,
    pub window_height: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            dark_mode: true,
            invert_pdf: false,
            bookmarks: HashMap::new(),
            recent_files: Vec::new(),
            last_page: HashMap::new(),
            window_width: 1200.0,
            window_height: 800.0,
        }
    }
}

impl AppSettings {
    /// Config file path - always next to the exe (portable mode)
    fn config_path() -> PathBuf {
        // Always resolve to exe directory, never current working directory
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.join("simple-pdf-reader-config.json");
            }
        }
        // Fallback: user home directory
        #[cfg(target_os = "windows")]
        {
            if let Ok(home) = std::env::var("USERPROFILE") {
                return PathBuf::from(home).join("simple-pdf-reader-config.json");
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            if let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(home).join("simple-pdf-reader-config.json");
            }
        }
        PathBuf::from("simple-pdf-reader-config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    pub fn add_recent_file(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        if self.recent_files.len() > 20 {
            self.recent_files.truncate(20);
        }
    }

    pub fn toggle_bookmark(&mut self, file_key: &str, page_no: usize) {
        let bookmarks = self.bookmarks.entry(file_key.to_string()).or_default();
        if let Some(idx) = bookmarks.iter().position(|b| b.page_no == page_no) {
            bookmarks.remove(idx);
        } else {
            bookmarks.push(Bookmark {
                page_no,
                label: format!("Page {}", page_no + 1),
            });
        }
    }

    pub fn is_bookmarked(&self, file_key: &str, page_no: usize) -> bool {
        self.bookmarks
            .get(file_key)
            .map(|bs| bs.iter().any(|b| b.page_no == page_no))
            .unwrap_or(false)
    }

    pub fn get_bookmarks(&self, file_key: &str) -> Vec<&Bookmark> {
        self.bookmarks
            .get(file_key)
            .map(|bs| bs.iter().collect())
            .unwrap_or_default()
    }

    pub fn save_last_page(&mut self, file_key: &str, page_no: usize) {
        self.last_page.insert(file_key.to_string(), page_no);
    }

    pub fn get_last_page(&self, file_key: &str) -> Option<usize> {
        self.last_page.get(file_key).copied()
    }
}
