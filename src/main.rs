mod app;
mod ui;
mod ssh;
mod config;
mod plugins;
mod utils;

use eframe::egui;

fn setup_custom_fonts(ctx: &egui::Context) {
    // 启用默认的中文字体支持
    let fonts = egui::FontDefinitions::default();
    
    // egui 内置支持中文，确保加载了正确的字符集
    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("AY Dev Tool - Terminal & DevOps"),
        ..Default::default()
    };

    eframe::run_native(
        "AY Dev Tool",
        options,
        Box::new(|cc| {
            // 设置字体以支持中文
            setup_custom_fonts(&cc.egui_ctx);
            
            // 启用深色主题
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            
            // 创建应用实例
            Ok(Box::new(app::TerminalApp::new(cc)))
        }),
    )
}
