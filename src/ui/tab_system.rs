use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::ssh::SyncSshManager;
use crate::ui::{ConnectionConfig, ConnectionManager, PluginsPanel, SimpleTerminalPanel};

/// Tab系统的核心trait - Strategy Pattern
pub trait TabContent {
    fn get_title(&self) -> String;
    fn get_id(&self) -> String;
    fn show(&mut self, ui: &mut egui::Ui, context: &mut TabContext);
    fn can_close(&self) -> bool;
    fn on_close(&mut self);
    fn get_tab_type(&self) -> TabType;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// Tab类型枚举
#[derive(Debug, Clone, PartialEq)]
pub enum TabType {
    Welcome,      // 欢迎/连接管理页面
    Terminal,     // 终端页面
    FileExplorer, // 文件浏览器（未来扩展）
    SystemInfo,   // 系统信息（未来扩展）
}

/// Tab上下文 - 提供Tab间共享的资源
pub struct TabContext {
    pub config: AppConfig,
    pub connection_manager: ConnectionManager,
    pub plugins_panel: PluginsPanel,
    pub pending_connection: Option<ConnectionConfig>, // 新增：待处理的连接请求
}

/// 欢迎Tab - 显示连接管理界面
pub struct WelcomeTab {
    id: String,
    title: String,
}

impl WelcomeTab {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: "快速连接".to_string(),
        }
    }
}

impl TabContent for WelcomeTab {
    fn get_title(&self) -> String {
        self.title.clone()
    }

    fn get_id(&self) -> String {
        self.id.clone()
    }

    fn show(&mut self, ui: &mut egui::Ui, context: &mut TabContext) {
        ui.horizontal(|ui| {
            // 左侧：系统监控面板
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() * 0.4);
                ui.heading("🖥️ 系统监控");
                context.plugins_panel.show(ui);
            });

            ui.separator();

            // 右侧：连接管理（终端列表）
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                ui.heading("🔗 终端连接");
                
                // 固定连接列表的尺寸
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(ui.available_width(), 400.0), // 固定高度400px
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        // 使用ScrollArea包装连接管理器，确保内容不会超出固定区域
                        egui::ScrollArea::vertical()
                            .max_height(380.0) // 留一点边距
                            .show(ui, |ui| {
                                if let Some(connection_config) = context.connection_manager.show(ui, &mut context.config) {
                                    // 将连接请求存储到上下文中，TabManager会处理它
                                    crate::app_log!(info, "Tab", "请求创建新的终端连接: {}@{}", 
                                        connection_config.username, connection_config.host);
                                    context.pending_connection = Some(connection_config);
                                }
                            });
                    }
                );
            });
        });
    }

    fn can_close(&self) -> bool {
        false // 欢迎Tab不能关闭
    }

    fn on_close(&mut self) {
        // 不执行任何操作
    }

    fn get_tab_type(&self) -> TabType {
        TabType::Welcome
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// 终端Tab - 包装SimpleTerminalPanel
pub struct TerminalTab {
    id: String,
    title: String,
    terminal: SimpleTerminalPanel,
    connection_config: Option<ConnectionConfig>,
}

impl TerminalTab {
    pub fn new(title: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: title.clone(),
            terminal: SimpleTerminalPanel::new(title, "未连接".to_string()),
            connection_config: None,
        }
    }

    pub fn new_with_connection(connection_config: ConnectionConfig) -> Self {
        let title = format!("{}@{}", connection_config.username, connection_config.host);
        let id = Uuid::new_v4().to_string();
        let connection_info = format!("正在连接到 {}@{}:{}...", 
            connection_config.username, connection_config.host, connection_config.port);
        let terminal = SimpleTerminalPanel::new(title.clone(), connection_info);
        
        // 这里暂时不直接连接，而是在show()方法中处理连接
        // 因为SimpleTerminalPanel需要SyncSshManager才能连接

        Self {
            id,
            title,
            terminal,
            connection_config: Some(connection_config),
        }
    }

    pub fn get_connection_config(&self) -> Option<&ConnectionConfig> {
        self.connection_config.as_ref()
    }
}

impl TabContent for TerminalTab {
    fn get_title(&self) -> String {
        if self.terminal.is_connected {
            format!("🟢 {}", self.title)
        } else {
            format!("🔴 {}", self.title)
        }
    }

    fn get_id(&self) -> String {
        self.id.clone()
    }

    fn show(&mut self, ui: &mut egui::Ui, _context: &mut TabContext) {
        self.terminal.show(ui);
    }

    fn can_close(&self) -> bool {
        true
    }

    fn on_close(&mut self) {
        crate::app_log!(info, "Tab", "关闭终端Tab: {}", self.title);
        self.terminal.disconnect();
    }

    fn get_tab_type(&self) -> TabType {
        TabType::Terminal
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Tab工厂 - Factory Pattern
pub struct TabFactory;

impl TabFactory {
    pub fn create_welcome_tab() -> Box<dyn TabContent> {
        Box::new(WelcomeTab::new())
    }

    pub fn create_terminal_tab(title: String) -> Box<dyn TabContent> {
        Box::new(TerminalTab::new(title))
    }

    pub fn create_terminal_tab_with_connection(connection_config: ConnectionConfig) -> Box<dyn TabContent> {
        Box::new(TerminalTab::new_with_connection(connection_config))
    }
}

/// Tab事件系统 - Observer Pattern
#[derive(Debug, Clone)]
pub enum TabEvent {
    CreateTerminal(ConnectionConfig),
    CloseTab(String),
    SwitchTab(String),
    RenameTab(String, String),
}

pub trait TabObserver {
    fn on_tab_event(&mut self, event: TabEvent);
}

/// Tab管理器 - 管理所有Tab的生命周期
pub struct TabManager {
    tabs: HashMap<String, Box<dyn TabContent>>,
    active_tab_id: Option<String>,
    observers: Vec<Box<dyn TabObserver>>,
    context: TabContext,
    ssh_manager: Arc<SyncSshManager>, // 新增：SSH管理器
}

impl TabManager {
    pub fn new(config: AppConfig) -> Self {
        let mut tabs = HashMap::new();
        
        // 创建默认的欢迎Tab
        let welcome_tab = TabFactory::create_welcome_tab();
        let welcome_id = welcome_tab.get_id();
        tabs.insert(welcome_id.clone(), welcome_tab);

        let ssh_manager = Arc::new(SyncSshManager::new());
        
        Self {
            tabs,
            active_tab_id: Some(welcome_id),
            observers: Vec::new(),
            context: TabContext {
                config,
                connection_manager: ConnectionManager::new(),
                plugins_panel: PluginsPanel::new(),
                pending_connection: None, // 初始化为None
            },
            ssh_manager,
        }
    }

    pub fn add_observer(&mut self, observer: Box<dyn TabObserver>) {
        self.observers.push(observer);
    }

    pub fn notify_observers(&mut self, event: TabEvent) {
        for observer in &mut self.observers {
            observer.on_tab_event(event.clone());
        }
    }

    pub fn create_terminal_tab(&mut self, connection_config: ConnectionConfig) {
        let mut tab = TabFactory::create_terminal_tab_with_connection(connection_config.clone());
        let tab_id = tab.get_id();
        
        // 如果是TerminalTab，设置SSH管理器并尝试连接
        if let Some(terminal_tab) = tab.as_any_mut().downcast_mut::<TerminalTab>() {
            terminal_tab.terminal.set_ssh_manager(Arc::clone(&self.ssh_manager), tab_id.clone());
            
            // 尝试创建SSH连接
            if let Some(config) = terminal_tab.connection_config.as_ref() {
                match self.ssh_manager.create_connection(tab_id.clone(), config) {
                    Ok(_) => {
                        crate::app_log!(info, "TabManager", "SSH连接创建成功: {}@{}", 
                            config.username, config.host);
                        terminal_tab.terminal.is_connected = true;
                    }
                    Err(e) => {
                        crate::app_log!(error, "TabManager", "SSH连接创建失败: {}", e);
                        terminal_tab.terminal.connection_info = format!("连接失败: {}", e);
                    }
                }
            }
        }
        
        self.tabs.insert(tab_id.clone(), tab);
        self.active_tab_id = Some(tab_id.clone());
        
        crate::app_log!(info, "TabManager", "创建新终端Tab: {}", tab_id);
        self.notify_observers(TabEvent::CreateTerminal(connection_config));
    }

    pub fn create_empty_terminal_tab(&mut self) {
        let tab_count = self.tabs.len();
        let tab = TabFactory::create_terminal_tab(format!("终端 {}", tab_count));
        let tab_id = tab.get_id();
        
        self.tabs.insert(tab_id.clone(), tab);
        self.active_tab_id = Some(tab_id.clone());
        
        crate::app_log!(info, "TabManager", "创建新空终端Tab: {}", tab_id);
    }

    pub fn close_tab(&mut self, tab_id: &str) {
        if let Some(mut tab) = self.tabs.remove(tab_id) {
            if tab.can_close() {
                tab.on_close();
                crate::app_log!(info, "TabManager", "关闭Tab: {}", tab_id);
                
                // 如果关闭的是当前活跃Tab，切换到其他Tab
                if self.active_tab_id.as_ref() == Some(&tab_id.to_string()) {
                    self.active_tab_id = self.tabs.keys().next().map(|s| s.clone());
                }
                
                self.notify_observers(TabEvent::CloseTab(tab_id.to_string()));
            } else {
                // 不能关闭，重新插入
                self.tabs.insert(tab_id.to_string(), tab);
            }
        }
    }

    pub fn switch_tab(&mut self, tab_id: &str) {
        if self.tabs.contains_key(tab_id) {
            self.active_tab_id = Some(tab_id.to_string());
            self.notify_observers(TabEvent::SwitchTab(tab_id.to_string()));
        }
    }

    pub fn get_active_tab(&mut self) -> Option<&mut Box<dyn TabContent>> {
        if let Some(active_id) = &self.active_tab_id {
            self.tabs.get_mut(active_id)
        } else {
            None
        }
    }

    pub fn get_all_tabs(&self) -> &HashMap<String, Box<dyn TabContent>> {
        &self.tabs
    }

    pub fn get_active_tab_id(&self) -> Option<&String> {
        self.active_tab_id.as_ref()
    }

    pub fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        // 收集需要执行的操作，避免借用检查问题
        let mut tab_to_switch: Option<String> = None;
        let mut tab_to_close: Option<String> = None;
        let mut create_new_tab = false;
        
        ui.horizontal(|ui| {
            // 收集Tab信息，避免在循环中修改self
            let tab_info: Vec<(String, String, bool, bool)> = self.tabs.iter()
                .map(|(id, tab)| (
                    id.clone(),
                    tab.get_title(),
                    self.active_tab_id.as_ref() == Some(id),
                    tab.can_close()
                ))
                .collect();
            
            // 渲染所有Tab按钮
            for (tab_id, title, is_active, can_close) in tab_info {
                ui.horizontal(|ui| {
                    if ui.selectable_label(is_active, &title).clicked() {
                        tab_to_switch = Some(tab_id.clone());
                    }
                    
                    // 显示关闭按钮（如果Tab可以关闭）
                    if can_close {
                        if ui.small_button("✕").clicked() {
                            tab_to_close = Some(tab_id);
                        }
                    }
                });
            }
            
            ui.separator();
            
            // 添加新Tab按钮
            if ui.button("➕ 新终端").clicked() {
                create_new_tab = true;
            }
        });
        
        // 执行收集的操作
        if let Some(tab_id) = tab_to_switch {
            self.switch_tab(&tab_id);
        }
        
        if let Some(tab_id) = tab_to_close {
            self.close_tab(&tab_id);
        }
        
        if create_new_tab {
            self.create_empty_terminal_tab();
        }
    }

    pub fn render_active_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(active_id) = self.active_tab_id.clone() {
            if let Some(active_tab) = self.tabs.get_mut(&active_id) {
                active_tab.show(ui, &mut self.context);
            }
        }
        
        // 检查是否有待处理的连接请求
        if let Some(connection_config) = self.context.pending_connection.take() {
            crate::app_log!(info, "TabManager", "处理待处理的连接请求: {}@{}", 
                connection_config.username, connection_config.host);
            self.create_terminal_tab(connection_config);
        }
    }

    pub fn save_config(&mut self) {
        if let Err(e) = self.context.config.save() {
            crate::app_log!(error, "TabManager", "保存配置失败: {}", e);
        }
    }
}
