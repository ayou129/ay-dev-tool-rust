use anyhow::Result;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;


use crate::ui::{AuthType, ConnectionConfig};

/// 完全同步的SSH连接 - 真正简单的实现
pub struct SyncSshConnection {
    pub config: ConnectionConfig,
    writer: Box<dyn Write + Send>,
    reader: Box<dyn Read + Send>,
    child_process: Box<dyn portable_pty::Child + Send + Sync>,
    pub is_connected: bool,
    password_sent: bool,
}

impl std::fmt::Debug for SyncSshConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncSshConnection")
            .field("config", &self.config)
            .field("is_connected", &self.is_connected)
            .field("password_sent", &self.password_sent)
            .finish()
    }
}

impl SyncSshConnection {
    /// 创建同步SSH连接
    pub fn create(config: &ConnectionConfig) -> Result<Self> {
        crate::app_log!(info, "SSH", "创建同步SSH连接: {}@{}:{}", config.username, config.host, config.port);
        
        // 创建PTY系统
        let pty_system = native_pty_system();
        
        // 创建PTY对
        let pty_pair = pty_system.openpty(PtySize {
            rows: 30,
            cols: 120,
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
        
        // 启动SSH进程
        let child_process = pty_pair.slave.spawn_command(ssh_cmd)
            .map_err(|e| {
                let error_msg = format!("启动SSH进程失败: {}", e);
                crate::app_log!(error, "SSH", "{}", error_msg);
                anyhow::anyhow!(error_msg)
            })?;

        // 获取writer和reader
        let writer = pty_pair.master.take_writer()
            .map_err(|e| anyhow::anyhow!("获取PTY writer失败: {}", e))?;
            
        let reader = pty_pair.master.try_clone_reader()
            .map_err(|e| anyhow::anyhow!("获取PTY reader失败: {}", e))?;

        // 设置非阻塞模式
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = reader.as_raw_fd();
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }

        crate::app_log!(info, "SSH", "同步SSH连接创建成功");

        Ok(Self {
            config: config.clone(),
            writer,
            reader,
            child_process,
            is_connected: true,
            password_sent: false,
        })
    }

    /// 同步发送命令
    pub fn send_command(&mut self, command: &str) -> Result<()> {
        crate::app_log!(info, "SSH", "同步发送命令: {}", command);
        
        let command_with_newline = format!("{}\r\n", command);
        self.writer.write_all(command_with_newline.as_bytes())?;
        self.writer.flush()?;
        
        crate::app_log!(info, "SSH", "命令发送完成: {}", command);
        Ok(())
    }

    /// 同步读取输出（非阻塞）
    pub fn read_output(&mut self) -> Result<String> {
        let mut buffer = [0u8; 4096];
        
        match self.reader.read(&mut buffer) {
            Ok(n) if n > 0 => {
                let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                
                // 处理密码输入
                if !self.password_sent && self.config.auth_type == AuthType::Password {
                    if let Some(password) = &self.config.password {
                        if data.contains("Password") || data.contains("password") || data.contains("Password:") {
                            crate::app_log!(info, "SSH", "检测到密码提示，发送密码");
                            let password_with_newline = format!("{}\r\n", password);
                            self.writer.write_all(password_with_newline.as_bytes())?;
                            self.writer.flush()?;
                            self.password_sent = true;
                            crate::app_log!(info, "SSH", "密码已发送");
                        }
                    }
                }
                
                if n > 0 {
                    crate::app_log!(debug, "SSH", "同步读取到数据: {} 字节", n);
                }
                Ok(data)
            }
            Ok(_) => Ok(String::new()), // 没有数据
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                Ok(String::new()) // 非阻塞模式下没有数据可读
            }
            Err(e) => Err(anyhow::anyhow!("读取PTY数据失败: {}", e))
        }
    }

    /// 断开连接
    pub fn disconnect(&mut self) -> Result<()> {
        crate::app_log!(info, "SSH", "断开同步SSH连接");
        
        if let Err(e) = self.child_process.kill() {
            crate::app_log!(warn, "SSH", "终止SSH进程失败: {}", e);
        } else {
            crate::app_log!(info, "SSH", "SSH进程已终止");
        }
        
        self.is_connected = false;
        Ok(())
    }
}

impl Drop for SyncSshConnection {
    fn drop(&mut self) {
        if self.is_connected {
            let _ = self.disconnect();
        }
    }
}

/// 完全同步的SSH管理器
#[derive(Debug)]
pub struct SyncSshManager {
    connections: Mutex<HashMap<String, SyncSshConnection>>,
}

impl SyncSshManager {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
        }
    }

    /// 创建SSH连接（同步）
    pub fn create_connection(&self, id: String, config: &ConnectionConfig) -> Result<()> {
        crate::app_log!(info, "SSH", "创建同步SSH连接: {}", id);
        
        let connection = SyncSshConnection::create(config)?;
        let mut connections = self.connections.lock().map_err(|_| anyhow::anyhow!("Failed to acquire lock"))?;
        connections.insert(id.clone(), connection);
        
        crate::app_log!(info, "SSH", "同步SSH连接已添加: {}", id);
        Ok(())
    }

    /// 执行命令（同步）
    pub fn execute_command(&self, id: &str, command: &str) -> Result<()> {
        crate::app_log!(info, "SSH", "同步执行命令 '{}' 在连接 '{}'", command, id);
        
        let mut connections = self.connections.lock().map_err(|_| anyhow::anyhow!("Failed to acquire lock"))?;
        if let Some(connection) = connections.get_mut(id) {
            connection.send_command(command)
        } else {
            Err(anyhow::anyhow!("连接不存在: {}", id))
        }
    }

    /// 读取连接的输出（同步）
    pub fn read_output(&self, id: &str) -> Result<String> {
        let mut connections = self.connections.lock().map_err(|_| anyhow::anyhow!("Failed to acquire lock"))?;
        if let Some(connection) = connections.get_mut(id) {
            connection.read_output()
        } else {
            Err(anyhow::anyhow!("连接不存在: {}", id))
        }
    }

    /// 断开连接
    pub fn disconnect(&self, id: &str) {
        crate::app_log!(info, "SSH", "断开连接: {}", id);
        if let Ok(mut connections) = self.connections.lock() {
            if let Some(mut connection) = connections.remove(id) {
                let _ = connection.disconnect();
            }
        }
    }

    /// 检查连接状态
    pub fn is_connected(&self, id: &str) -> bool {
        let connections = match self.connections.lock() {
            Ok(conn) => conn,
            Err(_) => return false,
        };
        connections.get(id).map_or(false, |conn| conn.is_connected)
    }
}
