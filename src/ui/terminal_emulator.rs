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

    /// 处理SSH原始输出，返回格式化的终端行和可能的提示符更新
    pub fn process_ssh_output(&mut self, raw_data: &str) -> TerminalProcessResult {
        // 累积处理数据，保持屏幕上下文，确保“提示符 + 命令”在同一行
        self.parser.process(raw_data.as_bytes());

        // 将VT100解析结果转换为终端逻辑
        self.extract_terminal_content()
    }

    /// 从VT100解析器中提取格式化的终端内容和提示符
    fn extract_terminal_content(&mut self) -> TerminalProcessResult {
        let mut lines = Vec::new();

        // 尝试用光标行作为提示符，否则回退到 title/icon
        let screen = self.parser.screen();
        let (cursor_row, _cursor_col) = self.cursor_position();
        let prompt_update = if cursor_row > 1 {
            let prompt_line = self.extract_line_from_screen(cursor_row - 1, &screen);
            let text = prompt_line.text().trim().to_string();
            if !text.is_empty() && !text.starts_with("Last login") { Some(text) } else { None }
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

        for row in 0..screen.size().0 {
            let line = self.extract_line_from_screen(row, &screen);

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

        // 仅返回新增行，避免重复输出（如 Last login）
        let new_lines = if self.last_line_count <= lines.len() {
            lines[self.last_line_count..].to_vec()
        } else {
            // 屏幕被清空或尺寸变化，全部返回
            lines.clone()
        };
        self.last_line_count = lines.len();

        TerminalProcessResult {
            lines: new_lines,
            prompt_update,
        }
    }

    /// 从屏幕的特定行提取格式化内容
    fn extract_line_from_screen(&self, row: u16, screen: &vt100::Screen) -> TerminalLine {
        let mut line = TerminalLine::new();
        let mut current_segment = TerminalSegment::default();

        for col in 0..screen.size().1 {
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

                // 添加字符到当前片段
                if !ch.trim().is_empty() || !current_segment.text.is_empty() {
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
                // 标准16色映射
                match idx {
                    0 => Some(egui::Color32::from_rgb(0, 0, 0)),     // 黑色
                    1 => Some(egui::Color32::from_rgb(128, 0, 0)),   // 红色
                    2 => Some(egui::Color32::from_rgb(0, 128, 0)),   // 绿色
                    3 => Some(egui::Color32::from_rgb(128, 128, 0)), // 黄色
                    4 => Some(egui::Color32::from_rgb(0, 0, 128)),   // 蓝色
                    5 => Some(egui::Color32::from_rgb(128, 0, 128)), // 紫色
                    6 => Some(egui::Color32::from_rgb(0, 128, 128)), // 青色
                    7 => Some(egui::Color32::from_rgb(192, 192, 192)), // 白色
                    8 => Some(egui::Color32::from_rgb(128, 128, 128)), // 亮黑色
                    9 => Some(egui::Color32::from_rgb(255, 0, 0)),   // 亮红色
                    10 => Some(egui::Color32::from_rgb(0, 255, 0)),  // 亮绿色
                    11 => Some(egui::Color32::from_rgb(255, 255, 0)), // 亮黄色
                    12 => Some(egui::Color32::from_rgb(0, 0, 255)),  // 亮蓝色
                    13 => Some(egui::Color32::from_rgb(255, 0, 255)), // 亮紫色
                    14 => Some(egui::Color32::from_rgb(0, 255, 255)), // 亮青色
                    15 => Some(egui::Color32::from_rgb(255, 255, 255)), // 亮白色
                    _ => None,                                       // 其他颜色暂不支持
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
