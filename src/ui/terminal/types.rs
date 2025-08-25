use eframe::egui;

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
    pub prompt_update: Option<String>,
}
