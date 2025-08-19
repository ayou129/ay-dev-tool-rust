use eframe::egui;
use egui_phosphor::regular;
use std::collections::HashMap;
use std::sync::Arc;


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
    ssh_manager: Arc<SshManager>,

    // 运行时
    runtime: Arc<tokio::runtime::Runtime>,
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

        // SSH管理器改为非锁版本，由各终端直接管理连接
        let ssh_manager = Arc::new(SshManager::new());

        // 创建运行时的Arc引用以便共享
        let runtime_arc = Arc::new(runtime);

        // 初始化 tabs - 默认创建一个显示连接列表的tab
        let mut tabs = HashMap::new();
        let mut default_terminal =
            TerminalPanel::new("快速连接".to_string(), "选择或添加连接".to_string());
        // 设置SSH命令执行器回调
        let ssh_manager_ref = ssh_manager.clone();
        let runtime_ref = runtime_arc.clone();
        default_terminal.set_ssh_command_executor(move |tab_id: &str, command: &str, sender| {
            let ssh_manager = ssh_manager_ref.clone();
            let tab_id = tab_id.to_string();
            let cmd = command.to_string();

            runtime_ref.spawn(async move {
                let result = match ssh_manager.execute_command(&tab_id, &cmd).await
                {
                    Ok(output) => {
                        log::info!("SSH命令执行成功: {} -> {}", cmd, output);
                        crate::ui::terminal_panel::CommandResult {
                            command: cmd.clone(),
                            output: Ok(output),
                        }
                    }
                    Err(e) => {
                        log::error!("SSH命令执行失败: {} -> {}", cmd, e);
                        crate::ui::terminal_panel::CommandResult {
                            command: cmd.clone(),
                            output: Err(e.to_string()),
                        }
                    }
                };

                // 发送结果回UI线程
                let _ = sender.send(result);
            });
        });
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
            runtime: runtime_arc,
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
        let mut terminal_panel =
            TerminalPanel::new("快速连接".to_string(), "选择或添加连接".to_string());

        // 设置SSH命令执行器回调
        let ssh_manager_ref = self.ssh_manager.clone();
        let runtime_ref = self.runtime.clone();
        terminal_panel.set_ssh_command_executor(move |tab_id: &str, command: &str, sender| {
            let ssh_manager = ssh_manager_ref.clone();
            let tab_id = tab_id.to_string();
            let cmd = command.to_string();

            runtime_ref.spawn(async move {
                let result = match ssh_manager.execute_command(&tab_id, &cmd).await
                {
                    Ok(output) => {
                        log::info!("SSH命令执行成功: {} -> {}", cmd, output);
                        crate::ui::terminal_panel::CommandResult {
                            command: cmd.clone(),
                            output: Ok(output),
                        }
                    }
                    Err(e) => {
                        log::error!("SSH命令执行失败: {} -> {}", cmd, e);
                        crate::ui::terminal_panel::CommandResult {
                            command: cmd.clone(),
                            output: Err(e.to_string()),
                        }
                    }
                };

                // 发送结果回UI线程
                let _ = sender.send(result);
            });
        });

        // 添加到 tabs 中
        self.tabs
            .insert(tab_id.clone(), TabContent::Terminal(terminal_panel, false));
        self.active_tab = tab_id;
    }

    // 获取当前连接状态信息 - UI中使用，保持同步
    fn get_connection_stats(&self) -> (usize, Vec<String>) {
        // 暂时返回空值，避免UI阻塞
        (0, vec![])
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
            // 设置SSH命令执行器回调
            let ssh_manager_ref = self.ssh_manager.clone();
            let runtime_ref = self.runtime.clone();
            terminal.set_ssh_command_executor(move |tab_id: &str, command: &str, sender| {
                let ssh_manager = ssh_manager_ref.clone();
                let tab_id = tab_id.to_string();
                let cmd = command.to_string();

                runtime_ref.spawn(async move {
                    let result = match ssh_manager.execute_command(&tab_id, &cmd).await
                    {
                        Ok(output) => {
                            log::info!("SSH命令执行成功: {} -> {}", cmd, output);
                            crate::ui::terminal_panel::CommandResult {
                                command: cmd.clone(),
                                output: Ok(output),
                            }
                        }
                        Err(e) => {
                            log::error!("SSH命令执行失败: {} -> {}", cmd, e);
                            crate::ui::terminal_panel::CommandResult {
                                command: cmd.clone(),
                                output: Err(e.to_string()),
                            }
                        }
                    };

                    // 发送结果回UI线程
                    let _ = sender.send(result);
                });
            });

            // 异步建立 SSH 连接
            let ssh_manager = self.ssh_manager.clone();
            let config = connection_config.clone();
            let tab_id = self.active_tab.clone();

            // 获取终端的命令发送器来通知连接结果
            let command_sender = terminal.get_command_sender();

            // 先尝试连接
            self.runtime.spawn(async move {
                    // 直接调用连接方法，无需锁
                    let connect_result = ssh_manager.connect(tab_id.clone(), &config).await;

                    match connect_result {
                        Ok(_) => {
                            // 记录连接成功日志
                            crate::app_log!(info, "SSH", "SSH连接建立成功: {}@{}:{}", config.username, config.host, config.port);

                            // 连接成功，先发送成功消息
                            if let Some(sender) = command_sender.clone() {
                                let _ = sender.send(crate::ui::terminal_panel::CommandResult {
                                    command: "connect_success".to_string(),
                                    output: Ok("✅ 连接成功".to_string()),
                                });
                            }

                            // 获取shell会话初始输出（包括Last login等信息）
                            crate::app_log!(info, "SSH", "准备调用get_shell_initial_output，tab_id: {}", tab_id);

                            // 直接获取初始输出，无需锁
                            crate::app_log!(info, "SSH", "开始调用get_shell_initial_output");
                            let initial_output_result = ssh_manager.get_shell_initial_output(&tab_id).await;

                            match initial_output_result {
                                Ok(initial_output) => {
                                    crate::app_log!(info, "SSH", "获取到shell初始输出: {}", initial_output);
                                    if let Some(sender) = command_sender {
                                        let _ = sender.send(crate::ui::terminal_panel::CommandResult {
                                            command: "initial_output".to_string(),
                                            output: Ok(initial_output),
                                        });
                                    }
                                }
                                Err(e) => {
                                    crate::app_log!(warn, "SSH", "获取shell初始输出失败: {}", e);
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

        // 连接统计 - 使用更安全的方式，降低频率避免锁竞争
        static mut FRAME_COUNT: u64 = 0;
        unsafe {
            FRAME_COUNT += 1;
            // 每500帧记录一次（约每8-10秒，取决于帧率）
            if FRAME_COUNT % 500 == 0 {
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
