use crate::ssh::SshManager;
use eframe::egui;
use egui_phosphor::regular;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct TerminalPanel {
    pub title: String,
    pub connection_info: String,
    pub output_buffer: VecDeque<String>,
    input_buffer: String,
    scroll_to_bottom: bool,
    is_connected: bool,
    ssh_manager: Option<Arc<Mutex<SshManager>>>,
    connection_id: Option<String>,
    command_receiver: Option<mpsc::UnboundedReceiver<CommandResult>>,
    command_sender: Option<mpsc::UnboundedSender<CommandResult>>,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub command: String,
    pub output: Result<String, String>,
}

// 手动实现Clone，因为mpsc通道不能直接clone
impl Clone for TerminalPanel {
    fn clone(&self) -> Self {
        // 为克隆创建新的通道
        let (sender, receiver) = mpsc::unbounded_channel();

        Self {
            title: self.title.clone(),
            connection_info: self.connection_info.clone(),
            output_buffer: self.output_buffer.clone(),
            input_buffer: self.input_buffer.clone(),
            scroll_to_bottom: self.scroll_to_bottom,
            is_connected: self.is_connected,
            ssh_manager: self.ssh_manager.clone(),
            connection_id: self.connection_id.clone(),
            command_receiver: Some(receiver),
            command_sender: Some(sender),
        }
    }
}

impl TerminalPanel {
    pub fn new(title: String, connection_info: String) -> Self {
        let mut output_buffer = VecDeque::new();
        output_buffer.push_back(format!("等待连接到 {}...", connection_info));

        let (sender, receiver) = mpsc::unbounded_channel();

        Self {
            title,
            connection_info: connection_info.clone(),
            output_buffer,
            input_buffer: String::new(),
            scroll_to_bottom: true,
            is_connected: false,
            ssh_manager: None,
            connection_id: None,
            command_receiver: Some(receiver),
            command_sender: Some(sender),
        }
    }

    pub fn set_ssh_manager(&mut self, ssh_manager: Arc<Mutex<SshManager>>, connection_id: String) {
        self.ssh_manager = Some(ssh_manager);
        self.connection_id = Some(connection_id);
    }

    pub fn get_command_sender(&self) -> Option<mpsc::UnboundedSender<CommandResult>> {
        self.command_sender.clone()
    }

    // 更新连接信息显示
    pub fn update_connection_info(&mut self) {
        if let (Some(ssh_manager), Some(connection_id)) = (&self.ssh_manager, &self.connection_id) {
            if let Ok(manager) = ssh_manager.try_lock() {
                if let Some(info) = manager.get_connection_info(connection_id) {
                    self.connection_info = format!("{}@{}:{}", info.username, info.host, info.port);
                }
            }
        }
    }

    pub fn add_output(&mut self, text: String) {
        self.output_buffer.push_back(text);

        // 限制缓冲区大小
        while self.output_buffer.len() > 10000 {
            self.output_buffer.pop_front();
        }

        self.scroll_to_bottom = true;
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.is_connected = connected;
        if connected {
            // 使用 execute_ssh_command 显示连接成功的欢迎信息
            self.execute_ssh_command("connect", "连接成功! 终端已就绪.");
        } else {
            self.add_output("连接断开".to_string());
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // 检查是否有命令结果需要处理
        self.process_command_results();

        // 更新连接信息
        self.update_connection_info();

        // 使用垂直布局，确保输入区域在底部
        egui::TopBottomPanel::top("terminal_status").show_inside(ui, |ui| {
            // 连接状态栏
            ui.horizontal(|ui| {
                let current_status = self.check_connection_status();
                let (status_icon, status_color) = if current_status {
                    ("●", egui::Color32::GREEN)
                } else {
                    ("●", egui::Color32::RED)
                };

                // 更新内部状态
                self.is_connected = current_status;

                ui.colored_label(status_color, status_icon);
                ui.label(&self.connection_info);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(
                            egui::RichText::new(format!("{} 重连", regular::ARROW_CLOCKWISE))
                                .size(14.0),
                        )
                        .clicked()
                    {
                        // 先断开现有连接
                        self.disconnect();
                        self.add_output("正在重新连接...".to_string());
                        // TODO: 这里应该触发重新连接逻辑
                    }

                    if ui
                        .button(egui::RichText::new(format!("{} 清屏", regular::ERASER)).size(14.0))
                        .clicked()
                    {
                        self.output_buffer.clear();
                    }
                });
            });
        });

        egui::TopBottomPanel::bottom("terminal_input").show_inside(ui, |ui| {
            // 终端输入区域 - 固定在底部
            ui.horizontal(|ui| {
                ui.label("$");

                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.input_buffer)
                        .font(egui::FontId::monospace(12.0))
                        .desired_width(ui.available_width() - 100.0),
                );

                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.execute_command();
                }

                if ui
                    .button(
                        egui::RichText::new(format!("{} 发送", regular::PAPER_PLANE_TILT))
                            .size(14.0),
                    )
                    .clicked()
                {
                    self.execute_command();
                }
            });
        });

        // 主终端输出区域 - 占用剩余空间
        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom(self.scroll_to_bottom)
                .show(ui, |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        for line in &self.output_buffer {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(line)
                                            .font(egui::FontId::monospace(12.0))
                                            .color(egui::Color32::WHITE),
                                    )
                                    .wrap(),
                                );
                            });
                        }
                    });
                });

            if self.scroll_to_bottom {
                self.scroll_to_bottom = false;
            }
        });
    }

    fn process_command_results(&mut self) {
        let mut results = Vec::new();

        if let Some(receiver) = &mut self.command_receiver {
            while let Ok(result) = receiver.try_recv() {
                results.push(result);
            }
        }

        for result in results {
            // 显示执行的命令
            if !result.command.trim().is_empty() {
                self.add_output(format!("$ {}", result.command));
            }

            match result.output {
                Ok(output) => {
                    if !output.trim().is_empty() {
                        self.add_output(output);
                    }
                }
                Err(error) => {
                    self.add_output(format!("错误: {}", error));
                }
            }
        }
    }

    fn execute_command(&mut self) {
        if !self.input_buffer.trim().is_empty() {
            let command = self.input_buffer.clone();

            if command.trim() == "clear" {
                self.output_buffer.clear();
                self.input_buffer.clear();
                return;
            }

            self.add_output(format!("$ {}", command));

            if self.is_connected && self.ssh_manager.is_some() && self.connection_id.is_some() {
                // 使用真正的SSH连接执行命令
                let ssh_manager = self.ssh_manager.clone().unwrap();
                let connection_id = self.connection_id.clone().unwrap();
                let cmd = command.trim().to_string();
                let sender = self.command_sender.clone();

                // 在后台执行SSH命令
                tokio::spawn(async move {
                    let result = match ssh_manager
                        .lock()
                        .await
                        .execute_command(&connection_id, &cmd)
                        .await
                    {
                        Ok(output) => {
                            log::info!("SSH命令执行成功: {} -> {}", cmd, output);
                            CommandResult {
                                command: cmd.clone(),
                                output: Ok(output),
                            }
                        }
                        Err(e) => {
                            log::error!("SSH命令执行失败: {} -> {}", cmd, e);
                            CommandResult {
                                command: cmd.clone(),
                                output: Err(e.to_string()),
                            }
                        }
                    };

                    // 发送结果回UI线程
                    if let Some(sender) = sender {
                        let _ = sender.send(result);
                    }
                });
            } else {
                self.add_output("错误: 未连接到远程主机".to_string());
            }

            self.input_buffer.clear();
        }
    }

    // 提供SSH命令执行的接口，用于直接添加命令执行结果
    pub fn execute_ssh_command(&mut self, command: &str, result: &str) {
        // 显示执行的命令和结果
        self.add_output(format!("$ {}", command));
        self.add_output(result.to_string());
    }

    // 检查连接状态
    pub fn check_connection_status(&self) -> bool {
        if let (Some(ssh_manager), Some(connection_id)) = (&self.ssh_manager, &self.connection_id) {
            // 尝试获取锁来检查连接状态
            if let Ok(manager) = ssh_manager.try_lock() {
                manager.is_connected(connection_id)
            } else {
                self.is_connected
            }
        } else {
            self.is_connected
        }
    }

    // 断开连接
    pub fn disconnect(&mut self) {
        let mut should_disconnect = false;

        if let (Some(ssh_manager), Some(connection_id)) = (&self.ssh_manager, &self.connection_id) {
            if let Ok(mut manager) = ssh_manager.try_lock() {
                manager.disconnect(connection_id);
                should_disconnect = true;
            }
        }

        if should_disconnect {
            self.is_connected = false;
            self.add_output("连接已断开".to_string());
        }
    }
}
