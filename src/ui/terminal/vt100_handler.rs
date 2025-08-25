/// VT100序列处理器 - 专门处理ANSI转义序列
pub struct Vt100Handler;

impl Vt100Handler {
    pub fn new() -> Self {
        Self
    }

    /// 处理清屏动作 - 解析SSH返回的清屏序列
    pub fn handle_clear_screen(&self, raw_data: &str) {
        if raw_data.contains("\x1b[2J") {
            crate::app_log!(debug, "VT100", "清屏: 整个屏幕");
        } else if raw_data.contains("\x1b[1J") {
            crate::app_log!(debug, "VT100", "清屏: 开始到光标");
        } else if raw_data.contains("\x1b[J") || raw_data.contains("\x1b[0J") {
            crate::app_log!(debug, "VT100", "清屏: 光标到末尾");
        }
    }

    /// 处理清行动作 - 解析SSH返回的清行序列
    pub fn handle_clear_line(&self, raw_data: &str) {
        if raw_data.contains("\x1b[2K") {
            crate::app_log!(debug, "VT100", "清行: 整行");
        } else if raw_data.contains("\x1b[1K") {
            crate::app_log!(debug, "VT100", "清行: 开始到光标");
        } else if raw_data.contains("\x1b[K") || raw_data.contains("\x1b[0K") {
            crate::app_log!(debug, "VT100", "清行: 光标到末尾");
        }
    }

    /// 处理光标移动动作 - 解析SSH返回的光标移动序列
    pub fn handle_cursor_move(&self, raw_data: &str) {
        if raw_data.contains("\x1b[A") {
            crate::app_log!(debug, "VT100", "光标: 向上");
        } else if raw_data.contains("\x1b[B") {
            crate::app_log!(debug, "VT100", "光标: 向下");
        } else if raw_data.contains("\x1b[C") {
            crate::app_log!(debug, "VT100", "光标: 向右");
        } else if raw_data.contains("\x1b[D") {
            crate::app_log!(debug, "VT100", "光标: 向左");
        }
    }

    /// 处理属性重置动作
    pub fn handle_reset_attributes(&self) {
        crate::app_log!(debug, "VT100", "属性重置");
    }

    /// 处理控制字符
    pub fn handle_control_chars(&self, raw_data: &str) {
        if raw_data.contains("\x07") {
            crate::app_log!(debug, "VT100", "响铃");
        }
        if raw_data.contains("\x09") {
            crate::app_log!(debug, "VT100", "制表符");
        }
        if raw_data.contains("\x0A") {
            crate::app_log!(debug, "VT100", "换行");
        }
        if raw_data.contains("\x0D") {
            crate::app_log!(debug, "VT100", "回车");
        }
    }

    /// 解析光标位置序列
    pub fn parse_cursor_position(&self, data: &str) -> Option<(u16, u16)> {
        // 简化的光标位置解析
        if data.contains("\x1b[H") {
            Some((1, 1)) // 默认位置
        } else {
            None
        }
    }
}
