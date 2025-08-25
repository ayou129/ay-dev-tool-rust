use crate::ssh::ssh2_client::Ssh2Manager;
use crate::ui::terminal::{TerminalEmulator, TerminalLine};
use crate::ui::ConnectionConfig;

use eframe::egui;

use std::collections::VecDeque;
use std::sync::Arc;

/// 真正简单的终端面板 - 直接读取SSH输出
pub struct SimpleTerminalPanel {
    pub title: String,
    pub connection_info: String,
    pub output_buffer: VecDeque<TerminalLine>,
    input_buffer: String,
    scroll_to_bottom: bool,
    pub is_connected: bool,
    ssh_manager: Option<Arc<Ssh2Manager>>,
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

    /// 设置SSH管理器并启动直接通信
    pub fn set_ssh_manager(&mut self, ssh_manager: Arc<Ssh2Manager>, tab_id: String) {
        self.ssh_manager = Some(ssh_manager.clone());
        self.tab_id = Some(tab_id.clone());
        crate::app_log!(info, "UI", "设置SSH2管理器: {:?}", self.tab_id);
        
        // 🔑 关键改进：直接从SSH2Manager读取，不创建额外的后台任务
        // SSH2ConnectionWrapper内部已经有独立的读取线程了
        crate::app_log!(info, "UI", "SSH2管理器设置完成，将直接读取SSH输出");
    }
    
    /// 设置SSH管理器和连接
    pub fn connect(&mut self, tab_id: String, config: &ConnectionConfig) -> anyhow::Result<()> {
        crate::app_log!(info, "UI", "开始连接SSH2: {}", tab_id);
        
        let mut ssh_manager = Ssh2Manager::new();
        ssh_manager.create_connection(tab_id.clone(), config)?;
        
        self.ssh_manager = Some(Arc::new(ssh_manager));
        self.tab_id = Some(tab_id);
        self.is_connected = true;
        self.connection_info = format!("{}@{}:{}", config.username, config.host, config.port);
        
        self.insert_text("✅ SSH2连接成功".to_string());
        crate::app_log!(info, "UI", "SSH2连接建立成功");
        
        Ok(())
    }

    /// 断开连接
    pub fn disconnect(&mut self) {
        if let (Some(_ssh_manager), Some(tab_id)) = (&mut self.ssh_manager, &self.tab_id) {
            crate::app_log!(info, "UI", "请求断开SSH连接: {}", tab_id);
        }
        
        self.ssh_manager = None;
        self.tab_id = None;
        self.is_connected = false;
        self.connection_info = "未连接".to_string();
        self.insert_text("连接已断开".to_string());
    }

    /// 🔑 核心方法：简单的UI渲染测试版本
    pub fn show(&mut self, ui: &mut egui::Ui) {
        
        // 🔑 恢复到单次调用，看看是否还有重复
        self.receive_ssh_output();
        
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

            // 🎯 输入已集成到终端内容中，不再需要单独的输入区域
        });
    }
    
    /// 🔑 批量读取SSH输出，避免重复处理
    fn receive_ssh_output(&mut self) {
        if let (Some(ssh_manager), Some(tab_id)) = (&self.ssh_manager, &self.tab_id) {
            // 🔑 关键改进：批量读取所有可用数据，避免分帧处理导致重复
            let mut all_data = String::new();
            let mut read_count = 0;
            
            // 一次性读取所有可用数据
            loop {
                match ssh_manager.read_output(tab_id) {
                    Ok(data) if !data.is_empty() => {
                        all_data.push_str(&data);
                        read_count += 1;
                        
                        // 防止无限循环，最多读取10次
                        if read_count >= 10 {
                            break;
                        }
                    }
                    Ok(_) => {
                        // 没有更多数据，退出循环
                        break;
                    }
                    Err(e) => {
                        if !e.to_string().contains("连接不存在") {
                            crate::app_log!(debug, "UI", "SSH读取错误: {}", e);
                        }
                        break;
                    }
                }
            }
            
            // 只有当确实有数据时才处理
            if !all_data.is_empty() {
                crate::app_log!(debug, "UI", "📦 批量读取SSH输出: {} 字节 ({} 次读取)", all_data.len(), read_count);
                
                // 🔑 关键：检测是否为初始连接输出
                if !self.has_ssh_initial_output {
                    self.has_ssh_initial_output = true;
                    crate::app_log!(info, "UI", "🎉 收到SSH初始连接输出");
                }
                
                // 🔑 关键：在显示到UI之前，先记录到日志
                if all_data.contains("连接已断开") {
                    crate::app_log!(error, "UI", "🚨 SSH2连接断开，可能是认证失败");
                    self.is_connected = false;
                    self.connection_info = "连接已断开（可能是认证失败）".to_string();
                }
                
                // 📢 关键：一次性处理所有数据，避免重复处理
                self.process_ssh_data(all_data);
            }
        }
    }

    /// 渲染终端输出 + 内嵌式输入（完全重写版）
    fn render_terminal_output(&mut self, ui: &mut egui::Ui) {
        let available_height = ui.available_height();
        let mut should_execute_command = false;
        let mut should_send_tab = false;
        
        // 🎯 关键修复：先复制所有需要的数据，避免借用冲突
        let lines: Vec<_> = self.output_buffer.iter().cloned().collect();
        let current_prompt = self.current_prompt.clone();
        let is_connected = self.is_connected;
        
        // 找到最后一行非空内容
        let mut last_non_empty_index = None;
        for (index, line) in lines.iter().enumerate().rev() {
            if !line.is_empty() {
                last_non_empty_index = Some(index);
                break;
            }
        }
        
        egui::ScrollArea::vertical()
            .max_height(available_height)
            .auto_shrink([false, false])
            .stick_to_bottom(self.scroll_to_bottom)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                
                // 渲染所有终端内容
                for (index, line) in lines.iter().enumerate() {
                    if Some(index) == last_non_empty_index && is_connected {
                        // 最后一行非空内容：显示内容 + 输入框
                        let (exec_cmd, send_tab) = Self::render_line_with_input_static_enhanced(ui, line, &mut self.input_buffer);
                        should_execute_command = exec_cmd;
                        should_send_tab = send_tab;
                    } else {
                        // 普通行：只显示内容
                        Self::render_terminal_line_static(ui, line);
                    }
                }
                
                // 如果没有任何非空内容，显示单独输入行
                if last_non_empty_index.is_none() && is_connected {
                    crate::app_log!(info, "UI", "📝 显示单独输入行");
                    let (exec_cmd, send_tab) = Self::render_integrated_input_line_static_enhanced(ui, &current_prompt, &mut self.input_buffer);
                    should_execute_command = exec_cmd;
                    should_send_tab = send_tab;
                }
            });

        if self.scroll_to_bottom {
            self.scroll_to_bottom = false;
        }
        
        // 处理命令执行
        if should_execute_command {
            crate::app_log!(info, "UI", "🚀 检测到回车键，准备执行命令");
            self.execute_command();
        }
        
        // 🎯 关键新增：处理Tab键自动补全
        if should_send_tab {
            self.send_tab_completion();
        }
    }

    /// 渲染单行终端内容（静态版本）
    fn render_terminal_line_static(ui: &mut egui::Ui, line: &TerminalLine) {
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
    
    /// 🎯 渲染带输入框的行（增强版 - 支持Tab补全）
    fn render_line_with_input_static_enhanced(ui: &mut egui::Ui, line: &TerminalLine, input_buffer: &mut String) -> (bool, bool) {
        let mut should_execute = false;
        let mut should_send_tab = false;
        
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            
            // 先渲染行内容
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
            
            // 在同一行后面添加输入框
            let response = ui.add(
                egui::TextEdit::singleline(input_buffer)
                    .font(egui::FontId::monospace(14.0))
                    .desired_width(ui.available_width())
                    .frame(false)
            );
            
            // 🎯 关键修复：使用更可靠的按键检测方式
            // 强制获取焦点
            response.request_focus();
            
            if response.has_focus() {
                // 方法1：检测回车键按下（优先）
                let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let tab_pressed = ui.input(|i| i.key_pressed(egui::Key::Tab));
                
                if enter_pressed {
                    should_execute = true;
                    crate::app_log!(info, "UI", "🚀 检测到回车键按下！");
                } else if tab_pressed {
                    should_send_tab = true;
                }
            }
            
            // 方法2：检测文本变化中的回车（备用）
            if response.changed() && input_buffer.ends_with('\n') {
                input_buffer.pop(); // 移除换行符
                should_execute = true;
            }
        });
        
        (should_execute, should_send_tab)
    }
    
    /// 🎯 渲染内嵌式输入行（增强版 - 支持Tab补全）
    fn render_integrated_input_line_static_enhanced(ui: &mut egui::Ui, current_prompt: &str, input_buffer: &mut String) -> (bool, bool) {
        crate::app_log!(info, "UI", "📝 render_integrated_input_line_static_enhanced() 被调用");
        
        let mut should_execute = false;
        let mut should_send_tab = false;
        
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            
            ui.add(egui::Label::new(
                egui::RichText::new(current_prompt)
                    .font(egui::FontId::monospace(14.0))
                    .color(egui::Color32::BLUE)
            ));
            
            ui.add(egui::Label::new(
                egui::RichText::new(" ")
                    .font(egui::FontId::monospace(14.0))
            ));
            
            let response = ui.add(
                egui::TextEdit::singleline(input_buffer)
                    .font(egui::FontId::monospace(14.0))
                    .desired_width(ui.available_width())
                    .frame(false)
            );
            
            // 🎯 关键修复：使用更可靠的按键检测方式
            if response.has_focus() {
                // 方法1：检测回车键按下（优先）
                let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let tab_pressed = ui.input(|i| i.key_pressed(egui::Key::Tab));
                
                if enter_pressed {
                    should_execute = true;
                    crate::app_log!(debug, "UI", "🔑 检测到回车键按下（集成输入行）");
                } else if tab_pressed {
                    should_send_tab = true;
                    crate::app_log!(debug, "UI", "🔑 检测到Tab键按下（集成输入行）");
                }
            }
            
            // 方法2：检测文本变化中的回车（备用）
            if response.changed() && input_buffer.ends_with('\n') {
                input_buffer.pop(); // 移除换行符
                should_execute = true;
                crate::app_log!(debug, "UI", "🔑 通过文本变化检测到回车（集成输入行）");
            }
            
            // 自动获取焦点
            if !response.has_focus() {
                response.request_focus();
            }
        });
        
        (should_execute, should_send_tab)
    }

    /// 🔑 真正简单的命令执行（同步，无回调）
    fn execute_command(&mut self) {
        crate::app_log!(debug, "UI", "🎯 execute_command 被调用，输入缓冲区内容: '{}'", self.input_buffer);
        
        if !self.input_buffer.trim().is_empty() {
            let command = self.input_buffer.clone();
            self.input_buffer.clear();
            
            crate::app_log!(info, "UI", "📝 准备执行命令: '{}'", command.trim());

            if command.trim() == "clear" {
                self.output_buffer.clear();
                crate::app_log!(info, "UI", "🧹 执行本地clear命令");
                return;
            }

            // 🔑 关键修改：移除手动插入命令显示，SSH终端会自动回显
            // 之前的代码：self.insert_text(format!("{} {}", self.current_prompt, command));
            // 现在直接发送命令，让SSH服务器处理回显

            if self.is_connected {
                crate::app_log!(debug, "UI", "🔗 连接状态: 已连接");
                if let (Some(ssh_manager), Some(tab_id)) = (&mut self.ssh_manager, &self.tab_id) {
                    crate::app_log!(debug, "UI", "📡 SSH管理器和Tab ID都存在，准备发送命令");
                    // 🔑 关键：直接同步发送命令，无异步回调
                    match ssh_manager.execute_command(tab_id, command.trim()) {
                        Ok(_) => {
                            crate::app_log!(info, "UI", "✅ 命令发送成功: {}", command.trim());
                            // 输出会在下一帧的read_ssh_output_sync中读取
                        }
                        Err(e) => {
                            crate::app_log!(error, "UI", "❌ 命令发送失败: {}", e);
                            self.insert_text(format!("命令执行失败: {}", e));
                        }
                    }
                } else {
                    self.insert_text("错误: SSH连接不存在".to_string());
                }
            } else {
                crate::app_log!(error, "UI", "❌ 连接状态: 未连接");
                self.insert_text("错误: 未连接到远程主机".to_string());
            }

            self.scroll_to_bottom = true;
        } else {
            crate::app_log!(debug, "UI", "🚫 输入缓冲区为空，不执行任何操作");
        }
    }
    
    /// 🎯 新增：发送Tab键进行自动补全
    fn send_tab_completion(&mut self) {
        if !self.input_buffer.is_empty() && self.is_connected {
            if let (Some(ssh_manager), Some(tab_id)) = (&mut self.ssh_manager, &self.tab_id) {
                // 🔑 关键：直接使用execute_command发送包含Tab字符的内容
                // 这样可以利用现有的架构，无需添加新接口
                let completion_input = format!("{}	", self.input_buffer);
                match ssh_manager.execute_command(tab_id, &completion_input) {
                    Ok(_) => {
                        crate::app_log!(debug, "UI", "🎯 Tab补全发送成功: '{}'", self.input_buffer);
                        // 注意：不清空输入缓冲区，让用户继续编辑
                        // 远程终端会返回补全结果，用户可以看到后再决定
                    }
                    Err(e) => {
                        crate::app_log!(error, "UI", "🎯 Tab补全发送失败: {}", e);
                    }
                }
            }
        }
    }

    /// 🔑 核心方法：终端内容插入（唯一插入接口）
    fn insert_line(&mut self, line: TerminalLine) {
        self.output_buffer.push_back(line);
        
        // 限制缓冲区大小
        while self.output_buffer.len() > 1000 {
            self.output_buffer.pop_front();
        }
        
        self.scroll_to_bottom = true;
    }
    
    /// 手动插入文本（不经过VT100）
    fn insert_text(&mut self, text: String) {
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
        
        self.insert_line(line);
    }

    /// SSH数据处理入口：VT100解析 + 屏幕状态更新（修复版）
    pub fn process_ssh_data(&mut self, data: String) {
        // 🔑 关键：VT100解析在这里完成
        let result = self.terminal_emulator.process_pty_output(&data);
        
        // 🎯 关键修复：直接使用VT100屏幕状态，不做增量处理
        self.output_buffer.clear();
        for line in result.lines {
            // 🔑 重要：保留所有行，包括空行（VT100屏幕状态是完整的）
            self.output_buffer.push_back(line);
        }
        
        // 更新提示符
        if let Some(prompt) = result.prompt_update {
            if !prompt.trim().is_empty() && !prompt.contains("Last login") {
                self.current_prompt = prompt.trim().to_string();
            }
        }
        
        self.scroll_to_bottom = true;
        crate::app_log!(debug, "UI", "📺 VT100屏幕状态更新完成: {} 行", self.output_buffer.len());
    }
}
