use vt100;

use super::types::{TerminalLine, TerminalSegment, TerminalProcessResult};

/// 内容提取器 - 从VT100解析结果提取显示内容 (增量式处理)
pub struct ContentExtractor {
    /// 记录上次处理的最大行数
    last_max_row: u16,
}

impl ContentExtractor {
    pub fn new() -> Self {
        Self {
            last_max_row: 0,
        }
    }

    /// 从VT100解析器提取终端内容 - 增量式处理
    pub fn extract_content(&mut self, parser: &vt100::Parser) -> TerminalProcessResult {
        let screen = parser.screen();
        
        // 🔑 关键：只提取新增的内容和当前光标附近的行
        let lines = self.extract_new_content(&screen);
        let prompt_update = self.detect_prompt(&screen);
        
        TerminalProcessResult {
            lines,
            prompt_update,
        }
    }

    /// 提取新内容 - 只提取增量部分
    fn extract_new_content(&mut self, screen: &vt100::Screen) -> Vec<TerminalLine> {
        let mut lines = Vec::new();
        let total_rows = screen.size().0;
        let cursor_row = screen.cursor_position().0;
        
        // 找到最后一个有内容的行
        let mut max_content_row = 0;
        for row in 0..total_rows {
            let line = self.extract_line_from_screen(row, screen);
            if !line.text().trim().is_empty() {
                max_content_row = row;
            }
        }
        
        // 🔑 关键策略：只提取从上次最大行到当前最大行的内容
        let start_row = if max_content_row > self.last_max_row {
            self.last_max_row
        } else {
            // 如果没有新内容，只提取光标所在行
            if cursor_row > 0 { cursor_row - 1 } else { 0 }
        };
        
        for row in start_row..=max_content_row {
            let line = self.extract_line_from_screen(row, screen);
            if !line.text().trim().is_empty() {
                crate::app_log!(debug, "VT100", "提取第{}行: {}", row, line.text().trim());
                lines.push(line);
            }
        }
        
        // 更新状态
        self.last_max_row = max_content_row;
        
        lines
    }
    
    /// 检测命令提示符
    fn detect_prompt(&self, screen: &vt100::Screen) -> Option<String> {
        let (cursor_row, _) = screen.cursor_position();
        
        if cursor_row >= 1 {
            let current_line = self.extract_line_from_screen(cursor_row - 1, screen);
            let text = current_line.text().trim().to_string();
            
            if !text.is_empty() && !text.starts_with("Last login") {
                Some(text)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// 从屏幕提取单行内容 - 简化版本
    fn extract_line_from_screen(&self, row: u16, screen: &vt100::Screen) -> TerminalLine {
        let mut line = TerminalLine::new();
        let cols = screen.size().1;
        
        let mut text_content = String::new();
        
        // 简化：只提取文本内容，不处理复杂样式
        for col in 0..cols {
            if let Some(cell) = screen.cell(row, col) {
                let ch = cell.contents();
                if !ch.is_empty() {
                    text_content.push_str(&ch);
                }
            }
        }
        
        // 创建单个默认样式的片段
        if !text_content.is_empty() {
            line.segments.push(TerminalSegment {
                text: text_content,
                color: None,
                background_color: None,
                bold: false,
                italic: false,
                underline: false,
                inverse: false,
            });
        }
        
        line
    }
}