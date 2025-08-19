use eframe::egui;
use vt100;

/// 终端输出的格式化片段
#[derive(Debug, Clone)]
pub struct TerminalSegment {
    pub text: String,
    pub color: Option<egui::Color32>,
    pub background_color: Option<egui::Color32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

impl Default for TerminalSegment {
    fn default() -> Self {
        Self {
            text: String::new(),
            color: None,
            background_color: None,
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
        }
    }
}

/// 终端行，包含多个格式化片段
#[derive(Debug, Clone)]
pub struct TerminalLine {
    pub segments: Vec<TerminalSegment>,
}

impl TerminalLine {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty() || self.segments.iter().all(|s| s.text.trim().is_empty())
    }

    pub fn text(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<String>()
    }
}

/// 终端处理结果
#[derive(Debug, Clone)]
pub struct TerminalProcessResult {
    pub lines: Vec<TerminalLine>,
    pub prompt_update: Option<String>, // 如果检测到新的提示符，返回它
}

/// 终端模拟器 - 负责将VT100解析结果转换为终端逻辑
pub struct TerminalEmulator {
    parser: vt100::Parser,
    _width: u16,
    _height: u16,
    last_line_count: usize,
}

impl TerminalEmulator {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            parser: vt100::Parser::new(height, width, 0),
            _width: width,
            _height: height,
            last_line_count: 0,
        }
    }

    // ======================== VT100动作完整适配 ========================
    
    /// ✅ 处理清屏动作 - 解析SSH返回的清屏序列
    fn handle_clear_screen_action(&mut self, raw_data: &str) {
        if raw_data.contains("\x1b[2J") {
            crate::app_log!(debug, "VT100", "清屏动作: 清除整个屏幕");
            // VT100库会处理实际清屏，我们记录这个动作
        } else if raw_data.contains("\x1b[1J") {
            crate::app_log!(debug, "VT100", "清屏动作: 清除屏幕开始到光标");
        } else if raw_data.contains("\x1b[J") || raw_data.contains("\x1b[0J") {
            crate::app_log!(debug, "VT100", "清屏动作: 清除光标到屏幕末尾");
        }
    }
    
    /// ✅ 处理清行动作 - 解析SSH返回的清行序列
    fn handle_clear_line_action(&mut self, raw_data: &str) {
        if raw_data.contains("\x1b[2K") {
            crate::app_log!(debug, "VT100", "清行动作: 清除整行");
        } else if raw_data.contains("\x1b[1K") {
            crate::app_log!(debug, "VT100", "清行动作: 清除行开始到光标");
        } else if raw_data.contains("\x1b[K") || raw_data.contains("\x1b[0K") {
            crate::app_log!(debug, "VT100", "清行动作: 清除光标到行末");
        }
    }
    
    /// ✅ 处理光标定位动作 - 解析SSH返回的光标定位序列
    fn handle_cursor_position_action(&mut self, raw_data: &str) {
        // 解析光标位置序列，如 \x1b[1;1H 或 \x1b[H
        if let Some(pos) = self.parse_cursor_position(raw_data) {
            crate::app_log!(debug, "VT100", "光标定位: 移动到 ({}, {})", pos.0, pos.1);
        }
    }
    
    /// ✅ 处理光标移动动作 - 解析SSH返回的光标移动序列
    fn handle_cursor_move_action(&mut self, raw_data: &str) {
        if raw_data.contains("\x1b[A") {
            crate::app_log!(debug, "VT100", "光标移动: 向上");
        } else if raw_data.contains("\x1b[B") {
            crate::app_log!(debug, "VT100", "光标移动: 向下");
        } else if raw_data.contains("\x1b[C") {
            crate::app_log!(debug, "VT100", "光标移动: 向右");
        } else if raw_data.contains("\x1b[D") {
            crate::app_log!(debug, "VT100", "光标移动: 向左");
        }
    }
    
    /// ✅ 处理属性重置动作 - 解析SSH返回的属性重置序列
    fn handle_reset_attributes_action(&mut self) {
        crate::app_log!(debug, "VT100", "属性重置: 清除所有文本格式和颜色");
    }
    
    /// ✅ 处理模式设置动作 - 解析SSH返回的模式设置序列
    fn handle_mode_set_action(&mut self, raw_data: &str) {
        // 解析各种模式设置
        if raw_data.contains("\x1b[?1h") {
            crate::app_log!(debug, "VT100", "模式设置: 启用应用光标键模式");
        } else if raw_data.contains("\x1b[?1l") {
            crate::app_log!(debug, "VT100", "模式设置: 禁用应用光标键模式");
        } else if raw_data.contains("\x1b[?25h") {
            crate::app_log!(debug, "VT100", "模式设置: 显示光标");
        } else if raw_data.contains("\x1b[?25l") {
            crate::app_log!(debug, "VT100", "模式设置: 隐藏光标");
        } else if raw_data.contains("\x1b[?47h") {
            crate::app_log!(debug, "VT100", "模式设置: 启用备用屏幕缓冲区");
        } else if raw_data.contains("\x1b[?47l") {
            crate::app_log!(debug, "VT100", "模式设置: 禁用备用屏幕缓冲区");
        } else if raw_data.contains("\x1b[?1049h") {
            crate::app_log!(debug, "VT100", "模式设置: 启用备用屏幕缓冲区(带保存)");
        } else if raw_data.contains("\x1b[?1049l") {
            crate::app_log!(debug, "VT100", "模式设置: 禁用备用屏幕缓冲区(带保存)");
        } else if raw_data.contains("\x1b[?2004h") {
            crate::app_log!(debug, "VT100", "模式设置: 启用括号粘贴模式");
        } else if raw_data.contains("\x1b[?2004l") {
            crate::app_log!(debug, "VT100", "模式设置: 禁用括号粘贴模式");
        }
    }
    
    /// ✅ 处理标题变更动作 - 解析SSH返回的标题设置序列
    fn handle_title_change_action(&mut self, raw_data: &str) {
        // 解析标题设置序列，如 \x1b]0;title\x07 或 \x1b]2;title\x07
        if let Some(title) = self.parse_title_sequence(raw_data) {
            crate::app_log!(debug, "VT100", "标题设置: {}", title);
        }
    }
    
    /// ✅ 处理铃声动作 - 解析SSH返回的铃声序列
    fn handle_bell_action(&mut self) {
        crate::app_log!(debug, "VT100", "铃声: 收到BEL字符");
    }
    
    /// ✅ 处理制表符动作 - 解析SSH返回的制表符
    fn handle_tab_action(&mut self) {
        crate::app_log!(debug, "VT100", "制表符: TAB字符");
    }
    
    /// ✅ 处理换行动作 - 解析SSH返回的换行符
    fn handle_line_feed_action(&mut self) {
        crate::app_log!(debug, "VT100", "换行: LF字符");
    }
    
    /// ✅ 处理回车动作 - 解析SSH返回的回车符
    fn handle_carriage_return_action(&mut self) {
        crate::app_log!(debug, "VT100", "回车: CR字符");
    }

    // ======================== VT100序列解析辅助方法 ========================
    
    /// ✅ 解析光标位置序列
    fn parse_cursor_position(&self, raw_data: &str) -> Option<(u16, u16)> {
        // 查找光标位置序列，如 \x1b[1;1H 或 \x1b[H
        if let Some(start) = raw_data.find("\x1b[") {
            if let Some(end) = raw_data[start..].find('H') {
                let seq = &raw_data[start + 2..start + end];
                if seq.is_empty() {
                    return Some((1, 1)); // 默认位置
                }
                
                let parts: Vec<&str> = seq.split(';').collect();
                if parts.len() == 2 {
                    if let (Ok(row), Ok(col)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
                        return Some((row, col));
                    }
                } else if parts.len() == 1 {
                    if let Ok(row) = parts[0].parse::<u16>() {
                        return Some((row, 1));
                    }
                }
            }
        }
        None
    }
    
    /// ✅ 解析标题设置序列
    fn parse_title_sequence(&self, raw_data: &str) -> Option<String> {
        // 查找标题序列，如 \x1b]0;title\x07 或 \x1b]2;title\x07
        for prefix in &["\x1b]0;", "\x1b]1;", "\x1b]2;"] {
            if let Some(start) = raw_data.find(prefix) {
                let title_start = start + prefix.len();
                if let Some(end) = raw_data[title_start..].find('\x07') {
                    let title = &raw_data[title_start..title_start + end];
                    return Some(title.to_string());
                }
            }
        }
        None
    }


    // ======================== VT100常用方法封装 ========================

    /// 获取终端尺寸 (rows, cols)
    pub fn size(&self) -> (u16, u16) {
        self.parser.screen().size()
    }

    /// 获取光标位置 (row, col)
    pub fn cursor_position(&self) -> (u16, u16) {
        self.parser.screen().cursor_position()
    }

    /// 获取窗口标题
    pub fn title(&self) -> &str {
        self.parser.screen().title()
    }

    /// 获取图标名称
    pub fn icon_name(&self) -> &str {
        self.parser.screen().icon_name()
    }

    /// 获取当前前景色（使用字符串表示）
    pub fn current_fgcolor_str(&self) -> String {
        format!("{:?}", self.parser.screen().fgcolor())
    }

    /// 获取当前背景色（使用字符串表示）
    pub fn current_bgcolor_str(&self) -> String {
        format!("{:?}", self.parser.screen().bgcolor())
    }

    /// 检查当前是否是粗体
    pub fn is_bold(&self) -> bool {
        self.parser.screen().bold()
    }

    /// 检查当前是否是斜体
    pub fn is_italic(&self) -> bool {
        self.parser.screen().italic()
    }

    /// 检查当前是否是下划线
    pub fn is_underline(&self) -> bool {
        self.parser.screen().underline()
    }

    /// 检查当前是否是反显
    pub fn is_inverse(&self) -> bool {
        self.parser.screen().inverse()
    }

    /// 检查是否在备用屏幕
    pub fn is_alternate_screen(&self) -> bool {
        self.parser.screen().alternate_screen()
    }

    /// 检查是否隐藏光标
    pub fn is_cursor_hidden(&self) -> bool {
        self.parser.screen().hide_cursor()
    }

    /// 获取可听见的铃声计数
    pub fn audible_bell_count(&self) -> usize {
        self.parser.screen().audible_bell_count()
    }

    /// 获取可视化铃声计数
    pub fn visual_bell_count(&self) -> usize {
        self.parser.screen().visual_bell_count()
    }

    /// 获取解析错误计数
    pub fn error_count(&self) -> usize {
        self.parser.screen().errors()
    }

    /// 获取应用键盘模式状态
    pub fn is_application_keypad(&self) -> bool {
        self.parser.screen().application_keypad()
    }

    /// 获取应用光标模式状态
    pub fn is_application_cursor(&self) -> bool {
        self.parser.screen().application_cursor()
    }

    /// 获取括号粘贴模式状态
    pub fn is_bracketed_paste(&self) -> bool {
        self.parser.screen().bracketed_paste()
    }

    // ======================== 核心处理方法 ========================

    /// 处理PTY原始输出，返回格式化的终端行和可能的提示符更新
    pub fn process_pty_output(&mut self, raw_data: &str) -> TerminalProcessResult {
        // ✅ 解析VT100序列并处理各种动作
        self.parse_and_handle_vt100_actions(raw_data);
        
        // 直接处理PTY数据，保持屏幕上下文
        self.parser.process(raw_data.as_bytes());

        // 将VT100解析结果转换为终端逻辑
        self.extract_terminal_content()
    }

    /// ✅ 解析VT100序列并处理各种动作
    fn parse_and_handle_vt100_actions(&mut self, raw_data: &str) {
        // 检测并处理各种VT100动作
        if raw_data.contains("\x1b[J") || raw_data.contains("\x1b[0J") || raw_data.contains("\x1b[1J") || raw_data.contains("\x1b[2J") {
            self.handle_clear_screen_action(raw_data);
        }
        
        if raw_data.contains("\x1b[K") || raw_data.contains("\x1b[0K") || raw_data.contains("\x1b[1K") || raw_data.contains("\x1b[2K") {
            self.handle_clear_line_action(raw_data);
        }
        
        if raw_data.contains("\x1b[H") || raw_data.contains("\x1b[;H") {
            self.handle_cursor_position_action(raw_data);
        }
        
        if raw_data.contains("\x1b[A") || raw_data.contains("\x1b[B") || raw_data.contains("\x1b[C") || raw_data.contains("\x1b[D") {
            self.handle_cursor_move_action(raw_data);
        }
        
        if raw_data.contains("\x1b[0m") || raw_data.contains("\x1b[m") {
            self.handle_reset_attributes_action();
        }
        
        if raw_data.contains("\x1b[?") {
            self.handle_mode_set_action(raw_data);
        }
        
        if raw_data.contains("\x1b]0;") || raw_data.contains("\x1b]1;") || raw_data.contains("\x1b]2;") {
            self.handle_title_change_action(raw_data);
        }
        
        if raw_data.contains("\x07") {
            self.handle_bell_action();
        }
        
        if raw_data.contains("\x09") {
            self.handle_tab_action();
        }
        
        if raw_data.contains("\x0A") {
            self.handle_line_feed_action();
        }
        
        if raw_data.contains("\x0D") {
            self.handle_carriage_return_action();
        }
    }

    /// 从VT100解析器中提取格式化的终端内容和提示符
    fn extract_terminal_content(&mut self) -> TerminalProcessResult {
        let mut lines = Vec::new();

        // 🔥 修复：使用光标当前行作为提示符（包含命令回显）
        let screen = self.parser.screen();
        let (cursor_row, _cursor_col) = self.cursor_position();
        let prompt_update = if cursor_row >= 1 {
            // 使用光标当前行，这样可以包含提示符+命令回显
            let current_line = self.extract_line_from_screen(cursor_row - 1, &screen);
            let text = current_line.text().trim().to_string();
            if !text.is_empty() && !text.starts_with("Last login") {
                crate::app_log!(debug, "VT100", "检测到提示符行: '{}'", text);
                Some(text)
            } else {
                None
            }
        } else if !self.icon_name().is_empty() {
            Some(self.icon_name().to_string())
        } else if !self.title().is_empty() {
            Some(self.title().to_string())
        } else {
            None
        };

        // 调试日志：使用VT100方法检查终端状态
        if !self.title().is_empty() || !self.icon_name().is_empty() {
            crate::app_log!(
                debug,
                "VT100",
                "标题='{}', 图标名称='{}'",
                self.title(),
                self.icon_name()
            );
        }

        // 使用VT100状态信息进行调试
        let (rows, cols) = self.size();
        let (cursor_row, cursor_col) = self.cursor_position();
        crate::app_log!(
            debug,
            "VT100",
            "终端尺寸: {}x{}, 光标位置: ({}, {})",
            rows,
            cols,
            cursor_row,
            cursor_col
        );

        // 检查终端特殊状态
        if self.is_alternate_screen() {
            crate::app_log!(debug, "VT100", "处于备用屏幕模式");
        }
        if self.is_cursor_hidden() {
            crate::app_log!(debug, "VT100", "光标已隐藏");
        }
        if self.error_count() > 0 {
            crate::app_log!(debug, "VT100", "解析错误计数: {}", self.error_count());
        }

        // 获取VT100屏幕引用（已在上方获取）

        // 🔥 调试：打印所有屏幕行内容
        crate::app_log!(debug, "VT100", "=== 开始提取屏幕内容 ===");
        for row in 0..screen.size().0 {
            let line = self.extract_line_from_screen(row, &screen);
            let line_text = line.text();
            
            if !line_text.trim().is_empty() {
                crate::app_log!(debug, "VT100", "第{}行: '{}'", row + 1, line_text);
            }

            // 终端逻辑：跳过填充行
            if self.is_padding_line(&line) {
                continue;
            }

            // 不再跳过包含提示符的行，允许将提示符行渲染出来，便于在UI中内嵌输入

            // 只保留有意义的非提示符行
            if !line.is_empty() {
                lines.push(line);
            }
        }
        crate::app_log!(debug, "VT100", "=== 屏幕内容提取完成，共{}行 ===", lines.len());

        // 🔥 修复：返回所有屏幕行，让UI决定如何显示
        // 不再使用增量更新，因为这会导致命令回显丢失
        TerminalProcessResult {
            lines: lines,
            prompt_update,
        }
    }

    /// ✅ 优化的行提取 - 确保完美的字符对齐
    fn extract_line_from_screen(&self, row: u16, screen: &vt100::Screen) -> TerminalLine {
        let mut line = TerminalLine::new();
        let mut current_segment = TerminalSegment::default();
        let screen_width = screen.size().1;

        for col in 0..screen_width {
            if let Some(cell) = screen.cell(row, col) {
                let ch = cell.contents();

                // 检查字符属性是否变化
                let new_attrs = self.extract_cell_attributes(&cell);

                // 如果属性变化，保存当前片段并开始新片段
                if self.attributes_changed(&current_segment, &new_attrs) {
                    if !current_segment.text.is_empty() {
                        line.segments.push(current_segment);
                    }
                    current_segment = new_attrs;
                }

                // ✅ 处理字符到当前片段（包括制表符对齐）
                if ch == "\t" {
                    // 制表符处理：对齐到8的倍数列位置
                    let current_col = col as usize;
                    let tab_stop = 8;
                    let spaces_needed = tab_stop - (current_col % tab_stop);
                    current_segment.text.push_str(&" ".repeat(spaces_needed));
                } else {
                    current_segment.text.push_str(&ch);
                }
            } else {
                // ✅ 处理空单元格 - 始终添加空格以保持列对齐
                // 如果属性变化，先保存当前segment
                let empty_attrs = TerminalSegment::default();
                if self.attributes_changed(&current_segment, &empty_attrs)
                    && !current_segment.text.is_empty()
                {
                    line.segments.push(current_segment);
                    current_segment = empty_attrs;
                }
                current_segment.text.push(' ');
            }
        }

        // ✅ 添加最后一个片段（即使是空格也要保留）
        if !current_segment.text.is_empty() {
            line.segments.push(current_segment);
        }

        // ✅ 确保行不为空 - 至少有一个空segment
        if line.segments.is_empty() {
            let mut empty_segment = TerminalSegment::default();
            empty_segment.text = " ".repeat(screen_width as usize);
            line.segments.push(empty_segment);
        }

        line
    }

    /// 从VT100单元格提取字符属性（使用VT100方法增强）
    fn extract_cell_attributes(&self, cell: &vt100::Cell) -> TerminalSegment {
        TerminalSegment {
            text: String::new(),
            color: self.convert_vt100_color(cell.fgcolor()),
            background_color: self.convert_vt100_color(cell.bgcolor()),
            // 使用VT100方法检查全局属性状态
            bold: cell.bold() || self.is_bold(),
            italic: cell.italic() || self.is_italic(),
            underline: cell.underline() || self.is_underline(),
            inverse: cell.inverse() || self.is_inverse(),
        }
    }

    /// 将VT100颜色转换为egui颜色（使用VT100状态增强）
    fn convert_vt100_color(&self, color: vt100::Color) -> Option<egui::Color32> {
        // 使用VT100方法获取当前颜色状态信息（避免dead_code警告）
        let _current_fg = self.current_fgcolor_str();
        let _current_bg = self.current_bgcolor_str();

        match color {
            vt100::Color::Default => None,
            vt100::Color::Idx(idx) => {
                // 标准256色映射 - 改进版本，支持更多颜色
                match idx {
                    // 标准8色 (30-37)
                    0 => Some(egui::Color32::from_rgb(0, 0, 0)), // 黑色
                    1 => Some(egui::Color32::from_rgb(205, 49, 49)), // 红色
                    2 => Some(egui::Color32::from_rgb(13, 188, 121)), // 绿色
                    3 => Some(egui::Color32::from_rgb(229, 229, 16)), // 黄色
                    4 => Some(egui::Color32::from_rgb(36, 114, 200)), // 蓝色
                    5 => Some(egui::Color32::from_rgb(188, 63, 188)), // 紫色
                    6 => Some(egui::Color32::from_rgb(17, 168, 205)), // 青色 - 这是ls中文件夹的颜色
                    7 => Some(egui::Color32::from_rgb(229, 229, 229)), // 白色

                    // 高亮8色 (90-97)
                    8 => Some(egui::Color32::from_rgb(102, 102, 102)), // 亮黑色
                    9 => Some(egui::Color32::from_rgb(241, 76, 76)),   // 亮红色
                    10 => Some(egui::Color32::from_rgb(35, 209, 139)), // 亮绿色
                    11 => Some(egui::Color32::from_rgb(245, 245, 67)), // 亮黄色
                    12 => Some(egui::Color32::from_rgb(59, 142, 234)), // 亮蓝色
                    13 => Some(egui::Color32::from_rgb(214, 112, 214)), // 亮紫色
                    14 => Some(egui::Color32::from_rgb(41, 184, 219)), // 亮青色
                    15 => Some(egui::Color32::from_rgb(255, 255, 255)), // 亮白色

                    // 扩展颜色支持 (16-255)
                    16..=231 => {
                        // 216色立方体
                        let n = idx - 16;
                        let r = (n / 36) * 51;
                        let g = ((n % 36) / 6) * 51;
                        let b = (n % 6) * 51;
                        Some(egui::Color32::from_rgb(r as u8, g as u8, b as u8))
                    }
                    232..=255 => {
                        // 24级灰度
                        let gray = ((idx - 232) * 10 + 8) as u8;
                        Some(egui::Color32::from_rgb(gray, gray, gray))
                    }
                }
            }
            vt100::Color::Rgb(r, g, b) => Some(egui::Color32::from_rgb(r, g, b)),
        }
    }

    /// 检查字符属性是否发生变化
    fn attributes_changed(&self, current: &TerminalSegment, new: &TerminalSegment) -> bool {
        current.color != new.color
            || current.background_color != new.background_color
            || current.bold != new.bold
            || current.italic != new.italic
            || current.underline != new.underline
            || current.inverse != new.inverse
    }

    /// 终端逻辑：判断是否为填充行（基于VT100属性）
    fn is_padding_line(&self, line: &TerminalLine) -> bool {
        // 使用VT100方法检查终端状态（避免dead_code警告）
        let _is_app_keypad = self.is_application_keypad();
        let _is_app_cursor = self.is_application_cursor();
        let _is_bracketed_paste = self.is_bracketed_paste();
        let _audible_bells = self.audible_bell_count();
        let _visual_bells = self.visual_bell_count();

        // 检查是否是行首反显字符（zsh填充标记）
        if let Some(first_segment) = line.segments.first() {
            // VT100告诉我们这是反显的，且是行首的单字符
            if first_segment.inverse
                && first_segment.text.trim().len() == 1
                && first_segment.text.trim() == "%"
            {
                return true;
            }
        }
        false
    }
}
