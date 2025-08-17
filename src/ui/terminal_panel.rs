use crate::ssh::SshManager;
use crate::utils::logger::{log_ansi_processing, log_prompt_extraction};
use eframe::egui;
use egui_phosphor::regular;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use vt100::Parser;

#[derive(Debug)]
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
    current_prompt: String,  // 当前提示符，如 "(base) ➜  ~"
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
            tab_id: None,
            command_receiver: Some(receiver),
            command_sender: Some(sender),
            current_prompt: "❯".to_string(),  // 默认提示符
        }
    }

    // 设置SSH管理器和tab_id（点击连接时立即调用）
    pub fn set_ssh_manager(&mut self, ssh_manager: Arc<Mutex<SshManager>>, tab_id: String) {
        self.ssh_manager = Some(ssh_manager);
        self.tab_id = Some(tab_id);  // 立即设置tab_id，用于区分展示方式
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
        self.output_buffer.push_back(text.clone());

        // 尝试从输出中提取提示符
        self.extract_prompt_from_output(&text);

        // 限制缓冲区大小
        while self.output_buffer.len() > 10000 {
            self.output_buffer.pop_front();
        }

        self.scroll_to_bottom = true;
    }

    // 从输出中提取提示符
    fn extract_prompt_from_output(&mut self, text: &str) {
        // 按行分割输出
        let lines: Vec<&str> = text.lines().collect();
        
        for line in lines.iter().rev() {  // 从最后一行开始查找
            let trimmed = line.trim();
            
            // 清理ANSI转义序列后检查
            let clean_line = self.strip_ansi_codes(trimmed);
            
            // 检查是否包含常见的提示符模式
            if self.is_prompt_like(&clean_line) {
                // 记录提示符提取日志
                log_prompt_extraction(trimmed, &clean_line);
                
                // 保存清理后的提示符用于显示
                self.current_prompt = clean_line;
                break;
            }
        }
    }

    // 使用专业的vt100库清理ANSI转义序列
    fn strip_ansi_codes(&self, text: &str) -> String {
        let original_length = text.len();
        
        // 创建一个虚拟终端解析器 (80列x24行，足够处理提示符)
        let mut parser = Parser::new(24, 80, 0);
        
        // 处理输入文本
        parser.process(text.as_bytes());
        
        // 获取解析后的纯文本内容
        let screen = parser.screen();
        
        // 使用 contents() 方法获取整个屏幕的文本内容
        let screen_contents = screen.contents();
        
        // 清理多余的空白字符和换行符
        let cleaned_text = screen_contents
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        
        let cleaned_length = cleaned_text.len();
        
        // 估算ANSI序列数量（原始长度 - 清理后长度的差异）
        let estimated_ansi_count = if original_length > cleaned_length {
            (original_length - cleaned_length) / 5  // 粗略估算，平均每个ANSI序列5个字符
        } else {
            0
        };
        
        // 记录ANSI处理日志
        if estimated_ansi_count > 0 {
            log_ansi_processing(original_length, cleaned_length, estimated_ansi_count);
        }
        
        cleaned_text
    }

    // 判断是否像提示符
    fn is_prompt_like(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }

        // 常见的提示符特征：
        // 1. 包含用户@主机模式: user@host
        // 2. 包含路径符号: ~ 或 /
        // 3. 包含常见提示符: $ # > ❯ ➜
        // 4. 包含环境标识: (base) (venv) 等
        // 5. 长度合理（不是很长的输出）

        let prompt_indicators = ["$", "#", ">", "❯", "➜", "~", "@"];
        let env_indicators = ["(base)", "(venv)", "(conda)"];
        
        // 长度检查：提示符通常不会太长
        if text.len() > 200 {
            return false;
        }

        // 包含提示符指示器
        let has_prompt_char = prompt_indicators.iter().any(|&indicator| text.contains(indicator));
        
        // 包含环境指示器
        let has_env_indicator = env_indicators.iter().any(|&indicator| text.contains(indicator));
        
        // 包含用户@主机模式
        let has_user_host = text.contains('@') && text.chars().filter(|&c| c == '@').count() == 1;

        // 以提示符字符结尾（常见模式）
        let ends_with_prompt = text.ends_with('$') || text.ends_with('#') || 
                              text.ends_with('>') || text.ends_with("❯ ") || 
                              text.ends_with("➜ ") || text.ends_with("~ ");

        // 符合条件的组合
        has_prompt_char && (has_env_indicator || has_user_host || ends_with_prompt)
    }



    pub fn show(&mut self, ui: &mut egui::Ui) {
        // 检查是否有命令结果需要处理
        self.process_command_results();

        // 更新连接信息
        self.update_connection_info();

        // 设置终端整体样式
        let terminal_style = egui::Style {
            visuals: egui::Visuals {
                dark_mode: true,
                panel_fill: egui::Color32::from_rgb(20, 22, 25),  // 深色背景
                window_fill: egui::Color32::from_rgb(25, 27, 30),
                override_text_color: Some(egui::Color32::from_rgb(240, 240, 240)),
                ..ui.style().visuals.clone()
            },
            ..ui.style().as_ref().clone()
        };
        ui.set_style(std::sync::Arc::new(terminal_style));

        // 状态栏 - 美化后的样式
        egui::TopBottomPanel::top("terminal_status")
            .exact_height(40.0)
            .show_inside(ui, |ui| {
                // 添加状态栏背景
                ui.painter().rect_filled(
                    ui.available_rect_before_wrap(),
                    egui::CornerRadius::same(4),
                    egui::Color32::from_rgb(35, 37, 40),
                );

                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    
                    let current_status = self.check_connection_status();
                    let (status_icon, status_color, status_text) = if current_status {
                        ("●", egui::Color32::from_rgb(0, 200, 83), "已连接")
                    } else {
                        ("●", egui::Color32::from_rgb(255, 69, 58), "未连接")
                    };

                    // 更新内部状态
                    self.is_connected = current_status;

                    // 状态指示器
                    ui.colored_label(
                        status_color, 
                        egui::RichText::new(status_icon).size(16.0)
                    );
                    ui.add_space(4.0);
                    
                    // 连接信息
                    ui.label(
                        egui::RichText::new(&self.connection_info)
                            .font(egui::FontId::monospace(13.0))
                            .color(egui::Color32::from_rgb(200, 200, 200))
                    );
                    
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(status_text)
                            .font(egui::FontId::proportional(12.0))
                            .color(status_color)
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        
                        // 重连按钮 - 美化样式
                        let reconnect_btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(format!("{} 重连", regular::ARROW_CLOCKWISE))
                                    .size(13.0)
                            )
                            .fill(egui::Color32::from_rgb(0, 122, 255))
                            .corner_radius(egui::CornerRadius::same(4))
                        );
                        
                        if reconnect_btn.clicked() {
                            self.disconnect();
                            self.add_output("已断开连接，请重新选择连接配置".to_string());
                        }

                        ui.add_space(4.0);

                        // 清屏按钮 - 美化样式
                        let clear_btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(format!("{} 清屏", regular::ERASER))
                                    .size(13.0)
                            )
                            .fill(egui::Color32::from_rgb(88, 86, 214))
                            .corner_radius(egui::CornerRadius::same(4))
                        );
                        
                        if clear_btn.clicked() {
                            self.output_buffer.clear();
                        }
                    });
                });
            });

        // 输入区域 - 美化后的样式
        egui::TopBottomPanel::bottom("terminal_input")
            .exact_height(60.0)
            .show_inside(ui, |ui| {
                // 添加输入区域背景
                ui.painter().rect_filled(
                    ui.available_rect_before_wrap(),
                    egui::CornerRadius::same(4),
                    egui::Color32::from_rgb(40, 42, 45),
                );

                // 使用垂直居中布局
                ui.allocate_ui_with_layout(
                    ui.available_size(),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add_space(16.0);
                        
                        // 美化的提示符 - 显示当前动态提示符
                        ui.label(
                            egui::RichText::new(&self.current_prompt)
                                .font(egui::FontId::monospace(16.0))
                                .color(egui::Color32::from_rgb(0, 200, 83))
                        );
                        
                        ui.add_space(12.0);

                        // 创建自定义样式的输入框
                        let input_style = ui.style_mut();
                        input_style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(55, 57, 60);
                        input_style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(65, 67, 70);
                        input_style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(70, 72, 75);
                        input_style.visuals.widgets.inactive.fg_stroke.color = egui::Color32::from_rgb(240, 240, 240);
                        input_style.visuals.widgets.hovered.fg_stroke.color = egui::Color32::from_rgb(255, 255, 255);
                        input_style.visuals.widgets.active.fg_stroke.color = egui::Color32::from_rgb(255, 255, 255);
                        input_style.visuals.selection.bg_fill = egui::Color32::from_rgb(0, 150, 200);
                        
                        // 美化的输入框 - 增强可见性
                        let input_response = ui.add_sized(
                            [ui.available_width() - 140.0, 36.0],
                            egui::TextEdit::singleline(&mut self.input_buffer)
                                .font(egui::FontId::monospace(15.0))
                                .hint_text("输入命令...")
                                .desired_width(f32::INFINITY)
                        );

                        if input_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.execute_command();
                        }

                        ui.add_space(12.0);

                        // 美化的发送按钮
                        let send_btn = ui.add_sized(
                            [90.0, 36.0],
                            egui::Button::new(
                                egui::RichText::new(format!("{} 发送", regular::PAPER_PLANE_TILT))
                                    .size(14.0)
                                    .color(egui::Color32::WHITE)
                            )
                            .fill(egui::Color32::from_rgb(0, 150, 136))
                            .corner_radius(egui::CornerRadius::same(8))
                        );
                        
                        if send_btn.clicked() {
                            self.execute_command();
                        }

                        ui.add_space(16.0);
                    }
                );
            });

        // 主终端输出区域 - 美化后的样式
        egui::CentralPanel::default().show_inside(ui, |ui| {
            // 添加输出区域背景
            ui.painter().rect_filled(
                ui.available_rect_before_wrap(),
                egui::CornerRadius::same(6),
                egui::Color32::from_rgb(16, 18, 21),
            );

            // 添加内边距
            egui::Frame::NONE
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(self.scroll_to_bottom)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                for line in &self.output_buffer {
                                    // 添加行号和时间戳效果
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing.x = 0.0;
                                        
                                        // 根据内容类型设置不同颜色（这里直接在下面的if-else中处理）
                                        
                                        // 行号（可选）
                                        if line.starts_with("$ ") {
                                            // 命令行，特殊样式
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(line)
                                                        .font(egui::FontId::monospace(14.0))
                                                        .color(egui::Color32::from_rgb(100, 200, 255))
                                                        .strong()
                                                )
                                                .wrap(),
                                            );
                                        } else if line.contains("错误") || line.contains("失败") || line.contains("Error") {
                                            // 错误信息，红色
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(line)
                                                        .font(egui::FontId::monospace(13.0))
                                                        .color(egui::Color32::from_rgb(255, 100, 100))
                                                )
                                                .wrap(),
                                            );
                                        } else if line.contains("连接") || line.contains("成功") {
                                            // 成功信息，绿色
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(line)
                                                        .font(egui::FontId::monospace(13.0))
                                                        .color(egui::Color32::from_rgb(100, 255, 100))
                                                )
                                                .wrap(),
                                            );
                                        } else if line.contains("正在") || line.contains("...") {
                                            // 进度信息，黄色
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(line)
                                                        .font(egui::FontId::monospace(13.0))
                                                        .color(egui::Color32::from_rgb(255, 200, 100))
                                                )
                                                .wrap(),
                                            );
                                        } else {
                                            // 普通输出，灰白色
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(line)
                                                        .font(egui::FontId::monospace(13.0))
                                                        .color(egui::Color32::from_rgb(220, 220, 220))
                                                )
                                                .wrap(),
                                            );
                                        }
                                    });
                                }
                                
                                // 如果没有输出，显示欢迎信息
                                if self.output_buffer.is_empty() {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(50.0);
                                        ui.label(
                                            egui::RichText::new("✨ 终端已准备就绪")
                                                .font(egui::FontId::proportional(16.0))
                                                .color(egui::Color32::from_rgb(150, 150, 150))
                                        );
                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new("请在下方输入命令或点击重连选择新的连接")
                                                .font(egui::FontId::proportional(12.0))
                                                .color(egui::Color32::from_rgb(120, 120, 120))
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
                    // 处理初始shell输出（欢迎信息和提示符）
                    if let Ok(output) = result.output {
                        // 直接添加原始输出，不做任何修改
                        self.add_output(output);
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
                    // 普通命令处理
                    // 注意：命令已在execute_command中显示，这里只显示结果
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

            if self.is_connected && self.ssh_manager.is_some() && self.tab_id.is_some() {
                // 使用真正的SSH连接执行命令
                let ssh_manager = self.ssh_manager.clone().unwrap();
                let tab_id = self.tab_id.clone().unwrap();
                let cmd = command.trim().to_string();
                let sender = self.command_sender.clone();

                // 在后台执行SSH命令
                tokio::spawn(async move {
                    let result = match ssh_manager
                        .lock()
                        .await
                        .execute_command(&tab_id, &cmd)
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
            self.tab_id = None;  // 清除tab_id，回到快速连接界面
            self.ssh_manager = None;    // 清除SSH管理器
            self.add_output("连接已断开".to_string());
        }
    }
}
