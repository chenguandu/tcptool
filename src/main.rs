#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use eframe::egui;
use tcptool::TcpToolApp;

fn main() -> eframe::Result<()> {
    env_logger::init();

    // Create Tokio runtime so that tokio::spawn() works from egui callbacks
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let _enter = runtime.enter();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("TCP 调试工具 - JT808")
            .with_icon(load_embedded_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "TCP Debug Tool",
        options,
        Box::new(|cc| {
            // Configure Chinese fonts
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(TcpToolApp::default()))
        }),
    )
}

/// Configure egui to use Chinese fonts on Windows
fn setup_fonts(ctx: &egui::Context) {
    use egui::FontDefinitions;

    let mut fonts = FontDefinitions::default();

    // Try to load Microsoft YaHei (common on Chinese Windows)
    let font_paths = [
        "C:\\Windows\\Fonts\\msyh.ttc",   // Microsoft YaHei Regular
        "C:\\Windows\\Fonts\\msyhbd.ttc", // Microsoft YaHei Bold
        "C:\\Windows\\Fonts\\simsun.ttc", // SimSun (fallback)
    ];

    let mut loaded = false;
    for path in &font_paths {
        if let Some(data) = load_font_data(path) {
            fonts
                .font_data
                .insert("chinese".to_string(), egui::FontData::from_owned(data).into());
            loaded = true;
            break;
        }
    }

    if loaded {
        // Replace the proportional and monospace font families
        if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            proportional.insert(0, "chinese".to_string());
        }
        if let Some(monospace) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            monospace.insert(0, "chinese".to_string());
        }
    } else {
        // Fallback: try any .ttf/.ttc in Windows Fonts directory that might have CJK
        if let Ok(entries) = std::fs::read_dir("C:\\Windows\\Fonts") {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                // Try common CJK fonts
                let lower = name.to_lowercase();
                if lower.contains("msyh")
                    || lower.contains("simsun")
                    || lower.contains("simhei")
                    || lower.contains("deng")
                    || lower.contains("yahei")
                    || lower.contains("source")
                {
                    if let Some(data) = load_font_data(&path.to_string_lossy()) {
                        fonts
                            .font_data
                            .insert("chinese".to_string(), egui::FontData::from_owned(data).into());
                        if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                            proportional.insert(0, "chinese".to_string());
                        }
                        break;
                    }
                }
            }
        }
    }

    ctx.set_fonts(fonts);
}

fn load_font_data(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// Load icon from embedded bytes (always available, no external file needed)
fn load_embedded_icon() -> egui::IconData {
    // logo.png is embedded at compile time via include_bytes!
    let png_bytes = include_bytes!("../logo.png");
    match image::load_from_memory(png_bytes) {
        Ok(img) => {
            let img = img.into_rgba8();
            let (w, h) = img.dimensions();
            egui::IconData {
                rgba: img.into_raw(),
                width: w,
                height: h,
            }
        }
        Err(e) => {
            log::warn!("Failed to decode embedded icon: {}", e);
            egui::IconData::default()
        }
    }
}
