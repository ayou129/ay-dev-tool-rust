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

/// 终端模拟器 - 负责将VT100解析结果转换为终端逻辑
pub struct TerminalEmulator {
    parser: vt100::Parser,
}

impl TerminalEmulator {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            parser: vt100::Parser::new(height, width, 0),
        }
    }

    /// 处理SSH原始输出，返回格式化的终端行
    pub fn process_ssh_output(&mut self, raw_data: &str) -> Vec<TerminalLine> {
        // 让VT100解析ANSI序列
        self.parser.process(raw_data.as_bytes());

        // 将VT100解析结果转换为终端逻辑
        self.extract_terminal_lines()
    }

    /// 从VT100解析器中提取格式化的终端行
    fn extract_terminal_lines(&self) -> Vec<TerminalLine> {
        let screen = self.parser.screen();
        let mut lines = Vec::new();

        for row in 0..screen.size().0 {
            let line = self.extract_line_from_screen(row, &screen);

            // 终端逻辑：跳过填充行
            if self.is_padding_line(&line) {
                continue;
            }

            // 只保留有意义的行
            if !line.is_empty() {
                lines.push(line);
            }
        }

        lines
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

    /// 从VT100单元格提取字符属性
    fn extract_cell_attributes(&self, cell: &vt100::Cell) -> TerminalSegment {
        TerminalSegment {
            text: String::new(),
            color: self.convert_vt100_color(cell.fgcolor()),
            background_color: self.convert_vt100_color(cell.bgcolor()),
            bold: cell.bold(),
            italic: cell.italic(),
            underline: cell.underline(),
            inverse: cell.inverse(),
        }
    }

    /// 将VT100颜色转换为egui颜色
    fn convert_vt100_color(&self, color: vt100::Color) -> Option<egui::Color32> {
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
