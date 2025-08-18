use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Debug => write!(f, "DEBUG"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub level: LogLevel,
    pub module: String,
    pub message: String,
}

impl LogEntry {
    pub fn format_for_file(&self) -> String {
        format!(
            "[{}] [{}] [{}] {}",
            self.timestamp.format("%Y-%m-%d %H:%M:%S"),
            self.level,
            self.module,
            self.message
        )
    }
}

pub struct Logger {
    pub log_file_path: Option<PathBuf>,
    console_enabled: bool,
    file_enabled: bool,
    min_level: LogLevel,
}

impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}

impl Logger {
    pub fn new() -> Self {
        // 创建日志目录
        let log_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ay-dev-tool-rust");

        if !log_dir.exists() {
            let _ = std::fs::create_dir_all(&log_dir);
        }

        let log_file = log_dir.join("app.log");

        Self {
            log_file_path: Some(log_file),
            console_enabled: true,
            file_enabled: true,
            min_level: LogLevel::Debug, // 改为Debug级别以查看更多日志
        }
    }

    fn should_log(&self, level: &LogLevel) -> bool {
        match (&self.min_level, level) {
            (LogLevel::Debug, _) => true,
            (LogLevel::Info, LogLevel::Debug) => false,
            (LogLevel::Info, _) => true,
            (LogLevel::Warn, LogLevel::Debug | LogLevel::Info) => false,
            (LogLevel::Warn, _) => true,
            (LogLevel::Error, LogLevel::Error) => true,
            (LogLevel::Error, _) => false,
        }
    }

    pub fn log(&self, level: LogLevel, module: &str, message: &str) {
        if !self.should_log(&level) {
            return;
        }

        let entry = LogEntry {
            timestamp: Local::now(),
            level: level.clone(),
            module: module.to_string(),
            message: message.to_string(),
        };

        // ✅ 输出到控制台：直接打印到stdout/stderr，不依赖log宏
        if self.console_enabled {
            let console_output = format!("[{}] [{}] [{}] {}", 
                entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                level,
                module, 
                message
            );
            
            match level {
                LogLevel::Error => {
                    eprintln!("{}", console_output);
                    log::error!("[{}] {}", module, message);
                },
                LogLevel::Warn => {
                    println!("{}", console_output);
                    log::warn!("[{}] {}", module, message);
                },
                LogLevel::Info => {
                    println!("{}", console_output);
                    log::info!("[{}] {}", module, message);
                },
                LogLevel::Debug => {
                    println!("{}", console_output);
                    log::debug!("[{}] {}", module, message);
                },
            }
        }

        // 输出到文件
        if self.file_enabled {
            if let Some(ref log_path) = self.log_file_path {
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
                    writeln!(file, "{}", entry.format_for_file()).ok();
                }
            }
        }
    }

    pub fn error(&self, module: &str, message: &str) {
        self.log(LogLevel::Error, module, message);
    }

    pub fn warn(&self, module: &str, message: &str) {
        self.log(LogLevel::Warn, module, message);
    }

    pub fn info(&self, module: &str, message: &str) {
        self.log(LogLevel::Info, module, message);
    }

    pub fn debug(&self, module: &str, message: &str) {
        self.log(LogLevel::Debug, module, message);
    }
}

// 全局日志实例
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};

static GLOBAL_LOGGER: OnceLock<Arc<Mutex<Logger>>> = OnceLock::new();

pub fn init_logger() -> Arc<Mutex<Logger>> {
    GLOBAL_LOGGER
        .get_or_init(|| Arc::new(Mutex::new(Logger::new())))
        .clone()
}

pub fn get_logger() -> Arc<Mutex<Logger>> {
    init_logger()
}

// 便捷的全局日志宏
#[macro_export]
macro_rules! app_log {
    (error, $module:expr, $($arg:tt)*) => {
        if let Ok(logger) = $crate::utils::logger::get_logger().lock() {
            logger.error($module, &format!($($arg)*));
        }
    };
    (warn, $module:expr, $($arg:tt)*) => {
        if let Ok(logger) = $crate::utils::logger::get_logger().lock() {
            logger.warn($module, &format!($($arg)*));
        }
    };
    (info, $module:expr, $($arg:tt)*) => {
        if let Ok(logger) = $crate::utils::logger::get_logger().lock() {
            logger.info($module, &format!($($arg)*));
        }
    };
    (debug, $module:expr, $($arg:tt)*) => {
        if let Ok(logger) = $crate::utils::logger::get_logger().lock() {
            logger.debug($module, &format!($($arg)*));
        }
    };
}

// SSH 专用日志函数
// 已移除 - 冗余日志，有成功/失败日志即可
// pub fn log_ssh_connection_attempt(host: &str, port: u16, username: &str) {
//     if let Ok(logger) = get_logger().lock() {
//         logger.info("SSH", &format!("尝试连接到 {}@{}:{}", username, host, port));
//     }
// }

pub fn log_ssh_connection_success(host: &str, port: u16, username: &str) {
    if let Ok(logger) = get_logger().lock() {
        logger.info("SSH", &format!("成功连接到 {}@{}:{}", username, host, port));
    }
}

pub fn log_ssh_connection_failed(host: &str, port: u16, username: &str, error: &str) {
    if let Ok(logger) = get_logger().lock() {
        logger.error(
            "SSH",
            &format!("连接失败 {}@{}:{} - {}", username, host, port, error),
        );
    }
}

pub fn log_ssh_command_execution(command: &str, connection: &str) {
    if let Ok(logger) = get_logger().lock() {
        logger.info(
            "SSH",
            &format!("执行命令 '{}' 在连接 '{}'", command, connection),
        );
    }
}

pub fn log_ssh_command_success(command: &str, _connection: &str, output_length: usize) {
    if let Ok(logger) = get_logger().lock() {
        logger.info(
            "SSH",
            &format!(
                "命令 '{}' 执行成功，输出长度: {} 字符",
                command, output_length
            ),
        );
    }
}

pub fn log_ssh_command_failed(command: &str, _connection: &str, error: &str, ssh_output: &str) {
    if let Ok(logger) = get_logger().lock() {
        logger.error("SSH", &format!("命令 '{}' 执行失败 - {}", command, error));
        // ✅ 打印完整的SSH接收信息
        if !ssh_output.trim().is_empty() {
            logger.error("SSH", &format!("SSH输出内容: {}", ssh_output.trim()));
        }
    }
}

pub fn log_ssh_disconnection(connection: &str, reason: &str) {
    if let Ok(logger) = get_logger().lock() {
        logger.info("SSH", &format!("连接 '{}' 已断开 - {}", connection, reason));
    }
}

pub fn log_ssh_authentication_method(username: &str, auth_type: &str) {
    if let Ok(logger) = get_logger().lock() {
        logger.debug(
            "SSH",
            &format!("用户 '{}' 使用 '{}' 认证方式", username, auth_type),
        );
    }
}
