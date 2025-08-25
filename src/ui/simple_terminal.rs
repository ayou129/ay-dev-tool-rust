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
        let mut special_key_to_send: Option<String> = None;
        
        // 🎯 关键修复：先复制所有需要的数据，避免借用冲突
        let lines: Vec<_> = self.output_buffer.iter().cloned().collect();
        let current_prompt = self.current_prompt.clone();
        let is_connected = self.is_connected;
        
        // 🔑 关键改进：获取VT100解析器的光标位置信息
        let cursor_position = self.terminal_emulator.cursor_position();
        // 🔑 重要修复：VT100坐标与数组索引的对应关系
        // VT100行1列12 -> 应该对应数组索引[1][11]（即第1行第12个字符）
        let cursor_row = cursor_position.0.saturating_sub(1) as usize; // VT100行号从1开始，数组从0开始
        let cursor_col = cursor_position.1.saturating_sub(1) as usize; // VT100列号从1开始，数组从0开始
        
        // crate::app_log!(debug, "UI", "📍 VT100光标原始位置: 行{}，列{} -> 数组索引: 行{}，列{}", 
            // cursor_position.0, cursor_position.1, cursor_row, cursor_col);
        
        // 🔍 调试信息：打印终端内容情况
        // crate::app_log!(debug, "UI", "📊 终端内容总行数: {}", lines.len());
        for (i, line) in lines.iter().enumerate() {
            if !line.is_empty() {
                let line_text = line.text();
                // crate::app_log!(debug, "UI", "📝 第{}行: '{}'", i, line_text.chars().take(50).collect::<String>());
            }
        }
        
        // 找到最后一行非空内容
        let mut last_non_empty_index = None;
        for (index, line) in lines.iter().enumerate().rev() {
            if !line.is_empty() {
                last_non_empty_index = Some(index);
                break;
            }
        }
        
        // 🎯 检测是否在全屏应用模式（如vim）
        let in_fullscreen_app = self.is_in_fullscreen_app(&lines);
        // crate::app_log!(debug, "UI", "🔍 全屏应用检测结果: {}", in_fullscreen_app);
        
        egui::ScrollArea::vertical()
            .max_height(available_height)
            .auto_shrink([false, false])
            .stick_to_bottom(self.scroll_to_bottom)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                
                // 🔑 关键修复：使用语义化的输入框显示逻辑
                // 不完全依赖VT100报告的光标位置，而是基于终端内容的语义来判断
                for (index, line) in lines.iter().enumerate() {
                    let line_text = line.text();
                    
                    // 检查是否是包含提示符的行（语义判断）
                    let is_prompt_line = line_text.contains("➜") || 
                                        line_text.contains("$") || 
                                        line_text.contains("#") ||
                                        line_text.starts_with("(") && line_text.contains(")") && line_text.contains("~");
                    
                    // 如果是提示符行且是最后一个提示符行，显示输入框
                    let should_show_input = is_prompt_line && is_connected && !in_fullscreen_app && {
                        // 检查是否是最后一个提示符行
                        let mut is_last_prompt = true;
                        for (later_index, later_line) in lines.iter().enumerate().skip(index + 1) {
                            let later_text = later_line.text();
                            if later_text.contains("➜") || later_text.contains("$") || later_text.contains("#") {
                                is_last_prompt = false;
                                break;
                            }
                        }
                        is_last_prompt
                    };
                    
                    if should_show_input {
                        // crate::app_log!(debug, "UI", "📝 在提示符行({}): '{}' 显示输入框", index, line_text.chars().take(30).collect::<String>());
                        let (exec_cmd, special_key) = Self::render_line_with_input_static_enhanced(ui, line, &mut self.input_buffer);
                        should_execute_command = exec_cmd;
                        special_key_to_send = special_key;
                    } else {
                        // 普通行：只显示内容
                        Self::render_terminal_line_static(ui, line);
                        
                        if is_prompt_line {
                            // crate::app_log!(debug, "UI", "ℹ️ 提示符行({})但不是最后一个: '{}'", index, line_text.chars().take(30).collect::<String>());
                        }
                    }
                }
                
                // 如果没有任何提示符行，显示单独输入行（备用）
                if lines.is_empty() && is_connected && !in_fullscreen_app {
                    // crate::app_log!(info, "UI", "📝 无终端内容，显示单独输入行");
                    let (exec_cmd, special_key) = Self::render_integrated_input_line_static_enhanced(ui, &current_prompt, &mut self.input_buffer);
                    should_execute_command = exec_cmd;
                    special_key_to_send = special_key;
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
        
        // 🎯 关键新增：处理特殊按键发送（统一通道）
        if let Some(special_key) = special_key_to_send {
            self.send_special_key(&special_key);
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
    
    /// 🎯 渲染带输入框的行（增强版 - 支持特殊按键处理和实时字符发送）
    fn render_line_with_input_static_enhanced(ui: &mut egui::Ui, line: &TerminalLine, input_buffer: &mut String) -> (bool, Option<String>) {
        let mut should_execute = false;
        let mut special_key_to_send = None;
        
        // 📝 记录输入前的内容，用于检测变化
        let previous_content = input_buffer.clone();
        
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
                // 🔑 特殊按键检测（优先级最高）
                special_key_to_send = Self::detect_special_keys(ui);
                
                // 回车键检测
                let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                if enter_pressed {
                    should_execute = true;
                    crate::app_log!(info, "UI", "🚀 检测到回车键按下！");
                }
            }
            
            // 方法2：检测文本变化中的回车（备用）
            if response.changed() && input_buffer.ends_with('\n') {
                input_buffer.pop(); // 移除换行符
                should_execute = true;
            }
        });
        
        // 🔑 核心新增：检测输入内容变化，实时发送新字符
        if previous_content != *input_buffer && special_key_to_send.is_none() {
            // 找出新增的字符
            if input_buffer.len() > previous_content.len() {
                let new_chars = &input_buffer[previous_content.len()..];
                crate::app_log!(debug, "UI", "🔤 检测到新输入字符: {:?}", new_chars);
                
                // 实时发送新字符（作为特殊键处理）
                special_key_to_send = Some(new_chars.to_string());
                
                // 🔑 关键修复：实时发送后，清空输入缓冲区，避免重复发送
                // SSH服务器会回显字符，我们不需要在本地保存
                input_buffer.clear();
                crate::app_log!(debug, "UI", "🧹 实时发送后清空输入缓冲区");
                
            } else if input_buffer.len() < previous_content.len() {
                // 检测到删除操作（Backspace）
                let deleted_count = previous_content.len() - input_buffer.len();
                crate::app_log!(debug, "UI", "⬅️ 检测到删除操作: {} 个字符", deleted_count);
                
                // 发送对应数量的退格键
                let backspace_chars = "\x08".repeat(deleted_count);
                special_key_to_send = Some(backspace_chars);
            }
        }
        
        (should_execute, special_key_to_send)
    }
    
    /// 🎯 渲染内嵌式输入行（增强版 - 支持特殊按键处理和实时字符发送）
    fn render_integrated_input_line_static_enhanced(ui: &mut egui::Ui, current_prompt: &str, input_buffer: &mut String) -> (bool, Option<String>) {
        crate::app_log!(info, "UI", "📝 render_integrated_input_line_static_enhanced() 被调用");
        
        let mut should_execute = false;
        let mut special_key_to_send = None;
        
        // 📝 记录输入前的内容，用于检测变化
        let previous_content = input_buffer.clone();
        
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
                // 🔑 特殊按键检测（优先级最高）
                special_key_to_send = Self::detect_special_keys(ui);
                
                // 回车键检测
                let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                if enter_pressed {
                    should_execute = true;
                    crate::app_log!(debug, "UI", "🔑 检测到回车键按下（集成输入行）");
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
        
        // 🔑 核心新增：检测输入内容变化，实时发送新字符
        if previous_content != *input_buffer && special_key_to_send.is_none() {
            // 找出新增的字符
            if input_buffer.len() > previous_content.len() {
                let new_chars = &input_buffer[previous_content.len()..];
                crate::app_log!(debug, "UI", "🔤 检测到新输入字符: {:?}", new_chars);
                
                // 实时发送新字符（作为特殊键处理）
                special_key_to_send = Some(new_chars.to_string());
                
                // 🔑 关键修复：实时发送后，清空输入缓冲区，避免重复发送
                // SSH服务器会回显字符，我们不需要在本地保存
                input_buffer.clear();
                crate::app_log!(debug, "UI", "🧹 实时发送后清空输入缓冲区");
                
            } else if input_buffer.len() < previous_content.len() {
                // 检测到删除操作（Backspace）
                let deleted_count = previous_content.len() - input_buffer.len();
                crate::app_log!(debug, "UI", "⬅️ 检测到删除操作: {} 个字符", deleted_count);
                
                // 发送对应数量的退格键
                let backspace_chars = "\x08".repeat(deleted_count);
                special_key_to_send = Some(backspace_chars);
            }
        }
        
        (should_execute, special_key_to_send)
    }

    /// 🔑 新增：特殊按键检测方法
    fn detect_special_keys(ui: &mut egui::Ui) -> Option<String> {
        ui.input(|i| {
            // Tab 键 - 自动补全
            if i.key_pressed(egui::Key::Tab) {
                crate::app_log!(debug, "UI", "🎯 检测到Tab键");
                return Some("\t".to_string());
            }
            
            // 方向键 - 光标移动和历史记录
            if i.key_pressed(egui::Key::ArrowUp) {
                crate::app_log!(debug, "UI", "⬆️ 检测到上箭头键");
                return Some("\x1b[A".to_string()); // ANSI 上箭头序列
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                crate::app_log!(debug, "UI", "⬇️ 检测到下箭头键");
                return Some("\x1b[B".to_string()); // ANSI 下箭头序列
            }
            if i.key_pressed(egui::Key::ArrowLeft) {
                crate::app_log!(debug, "UI", "⬅️ 检测到左箭头键");
                return Some("\x1b[D".to_string()); // ANSI 左箭头序列
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                crate::app_log!(debug, "UI", "➡️ 检测到右箭头键");
                return Some("\x1b[C".to_string()); // ANSI 右箭头序列
            }
            
            // Home/End 键
            if i.key_pressed(egui::Key::Home) {
                crate::app_log!(debug, "UI", "🏠 检测到Home键");
                return Some("\x1b[H".to_string()); // ANSI Home序列
            }
            if i.key_pressed(egui::Key::End) {
                crate::app_log!(debug, "UI", "🏁 检测到End键");
                return Some("\x1b[F".to_string()); // ANSI End序列
            }
            
            // Page Up/Down 键
            if i.key_pressed(egui::Key::PageUp) {
                crate::app_log!(debug, "UI", "🔼 检测到PageUp键");
                return Some("\x1b[5~".to_string()); // ANSI PageUp序列
            }
            if i.key_pressed(egui::Key::PageDown) {
                crate::app_log!(debug, "UI", "🔽 检测到PageDown键");
                return Some("\x1b[6~".to_string()); // ANSI PageDown序列
            }
            
            // Delete/Backspace 键
            if i.key_pressed(egui::Key::Delete) {
                crate::app_log!(debug, "UI", "🗑️ 检测到Delete键");
                return Some("\x1b[3~".to_string()); // ANSI Delete序列
            }
            
            // Ctrl 组合键
            if i.modifiers.ctrl {
                if i.key_pressed(egui::Key::C) {
                    crate::app_log!(debug, "UI", "⚠️ 检测到Ctrl+C");
                    return Some("\x03".to_string()); // Ctrl+C 中断信号
                }
                if i.key_pressed(egui::Key::D) {
                    crate::app_log!(debug, "UI", "📝 检测到Ctrl+D");
                    return Some("\x04".to_string()); // Ctrl+D EOF信号
                }
                if i.key_pressed(egui::Key::Z) {
                    crate::app_log!(debug, "UI", "⏸️ 检测到Ctrl+Z");
                    return Some("\x1a".to_string()); // Ctrl+Z 暂停信号
                }
            }
            
            None
        })
    }
    fn execute_command(&mut self) {
        crate::app_log!(debug, "UI", "🎯 execute_command 被调用，输入缓冲区内容: '{}'", self.input_buffer);
        
        // 🔑 关键变化：实时字符发送模式下，输入缓冲区可能为空
        // 因为所有字符都已经实时发送了，这里只需要发送回车符
        
        if self.is_connected {
            if let (Some(ssh_manager), Some(tab_id)) = (&mut self.ssh_manager, &self.tab_id) {
                // 🔑 方案1：如果输入缓冲区不为空，说明是旧模式，发送完整命令
                if !self.input_buffer.trim().is_empty() {
                    let command = self.input_buffer.clone();
                    self.input_buffer.clear();
                    
                    crate::app_log!(info, "UI", "📝 旧模式：发送完整命令: '{}'", command.trim());
                    
                    if command.trim() == "clear" {
                        self.output_buffer.clear();
                        crate::app_log!(info, "UI", "🧹 执行本地clear命令");
                        return;
                    }
                    
                    match ssh_manager.execute_command(tab_id, command.trim()) {
                        Ok(_) => {
                            crate::app_log!(info, "UI", "✅ 命令发送成功: {}", command.trim());
                        }
                        Err(e) => {
                            crate::app_log!(error, "UI", "❌ 命令发送失败: {}", e);
                            self.insert_text(format!("命令执行失败: {}", e));
                        }
                    }
                } else {
                    // 🔑 方案2：输入缓冲区为空，说明是实时模式，只发送回车符
                    crate::app_log!(info, "UI", "🔄 实时模式：发送回车符");
                    
                    match ssh_manager.send_raw(tab_id, "\n") {
                        Ok(_) => {
                            crate::app_log!(info, "UI", "✅ 回车符发送成功");
                        }
                        Err(e) => {
                            crate::app_log!(error, "UI", "❌ 回车符发送失败: {}", e);
                        }
                    }
                }
            } else {
                self.insert_text("错误: SSH连接不存在".to_string());
            }
        } else {
            crate::app_log!(error, "UI", "❌ 连接状态: 未连接");
            self.insert_text("错误: 未连接到远程主机".to_string());
        }
        
        // 清空输入缓冲区（防止遭留）
        self.input_buffer.clear();
        self.scroll_to_bottom = true;
    }
    
    /// 🎯 新增：发送特殊按键序列（统一的特殊按键处理通道）
    fn send_special_key(&mut self, key_sequence: &str) {
        if !self.is_connected {
            crate::app_log!(warn, "UI", "⚠️ 未连接，无法发送特殊按键");
            return;
        }
        
        if let (Some(ssh_manager), Some(tab_id)) = (&mut self.ssh_manager, &self.tab_id) {
            match ssh_manager.send_raw(tab_id, key_sequence) {
                Ok(_) => {
                    // 根据不同的按键类型记录不同的日志
                    match key_sequence {
                        "\t" => crate::app_log!(debug, "UI", "🎯 Tab补全发送成功"),
                        "\x1b[A" => crate::app_log!(debug, "UI", "⬆️ 上箭头发送成功"),
                        "\x1b[B" => crate::app_log!(debug, "UI", "⬇️ 下箭头发送成功"),
                        "\x1b[C" => crate::app_log!(debug, "UI", "➡️ 右箭头发送成功"),
                        "\x1b[D" => crate::app_log!(debug, "UI", "⬅️ 左箭头发送成功"),
                        "\x1b[H" => crate::app_log!(debug, "UI", "🏠 Home键发送成功"),
                        "\x1b[F" => crate::app_log!(debug, "UI", "🏁 End键发送成功"),
                        "\x1b[5~" => crate::app_log!(debug, "UI", "🔼 PageUp发送成功"),
                        "\x1b[6~" => crate::app_log!(debug, "UI", "🔽 PageDown发送成功"),
                        "\x1b[3~" => crate::app_log!(debug, "UI", "🗑️ Delete发送成功"),
                        "\x03" => crate::app_log!(debug, "UI", "⚠️ Ctrl+C中断信号发送成功"),
                        "\x04" => crate::app_log!(debug, "UI", "📝 Ctrl+D EOF信号发送成功"),
                        "\x1a" => crate::app_log!(debug, "UI", "⏸️ Ctrl+Z暂停信号发送成功"),
                        s if s.starts_with("\x08") => {
                            let count = s.len();
                            crate::app_log!(debug, "UI", "⬅️ 退格键发送成功: {} 个", count);
                        },
                        s if s.chars().all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace()) => {
                            // 普通字符（实时输入）
                            crate::app_log!(debug, "UI", "🔤 实时字符发送成功: {:?}", s);
                        },
                        _ => crate::app_log!(debug, "UI", "🔑 特殊按键发送成功: {:?}", key_sequence),
                    }
                }
                Err(e) => {
                    crate::app_log!(error, "UI", "❌ 特殊按键发送失败: {:?}, 错误: {}", key_sequence, e);
                }
            }
        } else {
            crate::app_log!(error, "UI", "❌ SSH管理器或Tab ID不存在，无法发送特殊按键");
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
        // 🔍 打印SSH返回的原文
        crate::app_log!(info, "SSH_RAW", "📥 SSH原文: {:?}", data);
        
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
    
    /// 检测是否在全屏应用模式（如vim、nano等）
    fn is_in_fullscreen_app(&self, lines: &[TerminalLine]) -> bool {
        // 🔑 改进的检测逻辑：基于终端内容的特征来判断
        
        // 如果行数很少（≤3行），通常不是全屏应用
        if lines.len() <= 3 {
            return false;
        }
        
        // 检查是否有明显的全屏应用特征
        for line in lines {
            let text = line.text();
            
            // 常见的全屏应用特征
            if text.contains("~") && text.contains("VIM") { // vim界面
                return true;
            }
            if text.contains("GNU nano") { // nano编辑器
                return true;
            }
            if text.contains("File:") && text.contains("Modified") { // 编辑器状态
                return true;
            }
            
            // 如果有明显的终端提示符，说明不是全屏应用
            if text.contains("➜") || text.contains("$") || text.contains("#") {
                return false;
            }
        }
        
        // 检查最后一行是否像提示符
        if let Some(last_line) = lines.last() {
            let last_text = last_line.text();
            // 如果最后一行包含提示符特征，不是全屏应用
            if last_text.contains("➜") || 
               last_text.contains("$") || 
               last_text.contains("#") ||
               last_text.starts_with('(') { // conda环境等
                return false;
            }
        }
        
        // 默认不是全屏应用
        false
    }
}
