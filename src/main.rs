mod app;
mod ui;
mod ssh;
mod config;
mod plugins;
mod utils;

use eframe::egui;

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    
    // 尝试加载系统的微软雅黑字体
    if cfg!(windows) {
        // Windows 系统字体路径
        let font_paths = [
            "C:\\Windows\\Fonts\\msyh.ttc",     // 微软雅黑
            "C:\\Windows\\Fonts\\simsun.ttc",  // 宋体 (备选)
            "C:\\Windows\\Fonts\\simhei.ttf",  // 黑体 (备选)
        ];
        
        for (i, font_path) in font_paths.iter().enumerate() {
            if let Ok(font_data) = std::fs::read(font_path) {
                let font_name = format!("chinese_font_{}", i);
                
                // 将字体数据添加到字体定义中
                fonts.font_data.insert(
                    font_name.clone(),
                    egui::FontData::from_owned(font_data).into(),
                );
                
                // 将中文字体设置为比例字体的首选项
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .insert(0, font_name.clone());
                
                // 将中文字体添加到等宽字体的备选项中
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Monospace)
                    .unwrap()
                    .insert(0, font_name);
                
                log::info!("Successfully loaded font: {}", font_path);
                break; // 成功加载一个字体就够了
            } else {
                log::warn!("Failed to load font: {}", font_path);
            }
        }
    }
    
    // 应用字体配置
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
