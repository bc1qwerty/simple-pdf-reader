use egui::TextureHandle;
use std::collections::{HashMap, VecDeque};

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
struct CacheKey {
    page_no: usize,
    scale_percent: u32,
    rotation: i32,
}

impl CacheKey {
    fn new(page_no: usize, scale: f32, rotation: i32) -> Self {
        Self {
            page_no,
            scale_percent: (scale * 100.0) as u32,
            rotation,
        }
    }
}

/// LRU texture cache - keeps rendered pages in GPU memory
pub struct PageCache {
    cache: HashMap<CacheKey, TextureHandle>,
    lru_order: VecDeque<CacheKey>,
    max_entries: usize,
}

impl PageCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: HashMap::new(),
            lru_order: VecDeque::new(),
            max_entries,
        }
    }

    pub fn get(&mut self, page_no: usize, scale: f32, rotation: i32) -> Option<&TextureHandle> {
        let key = CacheKey::new(page_no, scale, rotation);
        if self.cache.contains_key(&key) {
            // Move to front (most recently used)
            self.lru_order.retain(|k| k != &key);
            self.lru_order.push_front(key.clone());
            self.cache.get(&key)
        } else {
            None
        }
    }

    pub fn insert(&mut self, page_no: usize, scale: f32, rotation: i32, texture: TextureHandle) {
        let key = CacheKey::new(page_no, scale, rotation);

        // Remove if already exists
        if self.cache.contains_key(&key) {
            self.lru_order.retain(|k| k != &key);
        }

        // Evict oldest if full
        while self.cache.len() >= self.max_entries {
            if let Some(old_key) = self.lru_order.pop_back() {
                self.cache.remove(&old_key);
            }
        }

        self.lru_order.push_front(key.clone());
        self.cache.insert(key, texture);
    }

    pub fn contains(&self, page_no: usize, scale: f32, rotation: i32) -> bool {
        self.cache.contains_key(&CacheKey::new(page_no, scale, rotation))
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.lru_order.clear();
    }

    /// Get any cached texture for this page (any scale) for fallback display
    pub fn get_any_scale(&self, page_no: usize) -> Option<&TextureHandle> {
        self.cache
            .iter()
            .find(|(k, _)| k.page_no == page_no)
            .map(|(_, v)| v)
    }
}
