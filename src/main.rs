mod app;
mod config;
mod plugins;
mod ssh;
mod ui;
mod utils;

use eframe::egui;

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 添加Phosphor图标字体支持
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    // ✅ 优先配置等宽字体 - 确保终端字符对齐
    if cfg!(windows) {
        // Windows 等宽字体路径（按优先级排序）
        let monospace_fonts = [
            ("C:\\Windows\\Fonts\\consola.ttf", "Consolas"), // 最佳等宽字体
            ("C:\\Windows\\Fonts\\cour.ttf", "Courier New"), // 经典等宽字体
            ("C:\\Windows\\Fonts\\lucon.ttf", "Lucida Console"), // 系统等宽字体
        ];

        // 中文支持字体
        let chinese_fonts = [
            ("C:\\Windows\\Fonts\\msyh.ttc", "Microsoft YaHei"), // 微软雅黑
            ("C:\\Windows\\Fonts\\simsun.ttc", "SimSun"),        // 宋体
            ("C:\\Windows\\Fonts\\simhei.ttf", "SimHei"),        // 黑体
        ];

        // 1. 优先加载等宽字体
        for (font_path, font_name) in monospace_fonts.iter() {
            if let Ok(font_data) = std::fs::read(font_path) {
                fonts.font_data.insert(
                    font_name.to_string(),
                    egui::FontData::from_owned(font_data).into(),
                );

                // ✅ 等宽字体优先级最高
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Monospace)
                    .unwrap()
                    .insert(0, font_name.to_string());

                log::info!("成功加载等宽字体: {} ({})", font_name, font_path);
                break;
            }
        }

        // 2. 加载中文支持字体
        for (i, (font_path, font_name)) in chinese_fonts.iter().enumerate() {
            if let Ok(font_data) = std::fs::read(font_path) {
                let chinese_font_id = format!("chinese_font_{}", i);

                fonts.font_data.insert(
                    chinese_font_id.clone(),
                    egui::FontData::from_owned(font_data).into(),
                );

                // 中文字体作为等宽字体的后备
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Monospace)
                    .unwrap()
                    .push(chinese_font_id.clone());

                // 中文字体用于比例字体
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .insert(0, chinese_font_id);

                log::info!("成功加载中文字体: {} ({})", font_name, font_path);
                break;
            }
        }
    }

    // ✅ 设置终端专用的字体大小和间距
    ctx.set_fonts(fonts);

    // 优化全局样式以支持终端渲染
    let mut style = (*ctx.style()).clone();

    // 确保等宽字体的字符间距为0
    style
        .text_styles
        .insert(egui::TextStyle::Monospace, egui::FontId::monospace(14.0));

    // 优化终端专用样式
    style.spacing.item_spacing = egui::vec2(0.0, 6.0); // 水平无间距，垂直适中
    style.spacing.button_padding = egui::vec2(8.0, 4.0);

    ctx.set_style(style);

    log::info!("字体配置完成 - 等宽字体优先，支持中文");
}

fn main() -> eframe::Result<()> {
    // 初始化环境日志
    env_logger::init();

    // 初始化全局应用日志系统
    let logger = utils::logger::init_logger();
    
    // ✅ 清除旧的日志文件内容
    utils::logger::clear_log_file();
    
    logger.lock().unwrap().info("App", "应用程序启动");

    // 记录日志文件路径
    if let Ok(log_instance) = logger.lock()
        && let Some(log_path) = &log_instance.log_file_path
    {
        println!("📝 日志文件路径: {:?}", log_path);
        log::info!("日志文件路径: {:?}", log_path);
    }

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

            // 启用浅色主题（白底黑字，类似 iTerm2 明亮主题）
            cc.egui_ctx.set_visuals(egui::Visuals::light());

            // 创建应用实例
            Ok(Box::new(app::TerminalApp::new(cc)))
        }),
    )
}
