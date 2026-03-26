use anyhow::{Context, Result};
use std::path::Path;

use super::{DocumentMetadata, OutlineEntry, PageChar, PageLink, PdfBackend, PdfDocument, RenderedPage, SearchHit};

pub struct MuPdfBackend;

impl PdfBackend for MuPdfBackend {
    type Doc = MuPdfDocument;

    fn open_file(&self, path: &Path) -> Result<Self::Doc> {
        let doc = mupdf::Document::open(path.to_str().unwrap_or_default())
            .with_context(|| format!("Failed to open: {}", path.display()))?;
        let page_count = doc.page_count().with_context(|| "Failed to get page count")? as usize;
        let file_size = std::fs::metadata(path).ok().map(|m| m.len());
        Ok(MuPdfDocument { doc, page_count, file_size })
    }
}

pub struct MuPdfDocument {
    doc: mupdf::Document,
    page_count: usize,
    file_size: Option<u64>,
}

unsafe impl Send for MuPdfDocument {}

impl PdfDocument for MuPdfDocument {
    fn page_count(&self) -> usize {
        self.page_count
    }

    fn page_size(&self, page_no: usize) -> Result<(f32, f32)> {
        let page = self.doc.load_page(page_no as i32)
            .with_context(|| format!("Failed to load page {page_no}"))?;
        let bounds = page.bounds()?;
        Ok((bounds.x1 - bounds.x0, bounds.y1 - bounds.y0))
    }

    fn render_page(&self, page_no: usize, scale: f32) -> Result<RenderedPage> {
        self.render_page_rotated(page_no, scale, 0)
    }

    fn render_page_rotated(&self, page_no: usize, scale: f32, rotation: i32) -> Result<RenderedPage> {
        let page = self.doc.load_page(page_no as i32)
            .with_context(|| format!("Failed to load page {page_no}"))?;

        // Cap render resolution: max 4000px on longest side to avoid huge pixmaps
        let bounds = page.bounds()?;
        let pw = bounds.x1 - bounds.x0;
        let ph = bounds.y1 - bounds.y0;

        // After rotation, the effective dimensions change
        let (eff_w, eff_h) = if rotation == 90 || rotation == 270 {
            (ph, pw)
        } else {
            (pw, ph)
        };

        let max_dim = (eff_w * scale).max(eff_h * scale);
        let effective_scale = if max_dim > 4000.0 {
            scale * (4000.0 / max_dim)
        } else {
            scale
        };

        // Build transformation matrix: scale then rotate
        // concat is &mut self, so we need to chain carefully.
        let mut matrix = mupdf::Matrix::new_scale(effective_scale, effective_scale);
        matrix.concat(mupdf::Matrix::new_rotate(rotation as f32));

        // After rotation, the origin may shift negative. We need to translate
        // so the top-left corner is at (0,0).
        let corners = [
            (bounds.x0, bounds.y0),
            (bounds.x1, bounds.y0),
            (bounds.x0, bounds.y1),
            (bounds.x1, bounds.y1),
        ];
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        for (x, y) in corners {
            let tx = matrix.a * x + matrix.c * y + matrix.e;
            let ty = matrix.b * x + matrix.d * y + matrix.f;
            min_x = min_x.min(tx);
            min_y = min_y.min(ty);
        }
        // Translate so min corner is at origin
        matrix.concat(mupdf::Matrix::new_translate(-min_x, -min_y));

        let pixmap = page
            .to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), false, true)
            .with_context(|| format!("Failed to render page {page_no}"))?;

        let width = pixmap.width() as u32;
        let height = pixmap.height() as u32;
        let rgb = pixmap.samples();

        // Convert RGB (3ch) to RGBA (4ch) for egui
        let pixel_count = (width * height) as usize;
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        for i in 0..pixel_count {
            rgba.push(rgb[i * 3]);
            rgba.push(rgb[i * 3 + 1]);
            rgba.push(rgb[i * 3 + 2]);
            rgba.push(255);
        }

        Ok(RenderedPage { width, height, pixels: rgba, page_no, scale })
    }

    fn outline(&self) -> Result<Vec<OutlineEntry>> {
        match self.doc.outlines() {
            Ok(o) => Ok(convert_outline(&o)),
            Err(_) => Ok(vec![]),
        }
    }

    fn search_page(&self, page_no: usize, query: &str) -> Result<Vec<SearchHit>> {
        let page = self.doc.load_page(page_no as i32)?;
        // Use TextPage for more reliable search
        let text_page = page.to_text_page(mupdf::text_page::TextPageFlags::empty())?;
        let quads = text_page.search(query)?;

        if quads.is_empty() {
            return Ok(vec![]);
        }

        let quad_arrays: Vec<[f32; 8]> = quads.iter().map(|q| {
            [q.ul.x, q.ul.y, q.ur.x, q.ur.y, q.ll.x, q.ll.y, q.lr.x, q.lr.y]
        }).collect();

        Ok(vec![SearchHit { page_no, quads: quad_arrays }])
    }

    fn extract_text(&self, page_no: usize) -> Result<String> {
        let page = self.doc.load_page(page_no as i32)?;
        let text_page = page.to_text_page(mupdf::text_page::TextPageFlags::empty())?;
        Ok(text_page.to_text()?)
    }

    fn page_chars(&self, page_no: usize) -> Result<Vec<PageChar>> {
        let page = self.doc.load_page(page_no as i32)?;
        let text_page = page.to_text_page(mupdf::text_page::TextPageFlags::empty())?;
        let mut chars = Vec::new();
        for block in text_page.blocks() {
            for line in block.lines() {
                for tc in line.chars() {
                    if let Some(c) = tc.char() {
                        let q = tc.quad();
                        chars.push(PageChar {
                            c,
                            x0: q.ul.x.min(q.ll.x),
                            y0: q.ul.y.min(q.ur.y),
                            x1: q.ur.x.max(q.lr.x),
                            y1: q.ll.y.max(q.lr.y),
                        });
                    }
                }
            }
        }
        Ok(chars)
    }

    fn metadata(&self) -> Result<DocumentMetadata> {
        let title = self.doc.metadata(mupdf::document::MetadataName::Title).ok()
            .filter(|s| !s.is_empty());
        let author = self.doc.metadata(mupdf::document::MetadataName::Author).ok()
            .filter(|s| !s.is_empty());
        Ok(DocumentMetadata { title, author, page_count: self.page_count })
    }

    fn needs_password(&self) -> bool {
        self.doc.needs_password().unwrap_or(false)
    }

    fn authenticate(&mut self, password: &str) -> Result<bool> {
        Ok(self.doc.authenticate(password)?)
    }

    fn page_links(&self, page_no: usize) -> Result<Vec<PageLink>> {
        let page = self.doc.load_page(page_no as i32)?;
        let link_iter = page.links()?;
        let mut links = Vec::new();
        for link in link_iter {
            let bounds = link.bounds;
            let uri_str = link.uri;
            let page_target = link.dest.map(|d| d.loc.page_number as usize);
            let uri = if uri_str.starts_with("http://") || uri_str.starts_with("https://") {
                Some(uri_str)
            } else {
                None
            };
            links.push(PageLink {
                x0: bounds.x0,
                y0: bounds.y0,
                x1: bounds.x1,
                y1: bounds.y1,
                page_target,
                uri,
            });
        }
        Ok(links)
    }

    fn file_size_bytes(&self) -> Option<u64> {
        self.file_size
    }

    fn pdf_version(&self) -> Option<String> {
        // MuPDF doesn't expose version directly via this binding; return None
        None
    }
}

fn convert_outline(items: &[mupdf::Outline]) -> Vec<OutlineEntry> {
    items.iter().map(|item| {
        OutlineEntry {
            title: item.title.clone(),
            page_no: item.dest.map(|d| d.loc.page_number as usize),
            children: convert_outline(&item.down),
        }
    }).collect()
}
