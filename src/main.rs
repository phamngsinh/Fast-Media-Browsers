#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod file_ops;
mod thumbnail;

use eframe::egui;

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    // Load Windows system fonts for Vietnamese (Unicode) support
    if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf").or_else(|_| std::fs::read("C:\\Windows\\Fonts\\arial.ttf")) {
        fonts.font_data.insert(
            "my_font".to_owned(),
            egui::FontData::from_owned(font_data),
        );
        
        fonts.families.entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "my_font".to_owned());
            
        fonts.families.entry(egui::FontFamily::Monospace)
            .or_default()
            .push("my_font".to_owned());
    }

    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Fast Media Browser - Rust Edition"),
        ..Default::default()
    };
    
    eframe::run_native(
        "Fast Media Browser",
        options,
        Box::new(|cc| {
            setup_custom_fonts(&cc.egui_ctx);
            Box::<app::MediaBrowserApp>::default()
        }),
    )
}
