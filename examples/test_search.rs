fn main() {
    let doc = mupdf::Document::open("/home/seo/pdf-reader/test.pdf").unwrap();

    // Test page 1 (more content)
    let page = doc.load_page(1).unwrap();
    let tp = page.to_text_page(mupdf::text_page::TextPageFlags::empty()).unwrap();
    let text = tp.to_text().unwrap();
    let preview_len = 500.min(text.len());
    println!("Page 1 text:\n{}\n---", &text[..preview_len]);

    // Search Korean
    for query in &["연구", "조사", "근로"] {
        let hits = tp.search(query).unwrap();
        println!("Search '{}': {} hits", query, hits.len());
        for (i, q) in hits.iter().enumerate().take(3) {
            println!("  hit {}: ul=({:.1},{:.1}) lr=({:.1},{:.1})", i, q.ul.x, q.ul.y, q.lr.x, q.lr.y);
        }
    }

    // Also test page 0
    let page0 = doc.load_page(0).unwrap();
    let tp0 = page0.to_text_page(mupdf::text_page::TextPageFlags::empty()).unwrap();
    for query in &["연구", "코드북"] {
        let hits = tp0.search(query).unwrap();
        println!("Page 0 search '{}': {} hits", query, hits.len());
    }
}
