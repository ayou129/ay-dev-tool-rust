use crate::ssh::SshManager;
use crate::ui::terminal_emulator::{TerminalEmulator, TerminalLine, TerminalSegment};

use eframe::egui;
use egui_phosphor::regular;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

pub struct TerminalPanel {
    pub title: String,
    pub connection_info: String,
    pub output_buffer: VecDeque<TerminalLine>,
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
    terminal_emulator: TerminalEmulator, // 终端模拟器
    has_ssh_initial_output: bool,        // 是否已收到SSH初始输出
    // 内联输入相关状态
    inline_input_active: bool, // 是否激活内联输入模式
    cursor_blink_time: f64,    // 光标闪烁计时器
}

// 手动实现Debug trait
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
            .field("terminal_emulator", &"TerminalEmulator(hidden)") // 隐藏终端模拟器的内部细节
            .field("has_ssh_initial_output", &self.has_ssh_initial_output) // ✅ 添加新字段
            .field("inline_input_active", &self.inline_input_active)
            .field("cursor_blink_time", &self.cursor_blink_time)
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
            terminal_emulator: TerminalEmulator::new(200, 50), // 创建新的终端模拟器
            has_ssh_initial_output: false, // 初始化为未收到SSH输出
            inline_input_active: false,
            cursor_blink_time: 0.0,
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
            terminal_emulator: TerminalEmulator::new(200, 50), // 创建终端模拟器
            has_ssh_initial_output: false,   // 初始化为未收到SSH输出
            inline_input_active: false,
            cursor_blink_time: 0.0,
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

    // ✅ 更新tab标题（基于VT100解析结果）
    pub fn update_title_from_vt100(&mut self, vt100_title: &str) {
        if !vt100_title.is_empty() {
            // 提取用户友好的标题：user@host:path -> host:path
            if let Some(at_pos) = vt100_title.find('@') {
                if vt100_title[at_pos..].find(':').is_some() {
                    let host_path = &vt100_title[at_pos + 1..];
                    self.title = host_path.to_string();
                } else {
                    self.title = vt100_title.to_string();
                }
            } else {
                self.title = vt100_title.to_string();
            }
            crate::app_log!(debug, "SSH", "更新tab标题: {}", self.title);
        }
    }

    pub fn add_output(&mut self, text: String) {
        // ✅ 将文本转换为TerminalLine，正确处理制表符和换行符
        for line_text in text.split('\n') {
            if line_text.is_empty() {
                // 空行
                let mut line = TerminalLine::new();
                let mut segment = TerminalSegment::default();
                segment.text = " ".to_string(); // 空行至少有一个空格
                line.segments.push(segment);
                self.output_buffer.push_back(line);
            } else {
                // 处理制表符对齐
                let processed_text = self.process_tab_alignment(line_text);
                let mut line = TerminalLine::new();
                let mut segment = TerminalSegment::default();
                segment.text = processed_text;
                line.segments.push(segment);
                self.output_buffer.push_back(line);
            }
        }

        // 限制缓冲区大小
        while self.output_buffer.len() > 10000 {
            self.output_buffer.pop_front();
        }

        self.scroll_to_bottom = true;
    }

    /// ✅ 处理制表符对齐 - 将制表符转换为适当数量的空格
    fn process_tab_alignment(&self, text: &str) -> String {
        let mut result = String::new();
        let mut col = 0;

        for ch in text.chars() {
            if ch == '\t' {
                // 制表符：对齐到8的倍数列位置
                let tab_stop = 8;
                let spaces_needed = tab_stop - (col % tab_stop);
                result.push_str(&" ".repeat(spaces_needed));
                col += spaces_needed;
            } else if ch == '\r' {
                // 忽略回车符
                continue;
            } else {
                result.push(ch);
                col += 1;
            }
        }

        result
    }

    pub fn add_terminal_lines(&mut self, lines: Vec<TerminalLine>) {
        for line in lines {
            self.output_buffer.push_back(line);
        }

        // 限制缓冲区大小
        while self.output_buffer.len() > 10000 {
            self.output_buffer.pop_front();
        }

        self.scroll_to_bottom = true;
    }

    // PTY输出处理 - 使用新的PTY架构
    pub fn add_pty_output(&mut self, text: String) {
        if !text.is_empty() {
            // ✅ 打印PTY原文数据
            crate::app_log!(info, "PTY", "PTY原文内容: {:?}", text);

            // 检查是否包含ANSI转义序列
            if text.contains('\x1b') {
                // 使用TerminalEmulator处理PTY输出
                let result = self.terminal_emulator.process_pty_output(&text);

                // 处理提示符更新
                if let Some(new_prompt) = result.prompt_update {
                    self.current_prompt = new_prompt;
                }

                // ✅ 更新tab标题（基于VT100解析的标题）
                let vt100_title = self.terminal_emulator.title().to_string();
                if !vt100_title.is_empty() {
                    self.update_title_from_vt100(&vt100_title);
                }

                // 🔥 修复：直接替换整个output_buffer，而不是追加
                // 这样可以确保显示完整的VT100屏幕内容
                self.output_buffer.clear();
                self.add_terminal_lines(result.lines);

                // 标记已收到初始输出
                self.has_ssh_initial_output = true;
            } else {
                // 纯文本，直接显示
                self.add_output(text);
            }
        }
    }

    // 改进的字符网格方案：增加间距并保持颜色
    /// ✅ 完美字符网格渲染 - 解决对齐问题的最终方案
    fn render_terminal_line_grid_improved(&self, ui: &mut egui::Ui, line: &TerminalLine) {
        if line.is_empty() {
            return;
        }

        // 检查是否为纯文本行（无样式）
        let is_plain_text = line.segments.iter().all(|s| {
            s.color.is_none()
                && s.background_color.is_none()
                && !s.bold
                && !s.italic
                && !s.underline
                && !s.inverse
        });

        if is_plain_text {
            // ✅ 方案A：纯文本整行渲染 - 完美对齐
            let line_text = line.text();
            ui.add(
                egui::Label::new(
                    egui::RichText::new(line_text)
                        .font(egui::FontId::monospace(14.0))
                        .color(egui::Color32::BLACK),
                )
                .selectable(true),
            );
        } else {
            // ✅ 方案B：彩色文本使用无间距水平布局
            ui.horizontal(|ui| {
                // 完全消除间距
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
                ui.spacing_mut().indent = 0.0;

                for segment in &line.segments {
                    if segment.text.is_empty() {
                        continue;
                    }

                    // 创建富文本
                    let mut rich_text =
                        egui::RichText::new(&segment.text).font(egui::FontId::monospace(14.0));

                    // 应用颜色
                    if let Some(color) = segment.color {
                        rich_text = rich_text.color(color);
                    } else {
                        rich_text = rich_text.color(egui::Color32::BLACK);
                    }

                    // 应用背景色
                    if let Some(bg_color) = segment.background_color {
                        rich_text = rich_text.background_color(bg_color);
                    }

                    // 应用文本样式
                    if segment.bold {
                        rich_text = rich_text.strong();
                    }
                    if segment.italic {
                        rich_text = rich_text.italics();
                    }
                    if segment.underline {
                        rich_text = rich_text.underline();
                    }

                    // 处理反显
                    if segment.inverse {
                        rich_text = rich_text
                            .background_color(egui::Color32::BLACK)
                            .color(egui::Color32::WHITE);
                    }

                    // 渲染segment
                    ui.add(egui::Label::new(rich_text).selectable(true));
                }
            });
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // 检查是否有命令结果需要处理
        self.process_command_results();

        // 更新连接信息
        self.update_connection_info();

        // 设置终端样式 - iTerm2 明亮风格（白底黑字）
        let terminal_style = egui::Style {
            visuals: egui::Visuals {
                dark_mode: false,
                panel_fill: egui::Color32::WHITE,
                window_fill: egui::Color32::WHITE,
                override_text_color: None, // ✅ 不覆盖文本颜色，保持VT100颜色
                ..ui.style().visuals.clone()
            },
            spacing: egui::style::Spacing {
                item_spacing: egui::vec2(0.0, 6.0), // ✅ 水平间距设为0，保持对齐
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
                    egui::Color32::from_rgb(245, 245, 245),
                );

                // 底部分隔线
                ui.painter().hline(
                    rect.left()..=rect.right(),
                    rect.bottom(),
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 220, 220)),
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
                            .color(egui::Color32::from_rgb(60, 60, 60)),
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
                                    egui::RichText::new(regular::ERASER.to_string()).size(14.0),
                                )
                                .fill(egui::Color32::from_rgb(240, 240, 240))
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    egui::Color32::from_rgb(200, 200, 200),
                                ))
                                .corner_radius(egui::CornerRadius::same(6)),
                            );

                            if clear_btn.clicked() {
                                self.output_buffer.clear();
                            }

                            ui.add_space(8.0);

                            // 重连按钮 - GitHub风格
                            let reconnect_btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(regular::ARROW_CLOCKWISE.to_string())
                                        .size(14.0),
                                )
                                .fill(egui::Color32::from_rgb(230, 243, 255))
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    egui::Color32::from_rgb(180, 210, 255),
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

        // 输入区域改为内嵌到终端内容区域底部（紧随输出），模拟 iTerm2 体验

        // ✅ 新布局：只有终端输出区域，输入内嵌在最后一行
        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.render_terminal_output_area(ui);
        });
    }

    /// ✅ 渲染终端输出区域
    fn render_terminal_output_area(&mut self, ui: &mut egui::Ui) {
        // 终端背景 - 白底
        let terminal_bg_color = egui::Color32::WHITE;

        let rect = ui.available_rect_before_wrap();
        // 边框
        ui.painter().rect_stroke(
            rect.shrink(0.5),
            egui::CornerRadius::same(4),
            egui::Stroke::new(1.0, egui::Color32::from_rgb(210, 210, 210)),
            egui::StrokeKind::Outside,
        );
        ui.painter().rect_filled(
            rect.shrink(1.0),
            egui::CornerRadius::same(4),
            terminal_bg_color,
        );

        // 右键菜单和点击处理（不再占用布局空间）
        let area_id = ui.id().with("terminal_area");
        let response = ui.interact(rect, area_id, egui::Sense::click());

        response.context_menu(|ui| {
            ui.set_style(std::sync::Arc::new(egui::Style {
                visuals: egui::Visuals {
                    window_fill: egui::Color32::from_rgb(255, 255, 255),
                    panel_fill: egui::Color32::from_rgb(255, 255, 255),
                    override_text_color: Some(egui::Color32::BLACK),
                    ..ui.style().visuals.clone()
                },
                ..ui.style().as_ref().clone()
            }));

            if ui.button("📋 复制全部内容").clicked() {
                self.copy_all_to_clipboard(ui);
                ui.close();
            }

            ui.separator();

            if ui.button("🗑️ 清空终端").clicked() {
                self.output_buffer.clear();
                ui.close();
            }
        });

        // 现代化边距和滚动（轻主题右键菜单样式）- 增加外边距
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(24, 20)) // 增加外边距
            .outer_margin(egui::Margin::symmetric(8, 6)) // 添加外边距
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                            // 新架构：基于TerminalSegment属性渲染
                            // 🔥 修复：渲染所有行，最后一行使用内联输入
                            let len = self.output_buffer.len();
                            
                            if len > 0 {
                                // 渲染前面所有行（除了最后一行）
                                for i in 0..len-1 {
                                    if let Some(terminal_line) = self.output_buffer.get(i) {
                                        self.render_terminal_line_grid_improved(ui, terminal_line);
                                    }
                                }
                                
                                // 克隆最后一行来避免借用冲突
                                if let Some(last_line) = self.output_buffer.get(len-1).cloned() {
                                    self.render_terminal_line_with_inline_input(ui, &last_line);
                                }
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
                                    ui.add_space(12.0);
                                    ui.label(
                                        egui::RichText::new("💡 右键菜单：全选、复制、清空")
                                            .font(egui::FontId::proportional(12.0))
                                            .color(egui::Color32::from_rgb(128, 128, 128)),
                                    );
                                });
                            }
                        });
                    });
            });
    }

    /// ✅ 渲染带有内联输入的终端行（用于最后一行提示符）
    fn render_terminal_line_with_inline_input(&mut self, ui: &mut egui::Ui, line: &TerminalLine) {
        if line.is_empty() && self.input_buffer.is_empty() {
            return;
        }

        // ✅ 关键修复：在同一个horizontal布局中渲染提示符和输入
        ui.horizontal(|ui| {
            // 完全消除间距以保持字符对齐
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
            ui.spacing_mut().indent = 0.0;

            // 首先渲染提示符内容（VT100解析的带颜色内容）
            if !line.is_empty() {
                for segment in &line.segments {
                    if segment.text.is_empty() {
                        continue;
                    }
                    
                    // 创建富文本
                    let mut rich_text = egui::RichText::new(&segment.text)
                        .font(egui::FontId::monospace(14.0));
                    
                    // 应用颜色
                    if let Some(color) = segment.color {
                        rich_text = rich_text.color(color);
                    } else {
                        rich_text = rich_text.color(egui::Color32::BLACK);
                    }
                    
                    // 应用背景色
                    if let Some(bg_color) = segment.background_color {
                        rich_text = rich_text.background_color(bg_color);
                    }
                    
                    // 应用文本样式
                    if segment.bold {
                        rich_text = rich_text.strong();
                    }
                    if segment.italic {
                        rich_text = rich_text.italics();
                    }
                    if segment.underline {
                        rich_text = rich_text.underline();
                    }
                    
                    // 处理反显
                    if segment.inverse {
                        rich_text = rich_text
                            .background_color(egui::Color32::BLACK)
                            .color(egui::Color32::WHITE);
                    }
                    
                    // 渲染segment
                    ui.add(egui::Label::new(rich_text).selectable(true));
                }
            }

            // 然后在同一行右侧添加输入内容和光标
            if self.is_connected && self.has_ssh_initial_output {
                // 显示输入内容
                if !self.input_buffer.is_empty() {
                    ui.add(egui::Label::new(
                        egui::RichText::new(&self.input_buffer)
                            .font(egui::FontId::monospace(14.0))
                            .color(egui::Color32::from_rgb(0, 102, 153))
                    ).selectable(true));
                }

                // 更新光标闪烁时间
                self.cursor_blink_time += ui.ctx().input(|i| i.stable_dt as f64);

                // 显示闪烁光标
                if (self.cursor_blink_time % 1.0) < 0.5 {
                    ui.add(egui::Label::new(
                        egui::RichText::new("█")
                            .font(egui::FontId::monospace(14.0))
                            .color(egui::Color32::from_rgb(0, 102, 153))
                    ).selectable(false));
                }

                // 处理键盘输入
                self.handle_keyboard_input(ui);
            }
        });

        ui.add_space(2.0);
    }

    /// ✅ 处理键盘输入事件
    fn handle_keyboard_input(&mut self, ui: &mut egui::Ui) {
        // 确保UI有焦点来接收键盘输入
        ui.memory_mut(|mem| mem.request_focus(ui.id()));

        ui.input(|i| {
            // 处理字符输入
            for event in &i.events {
                match event {
                    egui::Event::Text(text) => {
                        // 过滤掉控制字符
                        let filtered_text: String = text
                            .chars()
                            .filter(|c| !c.is_control() || *c == '\t')
                            .collect();
                        if !filtered_text.is_empty() {
                            self.input_buffer.push_str(&filtered_text);
                        }
                    }
                    egui::Event::Key {
                        key, pressed: true, ..
                    } => match key {
                        egui::Key::Enter => {
                            if !self.input_buffer.trim().is_empty() {
                                self.execute_command();
                            }
                        }
                        egui::Key::Backspace => {
                            self.input_buffer.pop();
                        }
                        _ => {}
                    },
                    _ => {}
                }
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
                        // 使用专门的PTY输出处理方法，会进行VT100解析和提示符提取
                        self.add_pty_output(output);
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
                    // 普通PTY命令处理 - 使用VT100解析
                    // 注意：命令已在execute_command中显示，这里只显示结果
                    match result.output {
                        Ok(output) => {
                            // 任何返回都交给VT100解析（包括空返回）
                            self.add_pty_output(output);
                        }
                        Err(error) => {
                            // PTY错误信息现在包含实际的命令输出，直接显示
                            self.add_pty_output(error);
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

            if self.is_connected && self.tab_id.is_some() {
                // ✅ 新流程：直接发送命令给SSH，不做本地处理
                // 让SSH返回完整的VT100序列，包含命令回显和输出
                self.scroll_to_bottom = true;
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

            // 清空输入缓冲，等待SSH返回完整结果
            self.input_buffer.clear();
        }
    }

    /// ✅ 复制所有终端内容到剪贴板
    fn copy_all_to_clipboard(&self, ui: &mut egui::Ui) {
        let all_text = self
            .output_buffer
            .iter()
            .map(|line| line.text())
            .collect::<Vec<_>>()
            .join("\n");

        if !all_text.trim().is_empty() {
            let text_len = all_text.len();
            ui.ctx().copy_text(all_text);
            crate::app_log!(info, "Terminal", "已复制 {} 字符到剪贴板", text_len);
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
