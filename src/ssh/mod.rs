use anyhow::Result;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::Mutex;

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

fn log_ssh_command_success(command: &str, _connection_id: &str, output_len: usize) {
    crate::app_log!(info, "SSH", "命令 '{}' 执行成功，输出长度: {} 字符", command, output_len);
}

fn log_ssh_disconnection(connection_id: &str, reason: &str) {
    crate::app_log!(info, "SSH", "断开连接 '{}': {}", connection_id, reason);
}

/// PTY SSH连接
pub struct PtySshConnection {
    connection_info: ConnectionConfig,
    pty_pair: portable_pty::PtyPair,
    child_process: Box<dyn portable_pty::Child + Send + Sync>,
}

impl std::fmt::Debug for PtySshConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtySshConnection")
            .field("connection_info", &self.connection_info)
            .field("child_process", &"<Child Process>")
            .finish()
    }
}

impl PtySshConnection {
    pub async fn connect(config: &ConnectionConfig) -> Result<Self> {
        
        // 创建PTY系统
        let pty_system = native_pty_system();
        
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
        ssh_cmd.arg("-o");
        ssh_cmd.arg("StrictHostKeyChecking=no"); // 跳过主机密钥检查（开发用）
        
        // 端口设置
        if config.port != 22 {
            ssh_cmd.arg("-p");
            ssh_cmd.arg(config.port.to_string());
        }
        
        // 认证方式
        match &config.auth_type {
            AuthType::Password => {
                log_ssh_authentication_method(&config.username, "密码认证");
                // 密码认证：稍后在PTY中自动输入密码
            }
            AuthType::PublicKey => {
                log_ssh_authentication_method(&config.username, "公钥认证");
                if let Some(key_file) = &config.key_file {
                    ssh_cmd.arg("-i");
                    ssh_cmd.arg(key_file.as_ref() as &str);
                }
            }
        }
        
        // 目标主机
        ssh_cmd.arg(format!("{}@{}", config.username, config.host));
        
        // 启动SSH进程
        let child_process = pty_pair.slave.spawn_command(ssh_cmd)
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
        let mut reader = self.pty_pair.master.try_clone_reader()?;
        
        match reader.read(&mut buffer) {
            Ok(bytes_read) if bytes_read > 0 => {
                let output = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                crate::app_log!(debug, "SSH", "读取到 {} 字节数据: {:?}", bytes_read, output.trim());
                Ok(output)
            }
            Ok(_) => {
                crate::app_log!(debug, "SSH", "PTY读取返回0字节");
                Ok(String::new()) // 没有数据
            }
            Err(e) => {
                crate::app_log!(error, "SSH", "读取PTY输出失败: {}", e);
                Err(anyhow::anyhow!("读取PTY输出失败: {}", e))
            }
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

        let mut writer = self.pty_pair.master.take_writer()?;
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



    /// 检查连接是否仍然活跃
    pub fn is_alive(&mut self) -> bool {
        // 检查子进程是否还在运行
        match self.child_process.try_wait() {
            Ok(None) => true,  // 进程仍在运行
            _ => false,        // 进程已退出或检查失败
        }
    }
}

/// SSH连接管理器 - 只对连接集合加锁，连接本身不加锁
#[derive(Debug)]
pub struct SshManager {
    connections: Arc<Mutex<HashMap<String, PtySshConnection>>>,
}

impl SshManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn connect(&self, id: String, config: &ConnectionConfig) -> Result<()> {
        let connection = PtySshConnection::connect(config).await?;
        let mut connections = self.connections.lock().await;
        connections.insert(id, connection);
        Ok(())
    }

    /// 启动SSH数据流读取（非阻塞）
    pub async fn get_shell_initial_output(&self, id: &str) -> Result<String> {
        crate::app_log!(info, "SSH", "启动SSH连接的数据流，连接ID: {}", id);
        
        // 启动后台任务持续读取PTY数据
        let connections = self.connections.clone();
        let id_clone = id.to_string();
        
        tokio::spawn(async move {
            let mut password_sent = false;
            // 给SSH连接一点时间建立
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            
            loop {
                let data = {
                    let mut connections = connections.lock().await;
                    if let Some(connection) = connections.get_mut(&id_clone) {
                        connection.read_output()
                    } else {
                        crate::app_log!(warn, "SSH", "连接 {} 已不存在，停止数据读取任务", id_clone);
                        break;
                    }
                };
                
                match data {
                    Ok(data) if !data.is_empty() => {
                        crate::app_log!(info, "SSH", "后台任务读取到PTY数据: {} 字节", data.len());
                        
                        // 处理密码认证
                        if !password_sent && (data.contains("Password:") || data.contains("password:")) {
                            let connection_config = {
                                let connections = connections.lock().await;
                                if let Some(connection) = connections.get(&id_clone) {
                                    connection.connection_info.clone()
                                } else {
                                    break;
                                }
                            };
                            
                            if connection_config.auth_type == AuthType::Password {
                                if let Some(password) = &connection_config.password {
                                    crate::app_log!(info, "SSH", "后台任务：自动输入密码");
                                    let mut connections = connections.lock().await;
                                    if let Some(connection) = connections.get_mut(&id_clone) {
                                        if let Ok(mut writer) = connection.pty_pair.master.take_writer() {
                                            let _ = writer.write_all(format!("{}\n", password).as_bytes());
                                            let _ = writer.flush();
                                            password_sent = true;
                                        }
                                    }
                                }
                            }
                        }
                        
                        // TODO: 这里需要将数据发送给UI
                        // 暂时只打印日志，后续实现UI数据传输
                        crate::app_log!(info, "PTY_STREAM", "实时数据: {:?}", data.trim());
                    }
                    Ok(_) => {
                        // 没有数据，短暂等待
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    Err(_) => {
                        // 读取错误，短暂等待后继续
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
            }
        });
        
        // 立即返回，不阻塞UI
        Ok("SSH连接已建立，后台数据流已启动".to_string())
    }

    /// 发送命令并读取输出
    pub async fn execute_command(&self, id: &str, command: &str) -> Result<String> {
        // 第一步：发送命令
        {
            let mut connections = self.connections.lock().await;
            if let Some(connection) = connections.get_mut(id) {
                connection.send_command(command)?;
            } else {
                return Err(anyhow::anyhow!("连接不存在: {}", id));
            }
        } // connections锁在这里被释放
        
        // 等待命令执行（不持有锁）
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        
        // 第二步：读取输出（分批获取锁，减少持有时间）
        let mut output = String::new();
        for _ in 0..10 { // 最多尝试10次
            let data = {
                let mut connections = self.connections.lock().await;
                if let Some(connection) = connections.get_mut(id) {
                    connection.read_output()
                } else {
                    return Err(anyhow::anyhow!("连接不存在: {}", id));
                }
            }; // connections锁在每次循环后被释放
            
            match data {
                Ok(data) if !data.is_empty() => {
                    output.push_str(&data);
                }
                _ => break,
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        // 记录日志（不需要锁）
        crate::app_log!(info, "SSH", "命令 '{}' 执行成功，输出长度: {} 字符", command, output.len());

        Ok(output)
    }



    pub async fn disconnect(&self, id: &str) {
        let mut connections = self.connections.lock().await;
        if connections.contains_key(id) {
            log_ssh_disconnection(id, "用户主动断开连接");
            connections.remove(id);
            crate::app_log!(info, "SSH", "连接 '{}' 已断开", id);
        } else {
            crate::app_log!(warn, "SSH", "尝试断开不存在的连接: '{}'", id);
        }
    }

    pub async fn is_connected(&self, id: &str) -> bool {
        let mut connections = self.connections.lock().await;
        if let Some(connection) = connections.get_mut(id) {
            connection.is_alive()
        } else {
            false
        }
    }

    pub async fn get_connections(&self) -> Vec<String> {
        let connections = self.connections.lock().await;
        connections.keys().cloned().collect()
    }

    /// 获取连接配置信息
    pub async fn get_connection_info(&self, id: &str) -> Option<ConnectionConfig> {
        let connections = self.connections.lock().await;
        if let Some(connection) = connections.get(id) {
            Some(connection.connection_info.clone())
        } else {
            None
        }
    }
}