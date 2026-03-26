pub mod mupdf_backend;

use anyhow::Result;
use std::path::Path;

/// Rendered page - backend independent RGBA pixel buffer
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA row-major
    pub page_no: usize,
    pub scale: f32,
}

/// TOC / Outline entry
#[derive(Debug, Clone)]
pub struct OutlineEntry {
    pub title: String,
    pub page_no: Option<usize>,
    pub children: Vec<OutlineEntry>,
}

/// Search hit with quad coordinates
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub page_no: usize,
    pub quads: Vec<[f32; 8]>,
}

/// Document metadata
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub page_count: usize,
}

/// A positioned character on a page
#[derive(Debug, Clone)]
pub struct PageChar {
    pub c: char,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// A link on a PDF page
#[derive(Debug, Clone)]
pub struct PageLink {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub page_target: Option<usize>,
    pub uri: Option<String>,
}

/// PDF backend abstraction
pub trait PdfBackend: Send {
    type Doc: PdfDocument;
    fn open_file(&self, path: &Path) -> Result<Self::Doc>;
}

/// PDF document abstraction
#[allow(dead_code)]
pub trait PdfDocument: Send {
    fn page_count(&self) -> usize;
    fn page_size(&self, page_no: usize) -> Result<(f32, f32)>;
    fn render_page(&self, page_no: usize, scale: f32) -> Result<RenderedPage>;
    fn render_page_rotated(&self, page_no: usize, scale: f32, rotation: i32) -> Result<RenderedPage>;
    fn outline(&self) -> Result<Vec<OutlineEntry>>;
    fn search_page(&self, page_no: usize, query: &str) -> Result<Vec<SearchHit>>;
    fn extract_text(&self, page_no: usize) -> Result<String>;
    /// Get all characters with positions on a page
    fn page_chars(&self, page_no: usize) -> Result<Vec<PageChar>>;
    fn metadata(&self) -> Result<DocumentMetadata>;
    /// Get all links on a page
    fn page_links(&self, page_no: usize) -> Result<Vec<PageLink>>;
    fn needs_password(&self) -> bool;
    fn authenticate(&mut self, password: &str) -> Result<bool>;
    /// Get the file path for properties
    fn file_size_bytes(&self) -> Option<u64> { None }
    fn pdf_version(&self) -> Option<String> { None }
}
