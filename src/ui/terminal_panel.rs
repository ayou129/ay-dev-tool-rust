use eframe::egui;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct TerminalPanel {
    pub title: String,
    pub connection_info: String,
    output_buffer: VecDeque<String>,
    input_buffer: String,
    scroll_to_bottom: bool,
    is_connected: bool,
}

impl TerminalPanel {
    pub fn new(title: String, connection_info: String) -> Self {
        let mut output_buffer = VecDeque::new();
        output_buffer.push_back(format!("正在连接到 {}...", connection_info));
        
        Self {
            title,
            connection_info: connection_info.clone(),
            output_buffer,
            input_buffer: String::new(),
            scroll_to_bottom: true,
            is_connected: false,
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
            self.add_output("连接成功!".to_string());
        } else {
            self.add_output("连接断开".to_string());
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // 连接状态栏
            ui.horizontal(|ui| {
                let status_color = if self.is_connected {
                    egui::Color32::GREEN
                } else {
                    egui::Color32::RED
                };
                
                ui.colored_label(status_color, if self.is_connected { "🟢" } else { "🔴" });
                ui.label(&self.connection_info);
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🔄 重连").clicked() {
                        // TODO: 实现重连逻辑
                        self.add_output("正在重新连接...".to_string());
                    }
                    
                    if ui.button("🗑️ 清屏").clicked() {
                        self.output_buffer.clear();
                    }
                });
            });
            
            ui.separator();
            
            // 终端输出区域
            let output_height = ui.available_height() - 60.0; // 为输入区域预留空间
            
            egui::ScrollArea::vertical()
                .stick_to_bottom(self.scroll_to_bottom)
                .max_height(output_height)
                .show(ui, |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        for line in &self.output_buffer {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(line)
                                            .font(egui::FontId::monospace(12.0))
                                    ).wrap()
                                );
                            });
                        }
                    });
                });
            
            if self.scroll_to_bottom {
                self.scroll_to_bottom = false;
            }
            
            ui.separator();
            
            // 终端输入区域
            ui.horizontal(|ui| {
                ui.label("$");
                
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.input_buffer)
                        .font(egui::FontId::monospace(12.0))
                        .desired_width(ui.available_width() - 80.0)
                );
                
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.execute_command();
                }
                
                if ui.button("发送").clicked() {
                    self.execute_command();
                }
            });
        });
    }

    fn execute_command(&mut self) {
        if !self.input_buffer.trim().is_empty() {
            let command = self.input_buffer.clone();
            self.add_output(format!("$ {}", command));
            
            // TODO: 实际执行 SSH 命令
            if self.is_connected {
                // 模拟命令执行
                match command.trim() {
                    "ls" => self.add_output("file1.txt  file2.txt  directory/".to_string()),
                    "pwd" => self.add_output("/home/user".to_string()),
                    "whoami" => self.add_output("user".to_string()),
                    _ => self.add_output(format!("bash: {}: command not found", command.trim())),
                }
            } else {
                self.add_output("错误: 未连接到远程主机".to_string());
            }
            
            self.input_buffer.clear();
        }
    }
}
