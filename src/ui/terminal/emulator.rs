use vt100;

use super::types::{TerminalProcessResult, TerminalLine, TerminalSegment};
use super::vt100_handler::Vt100Handler;

/// 核心终端模拟器 - 简化版本(直接使用VT100状态)
pub struct TerminalEmulator {
    parser: vt100::Parser,
    vt100_handler: Vt100Handler,
    width: u16,
    height: u16,
}

impl TerminalEmulator {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            parser: vt100::Parser::new(height, width, 1000),
            vt100_handler: Vt100Handler::new(),
            width,
            height,
        }
    }

    /// 处理PTY输出数据 - 主要入口方法(直接使用VT100屏幕状态)
    pub fn process_pty_output(&mut self, data: &str) -> TerminalProcessResult {
        // 处理VT100序列
        self.handle_vt100_sequences(data);
        
        // 将数据传给解析器
        self.parser.process(data.as_bytes());
        
        // 🔑 关键：直接从 VT100 解析器获取屏幕内容
        self.extract_screen_content()
    }

    /// 处理VT100序列 - 简化版本
    fn handle_vt100_sequences(&self, raw_data: &str) {
        self.vt100_handler.handle_clear_screen(raw_data);
        self.vt100_handler.handle_clear_line(raw_data);
        self.vt100_handler.handle_cursor_move(raw_data);
        self.vt100_handler.handle_control_chars(raw_data);
    }

    /// 获取终端尺寸
    pub fn size(&self) -> (u16, u16) {
        (self.height, self.width)
    }

    /// 获取光标位置
    pub fn cursor_position(&self) -> (u16, u16) {
        let pos = self.parser.screen().cursor_position();
        (pos.0, pos.1)
    }

    /// 获取标题
    pub fn title(&self) -> &str {
        self.parser.screen().title()
    }

    /// 获取图标名称
    pub fn icon_name(&self) -> &str {
        self.parser.screen().icon_name()
    }

    /// 是否为备用屏幕模式
    pub fn is_alternate_screen(&self) -> bool {
        self.parser.screen().alternate_screen()
    }

    /// 光标是否隐藏
    pub fn is_cursor_hidden(&self) -> bool {
        self.parser.screen().hide_cursor()
    }

    /// 获取解析错误计数
    pub fn error_count(&self) -> usize {
        self.parser.screen().errors()
    }

    /// 重置终端状态
    pub fn reset(&mut self) {
        self.parser = vt100::Parser::new(self.height, self.width, 1000);
    }

    /// 🔑 从 VT100 屏幕直接获取完整状态(优化版，去除尾部空行)
    fn extract_screen_content(&self) -> TerminalProcessResult {
        let screen = self.parser.screen();
        let mut lines = Vec::new();
        
        // 🎯 关键修复：从屏幕获取所有行，但只保留有内容的部分
        let screen_height = screen.size().0;
        let mut last_content_row = 0;
        
        // 首先找到最后一行有内容的行
        for row in (0..screen_height).rev() {
            let line = self.extract_line_from_screen(row, &screen);
            if !line.is_empty() {
                last_content_row = row;
                break;
            }
        }
        
        // 只返回到最后一行有内容的行，避免大量空行
        for row in 0..=last_content_row {
            let line = self.extract_line_from_screen(row, &screen);
            lines.push(line);
        }
        
        // 检测提示符(从光标位置)
        let prompt_update = self.detect_prompt(&screen);
        
        crate::app_log!(debug, "VT100", "📺 屏幕状态更新: {} 行 (最后内容行: {})", lines.len(), last_content_row);
        
        TerminalProcessResult {
            lines,
            prompt_update,
        }
    }
    
    /// 从屏幕提取单行内容
    fn extract_line_from_screen(&self, row: u16, screen: &vt100::Screen) -> TerminalLine {
        let mut line = TerminalLine::new();
        let mut current_segment = TerminalSegment::default();
        let screen_width = screen.size().1;
        
        for col in 0..screen_width {
            if let Some(cell) = screen.cell(row, col) {
                let ch = cell.contents();
                
                // 检查字符属性是否变化
                let new_attrs = TerminalSegment {
                    text: String::new(),
                    color: self.convert_vt100_color(cell.fgcolor()),
                    background_color: self.convert_vt100_color(cell.bgcolor()),
                    bold: cell.bold(),
                    italic: cell.italic(),
                    underline: cell.underline(),
                    inverse: cell.inverse(),
                };
                
                // 如果属性变化，保存当前片段并开始新片段
                if self.attributes_changed(&current_segment, &new_attrs) {
                    if !current_segment.text.is_empty() {
                        line.segments.push(current_segment);
                    }
                    current_segment = new_attrs;
                }
                
                // 添加字符到当前片段
                if !ch.is_empty() {
                    current_segment.text.push_str(&ch);
                }
            }
        }
        
        // 添加最后一个片段
        if !current_segment.text.is_empty() {
            line.segments.push(current_segment);
        }
        
        line
    }
    
    /// 检测命令提示符 - 修复版本
    fn detect_prompt(&self, screen: &vt100::Screen) -> Option<String> {
        let (cursor_row, cursor_col) = screen.cursor_position();
        
        // 🎯 关键修复：从光标所在行提取提示符
        let current_line = self.extract_line_from_screen(cursor_row, screen);
        let line_text = current_line.text();
        
        // 🔑 提取光标位置之前的内容作为提示符
        if cursor_col > 0 && !line_text.trim().is_empty() {
            let prompt_text = if cursor_col as usize <= line_text.len() {
                line_text[..cursor_col as usize].trim().to_string()
            } else {
                line_text.trim().to_string()
            };
            
            // 过滤掉不需要的内容
            if !prompt_text.is_empty() 
                && !prompt_text.starts_with("Last login") 
                && !prompt_text.contains("from ") {
                crate::app_log!(debug, "VT100", "🎯 检测到提示符: '{}' (光标位置: {}:{})", prompt_text, cursor_row, cursor_col);
                return Some(prompt_text);
            }
        }
        
        None
    }
    
    /// 将VT100颜色转换为egui颜色
    fn convert_vt100_color(&self, color: vt100::Color) -> Option<egui::Color32> {
        match color {
            vt100::Color::Default => None,
            vt100::Color::Idx(idx) => {
                // 标准的16色调色板
                match idx {
                    0 => Some(egui::Color32::BLACK),
                    1 => Some(egui::Color32::from_rgb(128, 0, 0)),   // 红色
                    2 => Some(egui::Color32::from_rgb(0, 128, 0)),   // 绿色
                    3 => Some(egui::Color32::from_rgb(128, 128, 0)), // 黄色
                    4 => Some(egui::Color32::from_rgb(0, 0, 128)),   // 蓝色
                    5 => Some(egui::Color32::from_rgb(128, 0, 128)), // 紫色
                    6 => Some(egui::Color32::from_rgb(0, 128, 128)), // 青色
                    7 => Some(egui::Color32::LIGHT_GRAY),
                    8 => Some(egui::Color32::DARK_GRAY),
                    9 => Some(egui::Color32::RED),
                    10 => Some(egui::Color32::GREEN),
                    11 => Some(egui::Color32::YELLOW),
                    12 => Some(egui::Color32::BLUE),
                    13 => Some(egui::Color32::from_rgb(255, 0, 255)), // 品红
                    14 => Some(egui::Color32::from_rgb(0, 255, 255)), // 青色
                    15 => Some(egui::Color32::WHITE),
                    _ => None,
                }
            }
            vt100::Color::Rgb(r, g, b) => Some(egui::Color32::from_rgb(r, g, b)),
        }
    }
    
    /// 检查属性是否变化
    fn attributes_changed(&self, current: &TerminalSegment, new: &TerminalSegment) -> bool {
        current.color != new.color
            || current.background_color != new.background_color
            || current.bold != new.bold
            || current.italic != new.italic
            || current.underline != new.underline
            || current.inverse != new.inverse
    }
}
