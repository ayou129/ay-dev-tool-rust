use anyhow::Result;
use portable_pty::{CommandBuilder, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use crate::ui::{AuthType, ConnectionConfig};

/// SSH连接日志记录
fn log_ssh_connection_success(host: &str, port: u16, username: &str) {
    crate::app_log!(info, "SSH", "SSH连接建立成功: {}@{}:{}", username, host, port);
}

fn log_ssh_connection_failed(host: &str, port: u16, username: &str, error: &str) {
    crate::app_log!(error, "SSH", "SSH连接失败: {}@{}:{} - {}", username, host, port, error);
}

fn log_ssh_authentication_method(username: &str, method: &str) {
    crate::app_log!(debug, "SSH", "用户 '{}' 使用 '{}' 认证方式", username, method);
}

fn log_ssh_command_execution(command: &str, connection_id: &str) {
    crate::app_log!(info, "SSH", "执行命令 '{}' 在连接 '{}'", command, connection_id);
}

fn log_ssh_command_success(command: &str, connection_id: &str, output_len: usize) {
    crate::app_log!(info, "SSH", "命令 '{}' 执行成功，输出长度: {} 字符", command, output_len);
}

fn log_ssh_disconnection(connection_id: &str, reason: &str) {
    crate::app_log!(info, "SSH", "断开连接 '{}': {}", connection_id, reason);
}

/// PTY SSH连接
pub struct PtySshConnection {
    connection_info: ConnectionConfig,
    pty_pair: Box<dyn portable_pty::PtyPair + Send>,
    child_process: Box<dyn portable_pty::Child + Send + Sync>,
}

impl PtySshConnection {
    pub async fn connect(config: &ConnectionConfig) -> Result<Self> {
        let connection_id = format!("{}@{}:{}", config.username, config.host, config.port);
        
        // 创建PTY系统
        let pty_system = portable_pty::native_pty_system();
        
        // 创建PTY对
        let pty_pair = pty_system.openpty(PtySize {
            rows: 50,
            cols: 200,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // 构建SSH命令
        let mut ssh_cmd = CommandBuilder::new("ssh");
        
        // 基本SSH参数
        ssh_cmd.arg("-t"); // 强制分配PTY
        ssh_cmd.arg("-o").arg("StrictHostKeyChecking=no"); // 跳过主机密钥检查（开发用）
        
        // 端口设置
        if config.port != 22 {
            ssh_cmd.arg("-p").arg(config.port.to_string());
        }
        
        // 认证方式
        match &config.auth_type {
            AuthType::Password => {
                log_ssh_authentication_method(&config.username, "密码认证");
                // 对于密码认证，我们需要使用expect或类似工具，或者让用户手动输入
                // 这里先使用基本的ssh命令，密码需要手动输入
            }
            AuthType::PublicKey => {
                log_ssh_authentication_method(&config.username, "公钥认证");
                if let Some(key_file) = &config.key_file {
                    ssh_cmd.arg("-i").arg(key_file.as_ref());
                }
            }
        }
        
        // 目标主机
        ssh_cmd.arg(format!("{}@{}", config.username, config.host));
        
        // 启动SSH进程
        let child_process = pty_pair.slave().spawn_command(ssh_cmd)
            .map_err(|e| {
                let error_msg = format!("启动SSH进程失败: {}", e);
                log_ssh_connection_failed(&config.host, config.port, &config.username, &error_msg);
                anyhow::anyhow!(error_msg)
            })?;

        log_ssh_connection_success(&config.host, config.port, &config.username);

        Ok(Self {
            connection_info: config.clone(),
            pty_pair,
            child_process,
        })
    }

    /// 读取PTY输出
    pub fn read_output(&mut self) -> Result<String> {
        let mut buffer = vec![0u8; 8192];
        let mut reader = self.pty_pair.master().try_clone_reader()?;
        
        match reader.read(&mut buffer) {
            Ok(bytes_read) if bytes_read > 0 => {
                let output = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                Ok(output)
            }
            Ok(_) => Ok(String::new()), // 没有数据
            Err(e) => Err(anyhow::anyhow!("读取PTY输出失败: {}", e)),
        }
    }

    /// 发送命令到PTY
    pub fn send_command(&mut self, command: &str) -> Result<()> {
        let connection_id = format!(
            "{}@{}:{}",
            self.connection_info.username, 
            self.connection_info.host, 
            self.connection_info.port
        );

        log_ssh_command_execution(command, &connection_id);

        let mut writer = self.pty_pair.master().take_writer()?;
        let command_with_newline = format!("{}\n", command);
        writer.write_all(command_with_newline.as_bytes())?;
        writer.flush()?;

        Ok(())
    }

    /// 获取初始输出（连接成功后的欢迎信息等）
    pub async fn get_initial_output(&mut self) -> Result<String> {
        // 等待SSH连接建立和初始输出
        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
        
        let mut output = String::new();
        
        // 尝试读取多次以获取完整的初始输出
        for _ in 0..5 {
            match self.read_output() {
                Ok(data) if !data.is_empty() => {
                    output.push_str(&data);
                }
                _ => break,
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        if !output.is_empty() {
            crate::app_log!(info, "SSH", "获取到shell初始输出: {}", output.trim());
        }

        Ok(output)
    }

    pub fn get_info(&self) -> &ConnectionConfig {
        &self.connection_info
    }

    /// 检查连接是否仍然活跃
    pub fn is_alive(&self) -> bool {
        // 检查子进程是否还在运行
        match self.child_process.try_wait() {
            Ok(None) => true,  // 进程仍在运行
            _ => false,        // 进程已退出或检查失败
        }
    }
}

/// SSH连接管理器
#[derive(Debug)]
pub struct SshManager {
    connections: HashMap<String, Arc<Mutex<PtySshConnection>>>,
}

impl SshManager {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    pub async fn connect(&mut self, id: String, config: &ConnectionConfig) -> Result<()> {
        let connection = PtySshConnection::connect(config).await?;
        self.connections.insert(id, Arc::new(Mutex::new(connection)));
        Ok(())
    }

    /// 获取shell会话初始输出
    pub async fn get_shell_initial_output(&self, id: &str) -> Result<String> {
        if let Some(connection) = self.connections.get(id) {
            let mut conn = connection.lock().await;
            conn.get_initial_output().await
        } else {
            let error_msg = format!("连接不存在: {}", id);
            Err(anyhow::anyhow!(error_msg))
        }
    }

    /// 发送命令并读取输出
    pub async fn execute_command(&self, id: &str, command: &str) -> Result<String> {
        if let Some(connection) = self.connections.get(id) {
            let mut conn = connection.lock().await;
            
            // 发送命令
            conn.send_command(command)?;
            
            // 等待命令执行
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            
            // 读取输出
            let mut output = String::new();
            for _ in 0..10 { // 最多尝试10次
                match conn.read_output() {
                    Ok(data) if !data.is_empty() => {
                        output.push_str(&data);
                    }
                    _ => break,
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            let connection_id = format!(
                "{}@{}:{}",
                conn.connection_info.username,
                conn.connection_info.host,
                conn.connection_info.port
            );
            log_ssh_command_success(command, &connection_id, output.len());

            Ok(output)
        } else {
            Err(anyhow::anyhow!("连接不存在: {}", id))
        }
    }

    /// 持续读取输出（用于实时显示）
    pub async fn read_continuous_output(&self, id: &str) -> Result<String> {
        if let Some(connection) = self.connections.get(id) {
            let mut conn = connection.lock().await;
            conn.read_output()
        } else {
            Err(anyhow::anyhow!("连接不存在: {}", id))
        }
    }

    pub fn disconnect(&mut self, id: &str) {
        if self.connections.contains_key(id) {
            log_ssh_disconnection(id, "用户主动断开连接");
            self.connections.remove(id);
            crate::app_log!(info, "SSH", "连接 '{}' 已断开", id);
        } else {
            crate::app_log!(warn, "SSH", "尝试断开不存在的连接: '{}'", id);
        }
    }

    pub fn is_connected(&self, id: &str) -> bool {
        if let Some(connection) = self.connections.get(id) {
            if let Ok(conn) = connection.try_lock() {
                conn.is_alive()
            } else {
                true // 如果无法获取锁，假设连接存在
            }
        } else {
            false
        }
    }

    pub fn get_connections(&self) -> Vec<String> {
        self.connections.keys().cloned().collect()
    }
}