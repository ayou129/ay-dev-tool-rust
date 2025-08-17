use eframe::egui;
use egui_phosphor::regular;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::AppConfig;
use crate::ssh::SshManager;
use crate::ui::{ConnectionConfig, ConnectionManager, PluginsPanel, TerminalPanel};

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
    Terminal(TerminalPanel, bool), // TerminalPanel, is_connected
}

impl TerminalApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // 创建 Tokio 运行时
        let runtime = tokio::runtime::Runtime::new().unwrap();

        // 加载配置
        let config = AppConfig::load().unwrap_or_default();

        // 创建 SSH 管理器
        let ssh_manager = Arc::new(Mutex::new(SshManager::new()));

        // 初始化 tabs - 默认创建一个显示连接列表的tab
        let mut tabs = HashMap::new();
        let default_terminal =
            TerminalPanel::new("快速连接".to_string(), "选择或添加连接".to_string());
        tabs.insert(
            "tab_1".to_string(),
            TabContent::Terminal(default_terminal, false),
        );

        Self {
            config,
            active_tab: "tab_1".to_string(),
            tabs,
            connection_manager: ConnectionManager::new(),
            plugins_panel: PluginsPanel::new(),
            ssh_manager,
            runtime,
        }
    }

    fn render_top_panel(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Tab 切换按钮
            for (tab_id, content) in &self.tabs {
                let tab_name = match content {
                    TabContent::Terminal(terminal, _) => &terminal.title,
                };

                if ui
                    .selectable_label(&self.active_tab == tab_id, tab_name)
                    .clicked()
                {
                    self.active_tab = tab_id.clone();
                }
            }

            ui.separator();

            // 添加新终端按钮
            if ui
                .button(egui::RichText::new(format!("{} 新建", regular::PLUS)).size(16.0))
                .clicked()
            {
                self.create_new_tab();
            }
        });
    }

    fn render_main_content(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        match self.tabs.get_mut(&self.active_tab) {
            Some(TabContent::Terminal(terminal, _tab_is_connected)) => {
                // 使用 tab_id 判断是否已连接，有值就显示终端界面
                let has_connection = terminal.tab_id.is_some();
                
                if !has_connection {
                    // 显示连接列表（快速连接界面）
                    if let Some(connection_config) =
                        self.connection_manager.show(ui, &mut self.config)
                    {
                        self.connect_to_terminal(connection_config);
                    }
                } else {
                    // 显示终端界面
                    terminal.show(ui);
                }
            }
            None => {
                ui.label("No active tab");
            }
        }
    }

    fn render_side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("plugins")
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.heading("DevOps 插件");
                ui.separator();

                self.plugins_panel.show(ui);
            });
    }

    fn create_new_tab(&mut self) {
        // 生成唯一的 tab ID
        let tab_id = format!("tab_{}", self.tabs.len() + 1);

        // 创建新的终端面板（未连接状态）
        let terminal_panel =
            TerminalPanel::new("快速连接".to_string(), "选择或添加连接".to_string());

        // 添加到 tabs 中
        self.tabs
            .insert(tab_id.clone(), TabContent::Terminal(terminal_panel, false));
        self.active_tab = tab_id;
    }

    // 获取当前连接状态信息
    fn get_connection_stats(&self) -> (usize, Vec<String>) {
        if let Ok(manager) = self.ssh_manager.try_lock() {
            let connections = manager.get_connections();
            (connections.len(), connections)
        } else {
            (0, vec![])
        }
    }

    fn connect_to_terminal(&mut self, connection_config: ConnectionConfig) {
        if let Some(TabContent::Terminal(terminal, _is_connected)) =
            self.tabs.get_mut(&self.active_tab)
        {
            // 更新终端信息
            terminal.title = connection_config.name.clone();
            terminal.connection_info = format!(
                "{}@{}:{}",
                connection_config.username, connection_config.host, connection_config.port
            );

            // 显示连接过程
            terminal.add_output(format!(
                "正在连接到 {}@{}:{}...",
                connection_config.username, connection_config.host, connection_config.port
            ));

            // 设置SSH管理器和tab_id（立即切换到终端界面）
            terminal.set_ssh_manager(self.ssh_manager.clone(), self.active_tab.clone());

            // 异步建立 SSH 连接
            let ssh_manager = self.ssh_manager.clone();
            let config = connection_config.clone();
            let tab_id = self.active_tab.clone();

            // 获取终端的命令发送器来通知连接结果
            let command_sender = terminal.get_command_sender();

            // 先尝试连接
            self.runtime.spawn(async move {
                match ssh_manager
                    .lock()
                    .await
                    .connect(tab_id.clone(), &config)
                    .await
                {
                    Ok(_) => {
                        // 记录连接成功日志
                        crate::app_log!(info, "SSH", "SSH连接建立成功: {}@{}:{}", config.username, config.host, config.port);
                        
                        // 连接成功，先发送成功消息
                        if let Some(sender) = command_sender.clone() {
                            let _ = sender.send(crate::ui::terminal_panel::CommandResult {
                                command: "connect_success".to_string(),
                                output: Ok(format!("✅ 成功连接到 {}@{}:{}", config.username, config.host, config.port)),
                            });
                        }

                        // 获取初始shell输出（欢迎信息和提示符）
                        match ssh_manager.lock().await.get_initial_output(&tab_id).await {
                            Ok(initial_output) => {
                                crate::app_log!(info, "SSH", "获取到初始shell输出，长度: {} 字符", initial_output.len());
                                if let Some(sender) = command_sender {
                                    // 发送原始的shell输出
                                    let _ = sender.send(crate::ui::terminal_panel::CommandResult {
                                        command: "initial_output".to_string(),
                                        output: Ok(initial_output),
                                    });
                                }
                            }
                            Err(e) => {
                                crate::app_log!(warn, "SSH", "获取初始shell输出失败: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        // 连接失败，发送错误消息
                        if let Some(sender) = command_sender {
                            let _ = sender.send(crate::ui::terminal_panel::CommandResult {
                                command: "connect_failed".to_string(),
                                output: Err(format!("❌ 连接失败: {}\n\n请检查:\n• 主机地址和端口是否正确\n• 用户名和密码是否正确\n• 网络连接是否正常\n• 目标主机SSH服务是否启用", e)),
                            });
                        }
                    }
                }
            });

            // 注意：不在这里设置连接状态，而是在收到连接结果后设置
        }
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

        // 偶尔记录连接统计（每100帧一次）
        static mut FRAME_COUNT: u64 = 0;
        unsafe {
            FRAME_COUNT += 1;
            if FRAME_COUNT % 100 == 0 {
                let (count, connections) = self.get_connection_stats();
                if count > 0 {
                    log::debug!("当前活跃连接数: {}, 连接列表: {:?}", count, connections);
                }
            }
        }

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
