// 终端模块 - 模块化的终端模拟器实现

pub mod types;
pub mod vt100_handler;
pub mod content_extractor;
pub mod emulator;

// 重新导出公共接口
pub use types::{TerminalSegment, TerminalLine};
pub use emulator::TerminalEmulator;
