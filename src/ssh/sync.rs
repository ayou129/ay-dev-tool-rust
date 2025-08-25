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

        // 🔧 Windows特殊方案：使用echo命令向SSH管道传递密码
        let mut ssh_cmd = if cfg!(windows) && config.auth_type == AuthType::Password && config.password.is_some() {
            // Windows方案：使用cmd /c "echo password | ssh ..."
            let mut cmd = CommandBuilder::new("cmd");
            let password = config.password.as_ref().unwrap();
            let ssh_command = format!(
                "echo {} | ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o PasswordAuthentication=yes -o PubkeyAuthentication=no -o PreferredAuthentications=password -o ConnectTimeout=30 -p {} {}@{}",
                password,
                config.port,
                config.username,
                config.host
            );
            cmd.args(&["/c", &ssh_command]);
            crate::app_log!(info, "SSH", "使用Windows cmd + echo方案进行自动密码认证");
            cmd
        } else {
            // 标准SSH命令
            let mut cmd = CommandBuilder::new("ssh");
            cmd.args(&[
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
                "-o", "PasswordAuthentication=yes",
                "-o", "PubkeyAuthentication=no",
                "-o", "PreferredAuthentications=password",
                "-o", "ConnectTimeout=30",
                "-p", &config.port.to_string(),
                &format!("{}@{}", config.username, config.host)
            ]);
            crate::app_log!(info, "SSH", "使用标准SSH命令进行连接");
            cmd
        };
        
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
        
        // Windows平台暂时不设置非阻塞模式
        #[cfg(windows)]
        {
            crate::app_log!(info, "SSH", "Windows平台：跳过非阻塞模式设置");
        }

        crate::app_log!(info, "SSH", "同步SSH连接创建成功");
        
        // 🔑 如果使用了Windows echo方案，密码已经自动发送
        let password_already_sent = cfg!(windows) && config.auth_type == AuthType::Password && config.password.is_some();
        
        // 🔑 Windows关键修复：等待管道完全建立
        #[cfg(windows)]
        {
            use std::time::Duration;
            use std::thread;
            crate::app_log!(info, "SSH", "Windows平台：等待SSH管道建立完成");
            thread::sleep(Duration::from_millis(2000)); // 增加到2秒等待时间
        }

        Ok(Self {
            config: config.clone(),
            writer,
            reader,
            child_process,
            is_connected: true,
            password_sent: password_already_sent, // 🔑 根据是否使用echo方案决定
        })
    }

    /// 同步发送命令 - 改进错误处理
    pub fn send_command(&mut self, command: &str) -> Result<()> {
        if !self.is_connected {
            return Err(anyhow::anyhow!("SSH连接已断开"));
        }
        
        crate::app_log!(info, "SSH", "发送SSH命令: {}", command);
        
        let command_with_newline = format!("{}\r\n", command);
        
        match self.writer.write_all(command_with_newline.as_bytes()) {
            Ok(_) => {
                match self.writer.flush() {
                    Ok(_) => {
                        crate::app_log!(info, "SSH", "命令发送成功: {}", command);
                        Ok(())
                    }
                    Err(e) => {
                        crate::app_log!(error, "SSH", "命令flush失败: {}", e);
                        self.is_connected = false;
                        Err(anyhow::anyhow!("命令flush失败: {}", e))
                    }
                }
            }
            Err(e) => {
                crate::app_log!(error, "SSH", "命令发送失败: {}", e);
                self.is_connected = false;
                Err(anyhow::anyhow!("命令发送失败: {}", e))
            }
        }
    }

    /// 同步读取输出（非阻塞）- Windows优化版本
    pub fn read_output(&mut self) -> Result<String> {
        let mut buffer = [0u8; 4096];
        
        match self.reader.read(&mut buffer) {
            Ok(n) if n > 0 => {
                let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                
                // 🔑 关键：记录所有SSH输出，无论内容是什么
                crate::app_log!(info, "SSH", "读取到PTY数据: {} 字节，内容: {:?}", n, data);
                
                // 🔍 检查认证相关信息
                if data.contains("Permission denied") {
                    crate::app_log!(error, "SSH", "认证失败：权限被拒绝");
                } else if data.contains("Authentication failed") {
                    crate::app_log!(error, "SSH", "认证失败：认证失败");
                } else if data.contains("Login incorrect") {
                    crate::app_log!(error, "SSH", "认证失败：登录信息错误");
                } else if data.contains("Last login") {
                    crate::app_log!(info, "SSH", "认证成功：检测到登录信息");
                } else if data.contains("Welcome") || data.contains("$") || data.contains("#") {
                    crate::app_log!(info, "SSH", "认证成功：检测到Shell提示符");
                }
                
                self.handle_password_prompt(&data)?;
                Ok(data)
            }
            Ok(_) => {
                // 没有数据
                Ok(String::new())
            }
            Err(e) => {
                match e.kind() {
                    std::io::ErrorKind::WouldBlock => {
                        // 非阻塞模式下没有数据可读
                        Ok(String::new())
                    }
                    std::io::ErrorKind::BrokenPipe => {
                        // 管道断开，连接已关闭
                        crate::app_log!(error, "SSH", "🚨 SSH连接管道断开: {}，这通常表示认证失败或服务器拒绝连接", e);
                        self.is_connected = false;
                        // 🔑 重要：管道断开时也返回错误信息，而不是空字符串
                        Ok(format!("连接已断开: {}", e))
                    }
                    _ => {
                        // 其他错误，记录但不抛出，避免中断整个流程
                        crate::app_log!(warn, "SSH", "PTY读取警告: {}", e);
                        Ok(String::new())
                    }
                }
            }
        }
    }
    
    /// 处理密码提示的独立方法
    fn handle_password_prompt(&mut self, data: &str) -> Result<()> {
        // 🔍 详细记录SSH交互信息
        crate::app_log!(debug, "SSH", "处理SSH数据: {}", data.replace("\r", "\\r").replace("\n", "\\n"));
        
        if !self.password_sent && self.config.auth_type == AuthType::Password {
            if let Some(password) = &self.config.password {
                // 🔑 更宽泛的密码提示检测
                let has_password_prompt = data.contains("Password") 
                    || data.contains("password") 
                    || data.contains("Password:")
                    || data.contains("password:")
                    || data.to_lowercase().contains("password");
                    
                if has_password_prompt {
                    crate::app_log!(info, "SSH", "🔑 检测到密码提示，等待3秒后发送密码进行认证");
                    
                    // 🕑 等待3秒，让SSH客户端的stdin管道完全准备好
                    std::thread::sleep(std::time::Duration::from_millis(3000));
                    
                    let password_with_newline = format!("{}\r\n", password);
                    
                    crate::app_log!(info, "SSH", "现在尝试发送密码...");
                    
                    match self.writer.write_all(password_with_newline.as_bytes()) {
                        Ok(_) => {
                            match self.writer.flush() {
                                Ok(_) => {
                                    self.password_sent = true;
                                    crate::app_log!(info, "SSH", "✅ 密码发送成功，等待认证结果...");
                                }
                                Err(e) => {
                                    crate::app_log!(error, "SSH", "密码flush失败: {}", e);
                                    return Err(anyhow::anyhow!("密码flush失败: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            crate::app_log!(error, "SSH", "密码发送失败: {}", e);
                            return Err(anyhow::anyhow!("密码发送失败: {}", e));
                        }
                    }
                }
            }
        }
        Ok(())
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
            let result = connection.read_output();
            
            // 🔍 记录读取结果
            match &result {
                Ok(data) if !data.is_empty() => {
                    crate::app_log!(info, "SshManager", "从连接 {} 读取到数据: {} 字节", id, data.len());
                }
                Ok(_) => {
                    // 空数据，不记录以避免日志垃圾
                }
                Err(e) => {
                    crate::app_log!(warn, "SshManager", "从连接 {} 读取数据失败: {}", id, e);
                }
            }
            
            result
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
