fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Simple PDF Reader");
        res.set("FileDescription", "Simple Portable PDF Reader");
        res.set("CompanyName", "");
        res.set("LegalCopyright", "AGPL-3.0");
        res.compile().expect("Failed to compile Windows resources");
    }
}
