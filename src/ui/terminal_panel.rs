use crate::ssh::SshManager;

use eframe::egui;
use egui_phosphor::regular;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use vt100;

pub struct TerminalPanel {
    pub title: String,
    pub connection_info: String,
    pub output_buffer: VecDeque<String>,
    input_buffer: String,
    scroll_to_bottom: bool,
    pub is_connected: bool,
    ssh_manager: Option<Arc<Mutex<SshManager>>>,
    pub tab_id: Option<String>,
    command_receiver: Option<mpsc::UnboundedReceiver<CommandResult>>,
    command_sender: Option<mpsc::UnboundedSender<CommandResult>>,
    current_prompt: String, // 当前提示符，如 "(base) ➜  ~"
    ssh_command_executor:
        Option<Box<dyn Fn(&str, &str, mpsc::UnboundedSender<CommandResult>) + Send + Sync>>, // SSH命令执行回调
}

// 手动实现Debug trait，因为Parser不实现Debug
impl std::fmt::Debug for TerminalPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalPanel")
            .field("title", &self.title)
            .field("connection_info", &self.connection_info)
            .field("output_buffer", &self.output_buffer)
            .field("input_buffer", &self.input_buffer)
            .field("scroll_to_bottom", &self.scroll_to_bottom)
            .field("is_connected", &self.is_connected)
            .field("ssh_manager", &self.ssh_manager)
            .field("tab_id", &self.tab_id)
            .field("current_prompt", &self.current_prompt)
            .field("ssh_command_executor", &"Function(hidden)") // 隐藏函数的内部细节
            .finish_non_exhaustive()
    }
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
            tab_id: self.tab_id.clone(),
            command_receiver: Some(receiver),
            command_sender: Some(sender),
            current_prompt: self.current_prompt.clone(),
            ssh_command_executor: None, // 克隆时不复制函数
        }
    }
}

impl TerminalPanel {
    pub fn new(title: String, connection_info: String) -> Self {
        let output_buffer = VecDeque::new();

        let (sender, receiver) = mpsc::unbounded_channel();

        Self {
            title,
            connection_info: connection_info.clone(),
            output_buffer,
            input_buffer: String::new(),
            scroll_to_bottom: true,
            is_connected: false,
            ssh_manager: None,
            tab_id: None,
            command_receiver: Some(receiver),
            command_sender: Some(sender),
            current_prompt: "❯".to_string(), // 默认提示符
            ssh_command_executor: None,      // 初始化时为空，稍后设置
        }
    }

    // 设置SSH管理器和tab_id（点击连接时立即调用）
    pub fn set_ssh_manager(&mut self, ssh_manager: Arc<Mutex<SshManager>>, tab_id: String) {
        self.ssh_manager = Some(ssh_manager);
        self.tab_id = Some(tab_id); // 立即设置tab_id，用于区分展示方式
    }

    // 设置SSH命令执行器
    pub fn set_ssh_command_executor<F>(&mut self, executor: F)
    where
        F: Fn(&str, &str, mpsc::UnboundedSender<CommandResult>) + Send + Sync + 'static,
    {
        self.ssh_command_executor = Some(Box::new(executor));
    }

    pub fn get_command_sender(&self) -> Option<mpsc::UnboundedSender<CommandResult>> {
        self.command_sender.clone()
    }

    // 更新连接信息显示
    pub fn update_connection_info(&mut self) {
        if let (Some(ssh_manager), Some(tab_id)) = (&self.ssh_manager, &self.tab_id) {
            if let Ok(manager) = ssh_manager.try_lock() {
                if let Some(info) = manager.get_connection_info(tab_id) {
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

    // SSH输出处理 - 使用VT100处理ANSI序列，但不自己解释内容
    pub fn add_ssh_output(&mut self, text: String) {
        if !text.is_empty() {
            crate::app_log!(info, "SSH", "收到SSH输出: {} 字节", text.len());

            // 检查是否包含ANSI转义序列
            if text.contains('\x1b') {
                // 包含ANSI序列，使用VT100处理得到干净的文本
                let mut parser = vt100::Parser::new(200, 50, 0);
                parser.process(text.as_bytes());
                let clean_text = parser.screen().contents();

                crate::app_log!(debug, "SSH", "VT100处理后: {}", clean_text.trim());
                self.add_output(clean_text);
            } else {
                // 纯文本，直接显示
                self.add_output(text);
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // 检查是否有命令结果需要处理
        self.process_command_results();

        // 更新连接信息
        self.update_connection_info();

        // 设置现代终端样式 - 参考VS Code Terminal和iTerm2
        let terminal_style = egui::Style {
            visuals: egui::Visuals {
                dark_mode: true,
                panel_fill: egui::Color32::from_rgb(30, 30, 30), // 更现代的深灰色
                window_fill: egui::Color32::from_rgb(24, 24, 24), // 纯深色背景
                override_text_color: Some(egui::Color32::from_rgb(224, 224, 224)), // 柔和的白色
                ..ui.style().visuals.clone()
            },
            spacing: egui::style::Spacing {
                item_spacing: egui::vec2(8.0, 6.0),
                button_padding: egui::vec2(16.0, 8.0),
                indent: 20.0,
                ..ui.style().spacing.clone()
            },
            ..ui.style().as_ref().clone()
        };
        ui.set_style(std::sync::Arc::new(terminal_style));

        // 现代化状态栏 - 参考VS Code集成终端
        egui::TopBottomPanel::top("terminal_status")
            .exact_height(44.0)
            .show_inside(ui, |ui| {
                // 现代状态栏背景 - 渐变效果
                let rect = ui.available_rect_before_wrap();
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::ZERO,
                    egui::Color32::from_rgb(40, 40, 40),
                );

                // 底部分隔线
                ui.painter().hline(
                    rect.left()..=rect.right(),
                    rect.bottom(),
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 60)),
                );

                ui.horizontal(|ui| {
                    ui.add_space(16.0);

                    let current_status = self.check_connection_status();
                    let (status_icon, status_color, status_text) = if current_status {
                        ("●", egui::Color32::from_rgb(40, 167, 69), "已连接") // GitHub绿色
                    } else {
                        ("●", egui::Color32::from_rgb(203, 36, 49), "未连接") // GitHub红色
                    };

                    // 更新内部状态
                    self.is_connected = current_status;

                    // 现代化状态指示器
                    ui.colored_label(status_color, egui::RichText::new(status_icon).size(14.0));
                    ui.add_space(8.0);

                    // 连接信息 - 更现代的字体
                    ui.label(
                        egui::RichText::new(&self.connection_info)
                            .font(egui::FontId::monospace(14.0))
                            .color(egui::Color32::from_rgb(171, 178, 191)), // VS Code字体颜色
                    );

                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(status_text)
                            .font(egui::FontId::proportional(13.0))
                            .color(status_color),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(16.0);

                        // 现代化按钮组
                        ui.horizontal(|ui| {
                            // 清屏按钮 - 现代扁平设计
                            let clear_btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(format!("{}", regular::ERASER)).size(14.0),
                                )
                                .fill(egui::Color32::from_rgb(52, 53, 65)) // 深灰色
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 70, 70)))
                                .corner_radius(egui::CornerRadius::same(6)),
                            );

                            if clear_btn.clicked() {
                                self.output_buffer.clear();
                            }

                            ui.add_space(8.0);

                            // 重连按钮 - GitHub风格
                            let reconnect_btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(format!("{}", regular::ARROW_CLOCKWISE))
                                        .size(14.0),
                                )
                                .fill(egui::Color32::from_rgb(13, 110, 253)) // Bootstrap蓝色
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    egui::Color32::from_rgb(13, 110, 253),
                                ))
                                .corner_radius(egui::CornerRadius::same(6)),
                            );

                            if reconnect_btn.clicked() {
                                self.disconnect();
                                self.add_output("已断开连接，请重新选择连接配置".to_string());
                            }
                        });
                    });
                });
            });

        // 现代化输入区域 - 参考iTerm2和Windows Terminal
        egui::TopBottomPanel::bottom("terminal_input")
            .exact_height(64.0)
            .show_inside(ui, |ui| {
                let rect = ui.available_rect_before_wrap();

                // 现代输入区域背景 - 更深的色调
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::ZERO,
                    egui::Color32::from_rgb(32, 32, 32),
                );

                // 顶部分隔线
                ui.painter().hline(
                    rect.left()..=rect.right(),
                    rect.top(),
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 60)),
                );

                // 垂直居中布局
                ui.allocate_ui_with_layout(
                    ui.available_size(),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add_space(20.0);

                        // 现代化提示符 - VS Code风格
                        ui.label(
                            egui::RichText::new(&self.current_prompt)
                                .font(egui::FontId::monospace(15.0))
                                .color(egui::Color32::from_rgb(78, 201, 176)), // 青绿色提示符
                        );

                        ui.add_space(16.0);

                        // 现代化输入框样式
                        let input_style = ui.style_mut();
                        input_style.visuals.widgets.inactive.bg_fill =
                            egui::Color32::from_rgb(45, 45, 45);
                        input_style.visuals.widgets.hovered.bg_fill =
                            egui::Color32::from_rgb(50, 50, 50);
                        input_style.visuals.widgets.active.bg_fill =
                            egui::Color32::from_rgb(24, 24, 24);
                        input_style.visuals.widgets.inactive.fg_stroke.color =
                            egui::Color32::from_rgb(224, 224, 224);
                        input_style.visuals.widgets.hovered.fg_stroke.color =
                            egui::Color32::from_rgb(255, 255, 255);
                        input_style.visuals.widgets.active.fg_stroke.color =
                            egui::Color32::from_rgb(255, 255, 255);
                        input_style.visuals.selection.bg_fill =
                            egui::Color32::from_rgb(0, 120, 215); // Windows蓝色选择
                        input_style.visuals.widgets.inactive.corner_radius =
                            egui::CornerRadius::same(8);
                        input_style.visuals.widgets.hovered.corner_radius =
                            egui::CornerRadius::same(8);
                        input_style.visuals.widgets.active.corner_radius =
                            egui::CornerRadius::same(8);

                        // 现代化输入框 - 更好的视觉效果，支持中文输入
                        let input_response = ui.add_sized(
                            [ui.available_width() - 120.0, 40.0],
                            egui::TextEdit::singleline(&mut self.input_buffer)
                                .font(egui::FontId::monospace(15.0))
                                .hint_text("输入命令并按回车...")
                                .desired_width(f32::INFINITY)
                                .char_limit(1000), // 设置字符限制，确保有足够空间输入中文
                        );

                        // 自动获得焦点，便于输入
                        if !input_response.has_focus() && self.is_connected {
                            input_response.request_focus();
                        }

                        // 修复回车键处理 - 检查焦点状态和按键
                        if input_response.has_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            self.execute_command();
                        }
                        // 也支持失去焦点时的回车
                        if input_response.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            self.execute_command();
                        }

                        ui.add_space(16.0);

                        // 现代化发送按钮 - GitHub Actions风格
                        let send_btn = ui.add_sized(
                            [80.0, 40.0],
                            egui::Button::new(
                                egui::RichText::new(format!("{}", regular::PAPER_PLANE_TILT))
                                    .size(16.0)
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(35, 134, 54)) // GitHub绿色
                            .stroke(egui::Stroke::NONE)
                            .corner_radius(egui::CornerRadius::same(8)),
                        );

                        if send_btn.clicked() {
                            self.execute_command();
                        }

                        ui.add_space(20.0);
                    },
                );
            });

        // 现代化终端内容区域 - 参考Code和Terminal.app
        egui::CentralPanel::default().show_inside(ui, |ui| {
            // 终端背景 - 纯黑色背景，如真实终端
            ui.painter().rect_filled(
                ui.available_rect_before_wrap(),
                egui::CornerRadius::ZERO,
                egui::Color32::from_rgb(12, 12, 12), // 纯黑背景
            );

            // 现代化边距和滚动
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(20, 16))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(self.scroll_to_bottom)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                // 现代化终端输出样式
                                for line in &self.output_buffer {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing.x = 0.0;

                                        // 现代终端颜色方案 - 参考One Dark Pro
                                        if line.starts_with("$ ") {
                                            // 命令行 - 青色
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(line)
                                                        .font(egui::FontId::monospace(14.0))
                                                        .color(egui::Color32::from_rgb(
                                                            86, 182, 194,
                                                        )), // 青色
                                                )
                                                .wrap(),
                                            );
                                        } else if line.contains("错误")
                                            || line.contains("失败")
                                            || line.contains("Error")
                                        {
                                            // 错误信息 - 红色
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(line)
                                                        .font(egui::FontId::monospace(14.0))
                                                        .color(egui::Color32::from_rgb(
                                                            224, 108, 117,
                                                        )), // 柔和红色
                                                )
                                                .wrap(),
                                            );
                                        } else if line.contains("连接") || line.contains("成功")
                                        {
                                            // 成功信息 - 绿色
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(line)
                                                        .font(egui::FontId::monospace(14.0))
                                                        .color(egui::Color32::from_rgb(
                                                            152, 195, 121,
                                                        )), // 柔和绿色
                                                )
                                                .wrap(),
                                            );
                                        } else if line.contains("正在") || line.contains("...") {
                                            // 进度信息 - 黄色
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(line)
                                                        .font(egui::FontId::monospace(14.0))
                                                        .color(egui::Color32::from_rgb(
                                                            229, 192, 123,
                                                        )), // 柔和黄色
                                                )
                                                .wrap(),
                                            );
                                        } else {
                                            // 普通输出 - 高对比度白色
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(line)
                                                        .font(egui::FontId::monospace(14.0))
                                                        .color(egui::Color32::from_rgb(
                                                            171, 178, 191,
                                                        )), // VS Code默认文本色
                                                )
                                                .wrap(),
                                            );
                                        }
                                    });
                                }

                                // 现代化欢迎界面
                                if self.output_buffer.is_empty() {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(60.0);
                                        ui.label(
                                            egui::RichText::new("🚀 终端已就绪")
                                                .font(egui::FontId::proportional(18.0))
                                                .color(egui::Color32::from_rgb(86, 182, 194)),
                                        );
                                        ui.add_space(12.0);
                                        ui.label(
                                            egui::RichText::new("在下方输入命令开始使用")
                                                .font(egui::FontId::proportional(14.0))
                                                .color(egui::Color32::from_rgb(171, 178, 191)),
                                        );
                                    });
                                }
                            });
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
            // 特殊处理连接相关的命令
            match result.command.as_str() {
                "connect_success" => {
                    // 连接成功，设置连接状态并显示欢迎信息
                    self.is_connected = true;
                    if let Ok(output) = result.output {
                        self.add_output(output);
                    }
                }
                "initial_output" => {
                    // 处理初始shell输出（欢迎信息和提示符） - 使用VT100解析
                    if let Ok(output) = result.output {
                        // 使用专门的SSH输出处理方法，会进行VT100解析和提示符提取
                        self.add_ssh_output(output);
                    }
                }
                "connect_failed" => {
                    // 连接失败，但保持在终端界面，只更新连接状态
                    self.is_connected = false;
                    // 注意：不清除 tab_id，保持在终端界面
                    // 也不清除 ssh_manager，用户可能想重试
                    if let Err(error) = result.output {
                        self.add_output(error.clone());
                    }
                }
                "connect" => {
                    // 兼容老的连接命令格式
                    match result.output {
                        Ok(output) => {
                            self.is_connected = true;
                            self.add_output(output);
                        }
                        Err(error) => {
                            self.is_connected = false;
                            self.add_output(format!("连接错误: {}", error));
                        }
                    }
                }

                _ => {
                    // 普通SSH命令处理 - 使用VT100解析
                    // 注意：命令已在execute_command中显示，这里只显示结果
                    match result.output {
                        Ok(output) => {
                            if !output.trim().is_empty() {
                                // 使用SSH输出处理方法，会进行VT100解析和提示符更新
                                self.add_ssh_output(output);
                            }
                        }
                        Err(error) => {
                            // SSH错误信息现在包含实际的命令输出，直接显示
                            self.add_ssh_output(error);
                        }
                    }
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

            if self.is_connected && self.tab_id.is_some() {
                // 直接调用SSH命令执行器
                let tab_id = self.tab_id.clone().unwrap();
                let cmd = command.trim().to_string();
                let sender = self.command_sender.clone();

                if let (Some(executor), Some(sender)) = (&self.ssh_command_executor, sender) {
                    executor(&tab_id, &cmd, sender);
                } else {
                    self.add_output("错误: SSH命令执行器未初始化".to_string());
                }
            } else {
                self.add_output("错误: 未连接到远程主机".to_string());
            }

            self.input_buffer.clear();
        }
    }

    // 检查连接状态
    pub fn check_connection_status(&self) -> bool {
        if let (Some(ssh_manager), Some(tab_id)) = (&self.ssh_manager, &self.tab_id) {
            // 尝试获取锁来检查连接状态
            if let Ok(manager) = ssh_manager.try_lock() {
                manager.is_connected(tab_id)
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

        if let (Some(ssh_manager), Some(tab_id)) = (&self.ssh_manager, &self.tab_id) {
            if let Ok(mut manager) = ssh_manager.try_lock() {
                manager.disconnect(tab_id);
                should_disconnect = true;
            }
        }

        if should_disconnect {
            self.is_connected = false;
            self.tab_id = None; // 清除tab_id，回到快速连接界面
            self.ssh_manager = None; // 清除SSH管理器
            self.add_output("连接已断开".to_string());
        }
    }
}
