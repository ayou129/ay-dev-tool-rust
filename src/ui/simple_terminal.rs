use crate::ssh::SyncSshManager;
use crate::ui::terminal::{TerminalEmulator, TerminalLine};
use crate::ui::ConnectionConfig;

use eframe::egui;

use std::collections::VecDeque;
use std::sync::Arc;

/// 真正简单的终端面板 - 无channel，无异步，直接操作PTY
pub struct SimpleTerminalPanel {
    pub title: String,
    pub connection_info: String,
    pub output_buffer: VecDeque<TerminalLine>,
    input_buffer: String,
    scroll_to_bottom: bool,
    pub is_connected: bool,
    ssh_manager: Option<Arc<SyncSshManager>>,
    pub tab_id: Option<String>,
    current_prompt: String,
    terminal_emulator: TerminalEmulator,
    has_ssh_initial_output: bool,
}

impl std::fmt::Debug for SimpleTerminalPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimpleTerminalPanel")
            .field("title", &self.title)
            .field("is_connected", &self.is_connected)
            .field("tab_id", &self.tab_id)
            .finish()
    }
}

impl SimpleTerminalPanel {
    pub fn new(title: String, connection_info: String) -> Self {
        Self {
            title,
            connection_info,
            output_buffer: VecDeque::new(),
            input_buffer: String::new(),
            scroll_to_bottom: false,
            is_connected: false,
            ssh_manager: None,
            tab_id: None,
            current_prompt: "❯".to_string(),
            terminal_emulator: TerminalEmulator::new(120, 30),
            has_ssh_initial_output: false,
        }
    }

    /// 设置共享的SSH管理器
    pub fn set_ssh_manager(&mut self, ssh_manager: Arc<SyncSshManager>, tab_id: String) {
        self.ssh_manager = Some(ssh_manager);
        self.tab_id = Some(tab_id);
        crate::app_log!(info, "UI", "设置SSH管理器: {:?}", self.tab_id);
    }

    /// 设置SSH管理器和连接
    pub fn connect(&mut self, tab_id: String, config: &ConnectionConfig) -> anyhow::Result<()> {
        crate::app_log!(info, "UI", "开始连接SSH: {}", tab_id);
        
        let ssh_manager = Arc::new(SyncSshManager::new());
        ssh_manager.create_connection(tab_id.clone(), config)?;
        
        self.ssh_manager = Some(ssh_manager);
        self.tab_id = Some(tab_id);
        self.is_connected = true;
        self.connection_info = format!("{}@{}:{}", config.username, config.host, config.port);
        
        self.add_output("✅ 连接成功".to_string());
        crate::app_log!(info, "UI", "SSH连接建立成功");
        
        Ok(())
    }

    /// 断开连接
    pub fn disconnect(&mut self) {
        if let (Some(ssh_manager), Some(tab_id)) = (&mut self.ssh_manager, &self.tab_id) {
            ssh_manager.disconnect(tab_id);
        }
        
        self.ssh_manager = None;
        self.tab_id = None;
        self.is_connected = false;
        self.connection_info = "未连接".to_string();
        self.add_output("连接已断开".to_string());
    }

    /// 🔑 核心方法：真正简单的UI渲染
    pub fn show(&mut self, ui: &mut egui::Ui) {
        // 🔑 关键：每帧同步读取SSH输出（真正简单的实现）
        self.read_ssh_output_sync();
        
        // 设置终端样式
        ui.style_mut().visuals.panel_fill = egui::Color32::WHITE;
        ui.style_mut().visuals.window_fill = egui::Color32::WHITE;

        ui.vertical(|ui| {
            // 连接信息
            ui.horizontal(|ui| {
                ui.label("连接状态:");
                if self.is_connected {
                    ui.colored_label(egui::Color32::GREEN, &self.connection_info);
                } else {
                    ui.colored_label(egui::Color32::RED, &self.connection_info);
                }
            });

            ui.separator();

            // 终端输出区域
            self.render_terminal_output(ui);

            ui.separator();

            // 输入区域
            self.render_input_area(ui);
        });
    }

    /// 🔑 真正简单的SSH输出读取（同步，无异步）
    fn read_ssh_output_sync(&mut self) {
        if let (Some(ssh_manager), Some(tab_id)) = (&mut self.ssh_manager, &self.tab_id) {
            match ssh_manager.read_output(tab_id) {
                Ok(data) if !data.is_empty() => {
                    crate::app_log!(info, "UI", "📺 同步读取到SSH输出: {} 字节，内容: {:?}", data.len(), data);
                    
                    // 🔑 关键：在显示到UI之前，先记录到日志
                    if data.contains("连接已断开") {
                        crate::app_log!(error, "UI", "🚨 SSH连接断开，可能是认证失败");
                        self.is_connected = false;
                        self.connection_info = "连接已断开（可能是认证失败）".to_string();
                    }
                    
                    // 📺 关键：所有数据都要显示在UI上，无论是成功还是失败信息
                    self.add_pty_output(data);
                }
                Ok(_) => {
                    // 没有数据，正常情况，不记录日志以避免垃圾
                }
                Err(e) => {
                    crate::app_log!(warn, "UI", "SSH输出读取错误: {}", e);
                    // 🔑 错误信息也要显示在UI上
                    self.add_output(format!("错误: {}", e));
                }
            }
        }
    }

    /// 渲染终端输出
    fn render_terminal_output(&mut self, ui: &mut egui::Ui) {
        let available_height = ui.available_height() - 60.0; // 为输入区域留出空间
        
        egui::ScrollArea::vertical()
            .max_height(available_height)
            .auto_shrink([false, false])
            .stick_to_bottom(self.scroll_to_bottom)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                
                for line in &self.output_buffer {
                    self.render_terminal_line(ui, line);
                }
            });

        if self.scroll_to_bottom {
            self.scroll_to_bottom = false;
        }
    }

    /// 渲染单行终端内容
    fn render_terminal_line(&self, ui: &mut egui::Ui, line: &TerminalLine) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            
            for segment in &line.segments {
                if segment.text.is_empty() {
                    continue;
                }
                
                let mut rich_text = egui::RichText::new(&segment.text)
                    .font(egui::FontId::monospace(14.0));
                
                if let Some(color) = segment.color {
                    rich_text = rich_text.color(color);
                } else {
                    rich_text = rich_text.color(egui::Color32::BLACK);
                }
                
                if let Some(bg_color) = segment.background_color {
                    rich_text = rich_text.background_color(bg_color);
                }
                
                ui.add(egui::Label::new(rich_text).selectable(true));
            }
        });
    }

    /// 渲染输入区域
    fn render_input_area(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // 显示提示符
            ui.add(egui::Label::new(
                egui::RichText::new(&self.current_prompt)
                    .font(egui::FontId::monospace(14.0))
                    .color(egui::Color32::BLUE)
            ));

            // 输入框
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.input_buffer)
                    .font(egui::FontId::monospace(14.0))
                    .desired_width(ui.available_width() - 100.0)
            );

            // 发送按钮
            if ui.button("发送").clicked() || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                self.execute_command();
            }
        });
    }

    /// 🔑 真正简单的命令执行（同步，无回调）
    fn execute_command(&mut self) {
        if !self.input_buffer.trim().is_empty() {
            let command = self.input_buffer.clone();
            self.input_buffer.clear();

            if command.trim() == "clear" {
                self.output_buffer.clear();
                return;
            }

            // 显示用户输入
            self.add_output(format!("{} {}", self.current_prompt, command));

            if self.is_connected {
                if let (Some(ssh_manager), Some(tab_id)) = (&mut self.ssh_manager, &self.tab_id) {
                    // 🔑 关键：直接同步发送命令，无异步回调
                    match ssh_manager.execute_command(tab_id, command.trim()) {
                        Ok(_) => {
                            crate::app_log!(info, "UI", "命令发送成功: {}", command.trim());
                            // 输出会在下一帧的read_ssh_output_sync中读取
                        }
                        Err(e) => {
                            self.add_output(format!("命令执行失败: {}", e));
                        }
                    }
                } else {
                    self.add_output("错误: SSH连接不存在".to_string());
                }
            } else {
                self.add_output("错误: 未连接到远程主机".to_string());
            }

            self.scroll_to_bottom = true;
        }
    }

    /// 添加输出到终端
    fn add_output(&mut self, text: String) {
        // 简单创建TerminalLine
        let mut line = TerminalLine::new();
        line.segments.push(crate::ui::terminal::TerminalSegment {
            text,
            color: Some(egui::Color32::BLACK),
            background_color: None,
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
        });
        
        self.output_buffer.push_back(line);
        
        // 限制缓冲区大小
        while self.output_buffer.len() > 1000 {
            self.output_buffer.pop_front();
        }
        
        self.scroll_to_bottom = true;
    }

    /// 添加PTY输出（带VT100解析）
    pub fn add_pty_output(&mut self, data: String) {
        // 使用VT100解析器处理数据
        let result = self.terminal_emulator.process_pty_output(&data);
        
        // 添加处理后的行到输出缓冲区
        for line in result.lines {
            self.output_buffer.push_back(line);
        }
        
        // 更新提示符
        if let Some(prompt) = result.prompt_update {
            self.current_prompt = prompt;
        }
        
        // 限制缓冲区大小
        while self.output_buffer.len() > 1000 {
            self.output_buffer.pop_front();
        }
        
        self.scroll_to_bottom = true;
    }
}
