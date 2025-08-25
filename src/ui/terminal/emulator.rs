use vt100;

use super::types::TerminalProcessResult;
use super::vt100_handler::Vt100Handler;
use super::content_extractor::ContentExtractor;

/// 核心终端模拟器 - 简化版本
pub struct TerminalEmulator {
    parser: vt100::Parser,
    vt100_handler: Vt100Handler,
    content_extractor: ContentExtractor,
    width: u16,
    height: u16,
}

impl TerminalEmulator {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            parser: vt100::Parser::new(height, width, 1000),
            vt100_handler: Vt100Handler::new(),
            content_extractor: ContentExtractor::new(),
            width,
            height,
        }
    }

    /// 处理PTY输出数据 - 主要入口方法
    pub fn process_pty_output(&mut self, data: &str) -> TerminalProcessResult {
        // 处理VT100序列
        self.handle_vt100_sequences(data);
        
        // 将数据传给解析器
        self.parser.process(data.as_bytes());
        
        // 提取内容 - 使用可变引用支持增量式处理
        self.content_extractor.extract_content(&self.parser)
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
}
