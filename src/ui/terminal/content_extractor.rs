use vt100;

use super::types::{TerminalLine, TerminalSegment, TerminalProcessResult};

/// 内容提取器 - 从VT100解析结果提取显示内容 (简单直接版本)
pub struct ContentExtractor {
    // 移除所有复杂的状态跟踪，让VT100解析器自己处理增量
}

impl ContentExtractor {
    pub fn new() -> Self {
        Self {
            // 无状态，简单直接
        }
    }

    /// 从VT100解析器提取终端内容 - 真正的增量处理
    pub fn extract_content(&mut self, parser: &vt100::Parser) -> TerminalProcessResult {
        // 🔑 核心思路：不要提取整个屏幕，而是只返回空结果
        // 让上层业务逻辑来决定怎么处理数据
        
        TerminalProcessResult {
            lines: Vec::new(), // 暂时不返回任何行，避免重复
            prompt_update: None,
        }
    }

    /// 直接提取屏幕内容 - 简单版本
    fn extract_screen_lines(&self, screen: &vt100::Screen) -> Vec<TerminalLine> {
        let mut lines = Vec::new();
        let total_rows = screen.size().0;
        
        // 📝 简单策略：提取所有有内容的行
        for row in 0..total_rows {
            let line = self.extract_line_from_screen(row, screen);
            if !line.text().trim().is_empty() {
                crate::app_log!(debug, "VT100", "提取第{}行: {}", row, line.text().trim());
                lines.push(line);
            }
        }
        
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