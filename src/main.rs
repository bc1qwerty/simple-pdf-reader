// Hide console window on Windows release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod render;
mod state;
mod ui;

use app::PdfReaderApp;
use state::settings::AppSettings;

fn main() -> eframe::Result<()> {
    // Handle --register / --unregister before GUI
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--register" => {
                register_file_association();
                return Ok(());
            }
            "--unregister" => {
                unregister_file_association();
                return Ok(());
            }
            _ => {} // normal file path, continue to GUI
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("pdf_reader=info".parse().unwrap())
                .add_directive("wgpu=error".parse().unwrap()),
        )
        .init();

    let settings = AppSettings::load();
    let (w, h) = (settings.window_width, settings.window_height);

    // Load window icon from embedded PNG
    let icon = load_icon();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([w, h])
        .with_min_inner_size([600.0, 400.0])
        .with_drag_and_drop(true);

    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(icon_data);
    }

    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Simple PDF Reader",
        options,
        Box::new(|cc| Ok(Box::new(PdfReaderApp::new(cc)))),
    )
}

fn load_icon() -> Option<egui::IconData> {
    let png_bytes = include_bytes!("../assets/icon.png");
    let image = image::load_from_memory(png_bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

/// Register as default PDF viewer (Windows: HKCU registry, Linux: xdg-mime)
fn register_file_association() {
    let exe_path = std::env::current_exe().unwrap_or_default();
    let exe_str = exe_path.to_string_lossy();

    #[cfg(target_os = "windows")]
    {
        // Write to HKCU (no admin needed)
        let commands = [
            format!(r#"reg add "HKCU\Software\Classes\.pdf" /ve /d "SimplePDFReader.Document" /f"#),
            format!(r#"reg add "HKCU\Software\Classes\SimplePDFReader.Document" /ve /d "PDF Document" /f"#),
            format!(r#"reg add "HKCU\Software\Classes\SimplePDFReader.Document\shell\open\command" /ve /d "\"{exe_str}\" \"%1\"" /f"#),
            format!(r#"reg add "HKCU\Software\Classes\SimplePDFReader.Document\DefaultIcon" /ve /d "{exe_str},0" /f"#),
        ];
        for cmd in &commands {
            let _ = std::process::Command::new("cmd")
                .args(["/C", cmd])
                .output();
        }
        println!("Registered as default PDF viewer.");
        println!("Path: {}", exe_str);
    }

    #[cfg(target_os = "linux")]
    {
        // Create .desktop file
        let desktop_dir = std::env::var("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".local/share/applications"))
            .unwrap_or_else(|_| std::path::PathBuf::from("/usr/share/applications"));
        let _ = std::fs::create_dir_all(&desktop_dir);
        let desktop_file = desktop_dir.join("simple-pdf-reader.desktop");
        let content = format!(
            "[Desktop Entry]\nName=Simple PDF Reader\nExec={} %f\nType=Application\nMimeType=application/pdf;\nIcon=application-pdf\nTerminal=false\n",
            exe_str
        );
        if std::fs::write(&desktop_file, content).is_ok() {
            let _ = std::process::Command::new("xdg-mime")
                .args(["default", "simple-pdf-reader.desktop", "application/pdf"])
                .output();
            println!("Registered as default PDF viewer.");
            println!("Desktop file: {}", desktop_file.display());
        }
    }
}

/// Unregister file association
fn unregister_file_association() {
    #[cfg(target_os = "windows")]
    {
        let commands = [
            r#"reg delete "HKCU\Software\Classes\.pdf" /f"#,
            r#"reg delete "HKCU\Software\Classes\SimplePDFReader.Document" /f"#,
        ];
        for cmd in commands {
            let _ = std::process::Command::new("cmd")
                .args(["/C", cmd])
                .output();
        }
        println!("Unregistered PDF file association.");
    }

    #[cfg(target_os = "linux")]
    {
        let desktop_dir = std::env::var("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".local/share/applications"))
            .unwrap_or_else(|_| std::path::PathBuf::from("/usr/share/applications"));
        let desktop_file = desktop_dir.join("simple-pdf-reader.desktop");
        let _ = std::fs::remove_file(&desktop_file);
        println!("Unregistered PDF file association.");
    }
}

/// Public function for in-app registration (called from menu)
pub fn register_as_default() {
    register_file_association();
}

pub fn unregister_as_default() {
    unregister_file_association();
}
