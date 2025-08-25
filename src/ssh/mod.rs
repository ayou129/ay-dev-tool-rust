use anyhow::Result;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::ui::{AuthType, ConnectionConfig};

/// SSH命令消息
#[derive(Debug, Clone)]
pub struct SshCommand {
    pub command: String,
    pub response_sender: Option<mpsc::UnboundedSender<Result<String>>>,
}

/// SSH数据消息  
#[derive(Debug, Clone)]
pub struct SshData {
    pub data: String,
    pub connection_id: String,
}

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

fn log_ssh_disconnection(connection_id: &str, reason: &str) {
    crate::app_log!(info, "SSH", "断开连接 '{}': {}", connection_id, reason);
}

/// 消息传递式SSH连接 - 不再直接持有PTY对象
pub struct SshConnection {
    pub connection_info: ConnectionConfig,
    pub command_sender: mpsc::UnboundedSender<SshCommand>,
    pub is_connected: bool,
}

/// PTY连接的后台任务数据
pub struct PtyBackgroundTask {
    pty_pair: portable_pty::PtyPair,
    child_process: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Option<Box<dyn std::io::Write + Send>>,
    command_receiver: mpsc::UnboundedReceiver<SshCommand>,
    data_sender: mpsc::UnboundedSender<crate::ui::terminal_panel::CommandResult>,
}

impl std::fmt::Debug for SshConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshConnection")
            .field("connection_info", &self.connection_info)
            .field("is_connected", &self.is_connected)
            .finish()
    }
}

impl SshConnection {
    /// 创建新的SSH连接，返回连接对象和后台任务
    pub async fn create(
        config: &ConnectionConfig,
        data_sender: mpsc::UnboundedSender<crate::ui::terminal_panel::CommandResult>,
    ) -> Result<(Self, PtyBackgroundTask)> {
        
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
        ssh_cmd.args(&[
            "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=/dev/null", 
            "-p", &config.port.to_string(),
            &format!("{}@{}", config.username, config.host)
        ]);
        
        log_ssh_authentication_method(&config.username, 
            match config.auth_type {
                AuthType::Password => "密码认证",
                AuthType::PublicKey => "公钥认证",
            }
        );
        
        // 启动SSH进程
        let child_process = pty_pair.slave.spawn_command(ssh_cmd)
            .map_err(|e| {
                let error_msg = format!("启动SSH进程失败: {}", e);
                log_ssh_connection_failed(&config.host, config.port, &config.username, &error_msg);
                anyhow::anyhow!(error_msg)
            })?;

        log_ssh_connection_success(&config.host, config.port, &config.username);

        // 获取writer
        let writer = match pty_pair.master.take_writer() {
            Ok(w) => {
                crate::app_log!(info, "SSH", "成功获取PTY writer");
                Some(w)
            }
            Err(e) => {
                crate::app_log!(error, "SSH", "获取PTY writer失败: {}", e);
                None
            }
        };

        // 创建命令通道
        let (command_sender, command_receiver) = mpsc::unbounded_channel();

        // 创建连接对象
        let connection = Self {
            connection_info: config.clone(),
            command_sender,
            is_connected: true,
        };

        // 创建后台任务
        let background_task = PtyBackgroundTask {
            pty_pair,
            child_process,
            writer,
            command_receiver,
            data_sender,
        };

        Ok((connection, background_task))
    }

    /// 发送命令到SSH连接
    pub async fn send_command(&self, command: &str) -> Result<()> {
        let ssh_command = SshCommand {
            command: command.to_string(),
            response_sender: None,
        };
        
        self.command_sender.send(ssh_command)
            .map_err(|e| anyhow::anyhow!("发送命令失败: {}", e))?;
        
        Ok(())
    }
}

impl PtyBackgroundTask {
    /// 运行后台任务，处理PTY读写
    pub async fn run(mut self, connection_config: ConnectionConfig) {
        crate::app_log!(info, "SSH", "启动SSH后台任务");
        
        // 创建PTY数据通道
        let (pty_data_tx, mut pty_data_rx) = mpsc::unbounded_channel();
        
        // 启动独立的PTY读取任务
        if let Ok(mut reader) = self.pty_pair.master.try_clone_reader() {
            tokio::spawn(async move {
                let mut buffer = [0u8; 8192];
                crate::app_log!(info, "SSH", "PTY读取任务启动");
                loop {
                    match reader.read(&mut buffer) {
                        Ok(n) if n > 0 => {
                            let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                            crate::app_log!(debug, "SSH", "PTY读取到数据: {} 字节", n);
                            if pty_data_tx.send(data).is_err() {
                                crate::app_log!(warn, "SSH", "PTY数据发送失败，接收端已关闭");
                                break;
                            }
                        }
                        Ok(_) => {
                            crate::app_log!(info, "SSH", "PTY读取结束 (EOF)");
                            break;
                        }
                        Err(e) => {
                            crate::app_log!(error, "SSH", "PTY读取错误: {}", e);
                            break;
                        }
                    }
                }
                crate::app_log!(info, "SSH", "PTY读取任务结束");
            });
        } else {
            crate::app_log!(error, "SSH", "无法创建PTY reader，数据读取将不可用");
        }
        
        let mut password_sent = false;
        crate::app_log!(debug, "SSH", "准备进入主循环");
        
        // 主事件循环
        let mut loop_count = 0;
        loop {
            loop_count += 1;
            // 优化的轮询方案：先处理PTY数据，再处理命令
            // 这样既保证了响应性，又避免了select!的竞争问题
            
            // 先非阻塞检查PTY数据（优先级最高）
            match pty_data_rx.try_recv() {
                Ok(data) => {
                    crate::app_log!(info, "SSH", "后台任务：收到PTY数据");
                    if let Err(e) = self.handle_pty_data(data, &connection_config, &mut password_sent).await {
                        crate::app_log!(error, "SSH", "处理PTY数据失败: {}", e);
                    }
                    continue; // 立即进入下一次循环，继续处理可能的更多数据
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // 没有PTY数据，继续检查命令
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    crate::app_log!(info, "SSH", "PTY数据通道关闭");
                    break;
                }
            }
            
            // 再非阻塞检查命令
            match self.command_receiver.try_recv() {
                Ok(ssh_command) => {
                    crate::app_log!(debug, "SSH", "后台任务：收到命令消息");
                    crate::app_log!(debug, "SSH", "后台任务：准备处理命令: {}", ssh_command.command);
                    if let Err(e) = self.handle_command(&ssh_command).await {
                        crate::app_log!(error, "SSH", "处理命令失败: {}", e);
                    }
                    continue; // 处理完命令后继续下一次循环
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // 没有命令，等待一小段时间后继续
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    crate::app_log!(info, "SSH", "命令通道关闭，退出后台任务");
                    break;
                }
            }
            
            // 如果都没有数据，短暂休眠避免CPU空转（优化为5ms）
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        
        // 清理子进程
        if let Err(e) = self.child_process.kill() {
            crate::app_log!(warn, "SSH", "终止SSH进程失败: {}", e);
        } else {
            crate::app_log!(info, "SSH", "SSH进程已终止");
        }
        
        crate::app_log!(info, "SSH", "SSH后台任务结束");
    }
    
    async fn handle_command(&mut self, command: &SshCommand) -> Result<()> {
        crate::app_log!(info, "SSH", "处理命令: {}", command.command);
        
        let result = if let Some(ref mut writer) = self.writer {
            let command_with_newline = format!("{}\r\n", command.command);
            match writer.write_all(command_with_newline.as_bytes()) {
                Ok(_) => {
                    writer.flush()?;
                    crate::app_log!(info, "SSH", "命令已发送: {}", command.command);
                    Ok("".to_string()) // 返回空字符串，避免显示内部状态
                }
                Err(e) => Err(anyhow::anyhow!("写入命令失败: {}", e))
            }
        } else {
            Err(anyhow::anyhow!("PTY writer不可用"))
        };
        
        // 如果有响应发送器，发送结果
        if let Some(sender) = &command.response_sender {
            let response = match &result {
                Ok(msg) => Ok(msg.clone()),
                Err(e) => Err(anyhow::anyhow!("{}", e))
            };
            let _ = sender.send(response);
        }
        
        result.map(|_| ())
    }
    
    async fn handle_pty_data(&mut self, data: String, connection_config: &ConnectionConfig, password_sent: &mut bool) -> Result<()> {
        crate::app_log!(info, "SSH", "处理PTY数据: {} 字节", data.len());
        
        // 处理密码输入（从原来的handle_pty_read移过来）
        if !*password_sent && connection_config.auth_type == AuthType::Password {
            if let Some(password) = &connection_config.password {
                let needs_password = data.contains("Password") 
                    || data.contains("password") 
                    || data.contains("Password:");
                
                if needs_password {
                    crate::app_log!(info, "SSH", "检测到密码提示，发送密码");
                    if let Some(ref mut writer) = self.writer {
                        let _ = writer.write_all(format!("{}\r\n", password).as_bytes());
                        let _ = writer.flush();
                        *password_sent = true;
                        crate::app_log!(info, "SSH", "密码已发送");
                    }
                }
            }
        }
        
        // 发送数据到UI
        let ssh_data = SshData {
            data: data.clone(),
            connection_id: "current".to_string(),
        };
        crate::app_log!(info, "SSH", "发送SSH数据到UI: {} 字节，连接ID: {}", ssh_data.data.len(), ssh_data.connection_id);
        
        let _ = self.data_sender.send(crate::ui::terminal_panel::CommandResult {
            command: "pty_stream".to_string(),
            output: Ok(data),
        });
        
        Ok(())
    }
}

/// SSH连接管理器 - 使用消息传递架构
#[derive(Debug)]
pub struct SshManager {
    connections: Arc<Mutex<HashMap<String, SshConnection>>>,
}

impl SshManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn connect(&self, id: String, config: &ConnectionConfig) -> Result<()> {
        // 获取数据发送器（这里需要从UI传入）
        let (data_sender, _) = mpsc::unbounded_channel();
        
        let (connection, background_task) = SshConnection::create(config, data_sender).await?;
        
        // 启动后台任务
        let config_clone = config.clone();
        tokio::spawn(async move {
            background_task.run(config_clone).await;
        });
        
        // 保存连接
        let mut connections = self.connections.lock().await;
        connections.insert(id, connection);
        
        Ok(())
    }

    /// 创建连接并返回数据接收器
    pub async fn create_connection(&self, id: String, config: &ConnectionConfig, data_sender: mpsc::UnboundedSender<crate::ui::terminal_panel::CommandResult>) -> Result<()> {
        let (connection, background_task) = SshConnection::create(config, data_sender).await?;
        
        // 启动后台任务并添加错误处理
        let config_clone = config.clone();
        let id_clone = id.clone();
        let task_handle = tokio::spawn(async move {
            crate::app_log!(info, "SSH", "后台任务开始执行，连接ID: {}", id_clone);
            background_task.run(config_clone).await;
            crate::app_log!(info, "SSH", "后台任务正常结束，连接ID: {}", id_clone);
        });
        
        // 监控任务状态
        let id_monitor = id.clone();
        tokio::spawn(async move {
            if let Err(e) = task_handle.await {
                crate::app_log!(error, "SSH", "后台任务崩溃，连接ID: {}: {}", id_monitor, e);
            }
        });
        
        // 保存连接
        let mut connections = self.connections.lock().await;
        connections.insert(id, connection);
        
        Ok(())
    }

    /// 执行命令 - 现在使用消息传递
    pub async fn execute_command(&self, id: &str, command: &str) -> Result<String> {
        log_ssh_command_execution(command, id);
        crate::app_log!(info, "SSH", "发送命令到PTY: {}", command);
        
        let connections = self.connections.lock().await;
        if let Some(connection) = connections.get(id) {
            connection.send_command(command).await?;
            crate::app_log!(info, "SSH", "命令 '{}' 已发送到PTY", command);
            Ok("".to_string()) // 返回空字符串，避免显示内部状态
        } else {
            crate::app_log!(error, "SSH", "连接不存在: {}", id);
            Err(anyhow::anyhow!("连接不存在: {}", id))
        }
    }

    /// 启动SSH数据流读取（兼容旧API）
    pub async fn get_shell_initial_output(&self, id: &str, _data_sender: Option<mpsc::UnboundedSender<crate::ui::terminal_panel::CommandResult>>) -> Result<String> {
        crate::app_log!(info, "SSH", "启动SSH连接的数据流，连接ID: {}", id);
        // 返回空字符串，因为真正的shell输出会通过PTY读取任务自动发送到UI
        // 这样避免内部状态消息显示在终端内容区域
        Ok("".to_string())
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
        let connections = self.connections.lock().await;
        connections.get(id).map(|c| c.is_connected).unwrap_or(false)
    }

    pub async fn get_connections(&self) -> Vec<String> {
        let connections = self.connections.lock().await;
        connections.keys().cloned().collect()
    }

    pub async fn get_connection_info(&self, id: &str) -> Option<ConnectionConfig> {
        let connections = self.connections.lock().await;
        connections.get(id).map(|c| c.connection_info.clone())
    }
}