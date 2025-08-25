use eframe::egui;

use crate::config::AppConfig;
use crate::ui::{TabManager, TabEvent, TabObserver, ConnectionConfig};

/// 使用Tab系统的简化应用 - 应用设计模式
pub struct TabBasedApp {
    tab_manager: TabManager,
}

/// 应用级Tab观察者 - Observer Pattern实现
impl TabObserver for TabBasedApp {
    fn on_tab_event(&mut self, event: TabEvent) {
        match event {
            TabEvent::CreateTerminal(connection_config) => {
                crate::app_log!(info, "App", "处理创建终端事件: {}@{}", 
                    connection_config.username, connection_config.host);
                // TabManager已经处理了Tab创建，这里可以做额外的应用级处理
            }
            TabEvent::CloseTab(tab_id) => {
                crate::app_log!(info, "App", "处理关闭Tab事件: {}", tab_id);
                // 可以在这里做清理工作
            }
            TabEvent::SwitchTab(tab_id) => {
                crate::app_log!(debug, "App", "切换到Tab: {}", tab_id);
                // 可以在这里更新应用状态
            }
            TabEvent::RenameTab(tab_id, new_name) => {
                crate::app_log!(info, "App", "重命名Tab {} -> {}", tab_id, new_name);
            }
        }
    }
}

impl TabBasedApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        crate::app_log!(info, "App", "启动基于Tab系统的应用");

        // 加载配置
        let config = AppConfig::load().unwrap_or_default();

        // 创建Tab管理器
        let tab_manager = TabManager::new(config);

        // 注册观察者（暂时注释，因为需要解决借用问题）
        // let observer = Box::new(self);
        // tab_manager.add_observer(observer);

        Self {
            tab_manager,
        }
    }

    /// 处理连接请求 - 从WelcomeTab触发
    pub fn handle_connection_request(&mut self, connection_config: ConnectionConfig) {
        crate::app_log!(info, "App", "处理连接请求: {}@{}", 
            connection_config.username, connection_config.host);
        
        // 创建新的终端Tab
        self.tab_manager.create_terminal_tab(connection_config);
    }
}

impl eframe::App for TabBasedApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 顶部Tab栏
        egui::TopBottomPanel::top("tab_panel").show(ctx, |ui| {
            self.tab_manager.render_tab_bar(ui);
        });

        // 主内容区域
        egui::CentralPanel::default().show(ctx, |ui| {
            self.tab_manager.render_active_tab(ui);
        });

        // 持续重绘以保证PTY数据实时读取
        ctx.request_repaint();
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        // 保存配置
        self.tab_manager.save_config();
    }
}

/// Tab应用工厂 - Factory Pattern
pub struct TabAppFactory;

impl TabAppFactory {
    pub fn create_app(cc: &eframe::CreationContext<'_>) -> Box<dyn eframe::App> {
        Box::new(TabBasedApp::new(cc))
    }
}
