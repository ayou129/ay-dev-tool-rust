use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::AppConfig;
use crate::ui::{ConnectionManager, TerminalPanel, PluginsPanel};
use crate::ssh::SshManager;

pub struct TerminalApp {
    // 应用状态
    config: AppConfig,
    active_tab: String,
    tabs: HashMap<String, TabContent>,
    
    // UI 组件
    connection_manager: ConnectionManager,
    plugins_panel: PluginsPanel,
    
    // SSH 管理器
    ssh_manager: Arc<Mutex<SshManager>>,
    
    // 运行时
    runtime: tokio::runtime::Runtime,
}

#[derive(Debug, Clone)]
pub enum TabContent {
    ConnectionList,
    Terminal(TerminalPanel),
    Settings,
}

impl TerminalApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 创建 Tokio 运行时
        let runtime = tokio::runtime::Runtime::new().unwrap();
        
        // 加载配置
        let config = AppConfig::load().unwrap_or_default();
        
        // 创建 SSH 管理器
        let ssh_manager = Arc::new(Mutex::new(SshManager::new()));
        
        // 初始化 tabs
        let mut tabs = HashMap::new();
        tabs.insert("connections".to_string(), TabContent::ConnectionList);
        
        Self {
            config,
            active_tab: "connections".to_string(),
            tabs,
            connection_manager: ConnectionManager::new(),
            plugins_panel: PluginsPanel::new(),
            ssh_manager,
            runtime,
        }
    }

    fn render_top_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Tab 切换按钮
            for (tab_id, content) in &self.tabs {
                let tab_name = match content {
                    TabContent::ConnectionList => "Quick Connect",
                    TabContent::Terminal(terminal) => &terminal.title,
                    TabContent::Settings => "Settings",
                };
                
                if ui.selectable_label(
                    &self.active_tab == tab_id,
                    tab_name
                ).clicked() {
                    self.active_tab = tab_id.clone();
                }
            }
            
            ui.separator();
            
            // 添加新终端按钮
            if ui.button("➕").clicked() {
                // TODO: 实现新建终端逻辑
            }
        });
    }

    fn render_main_content(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        match self.tabs.get_mut(&self.active_tab) {
            Some(TabContent::ConnectionList) => {
                self.connection_manager.show(ui, &mut self.config);
            }
            Some(TabContent::Terminal(terminal)) => {
                terminal.show(ui);
            }
            Some(TabContent::Settings) => {
                ui.label("Settings panel - Coming soon");
            }
            None => {
                ui.label("No active tab");
            }
        }
    }

    fn render_side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("plugins")
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.heading("DevOps Plugins");
                ui.separator();
                
                self.plugins_panel.show(ui);
            });
    }
}

impl eframe::App for TerminalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 渲染侧边栏插件面板
        self.render_side_panel(ctx);
        
        // 主中央面板
        egui::CentralPanel::default().show(ctx, |ui| {
            // 顶部 Tab 区域
            egui::TopBottomPanel::top("tabs").show_inside(ui, |ui| {
                self.render_top_panel(ctx, ui);
            });
            
            // 主内容区域
            egui::CentralPanel::default().show_inside(ui, |ui| {
                self.render_main_content(ctx, ui);
            });
        });
        
        // 请求重绘以保持响应
        ctx.request_repaint();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // 保存应用配置
        if let Ok(config_str) = serde_json::to_string(&self.config) {
            storage.set_string("app_config", config_str);
        }
    }
}
