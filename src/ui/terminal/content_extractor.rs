use vt100;

use super::types::{TerminalLine, TerminalSegment, TerminalProcessResult};

/// 内容提取器 - 从VT100解析结果提取显示内容 (简化版本)
pub struct ContentExtractor;

impl ContentExtractor {
    pub fn new() -> Self {
        Self
    }

    /// 从VT100解析器提取终端内容 - 简化版本
    pub fn extract_content(&self, parser: &vt100::Parser) -> TerminalProcessResult {
        let lines = self.extract_lines(parser);
        let prompt_update = self.detect_prompt(parser);
        
        TerminalProcessResult {
            lines,
            prompt_update,
        }
    }

    /// 提取屏幕行内容
    fn extract_lines(&self, parser: &vt100::Parser) -> Vec<TerminalLine> {
        let screen = parser.screen();
        let mut lines = Vec::new();
        
        // 收集所有非空行
        for row in 0..screen.size().0 {
            let line = self.extract_line_from_screen(row, &screen);
            if !line.text().trim().is_empty() {
                lines.push(line);
            }
        }
        
        lines
    }

    /// 检测命令提示符
    fn detect_prompt(&self, parser: &vt100::Parser) -> Option<String> {
        let screen = parser.screen();
        let (cursor_row, _) = (screen.cursor_position().0, screen.cursor_position().1);
        
        if cursor_row >= 1 {
            let current_line = self.extract_line_from_screen(cursor_row - 1, &screen);
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