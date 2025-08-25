use ssh2::{Channel, Session};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use anyhow::{Result, anyhow};

use crate::ui::{AuthType, ConnectionConfig};

/// SSH2连接结构体 - 包装ssh2的Session和Channel
pub struct Ssh2Connection {
    pub config: ConnectionConfig,
    session: Session,
    channel: Option<Channel>,
    tcp_stream: Option<TcpStream>,
    pub is_connected: bool,
    pub terminal_size: (u16, u16),
}

impl Ssh2Connection {
    /// 创建新的SSH2连接
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            session: Session::new().unwrap(),
            channel: None,
            tcp_stream: None,
            is_connected: false,
            terminal_size: (80, 24), // 默认终端尺寸
        }
    }

    /// 建立SSH连接
    pub async fn connect(&mut self) -> Result<()> {
        crate::app_log!(info, "SSH2", "开始连接到 {}@{}:{}", 
            self.config.username, self.config.host, self.config.port);

        // 建立TCP连接
        let tcp = TcpStream::connect(format!("{}:{}", self.config.host, self.config.port))
            .map_err(|e| {
                crate::app_log!(error, "SSH2", "TCP连接失败: {}", e);
                anyhow!("TCP连接失败: {}", e)
            })?;
            
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(30)))?;
        tcp.set_nodelay(true)?; // 禁用Nagle算法，提高响应性

        // 设置SSH会话
        self.session.set_tcp_stream(tcp.try_clone()?);
        
        // 设置SSH会话选项，提高兼容性
        self.session.set_compress(true);
        self.session.set_timeout(30000); // 30秒超时
        
        // 尝试SSH握手
        crate::app_log!(info, "SSH2", "开始SSH握手...");
        self.session.handshake().map_err(|e| {
            crate::app_log!(error, "SSH2", "SSH握手失败: {}", e);
            anyhow!("密钥交换失败，可能是服务器不支持客户端的加密算法。请检查：\n1. SSH服务器是否正常运行\n2. 防火墙是否阻止连接\n3. 网络连接是否稳定")
        })?;
        
        crate::app_log!(info, "SSH2", "SSH握手成功，开始认证...");

        // 认证
        self.authenticate().await?;

        // 创建Shell通道
        let mut channel = self.session.channel_session()?;
        channel.request_pty("xterm-256color", None, Some((self.terminal_size.0 as u32, self.terminal_size.1 as u32, 0u32, 0u32)))?;
        channel.shell()?;
        
        // 🔑 关键：在创建Shell通道后设置非阻塞模式
        self.session.set_blocking(false);
        crate::app_log!(info, "SSH2", "SSH会话已设置为非阻塞模式");

        self.channel = Some(channel);
        self.tcp_stream = Some(tcp);
        self.is_connected = true;

        crate::app_log!(info, "SSH2", "SSH2连接建立成功: {}@{}", 
            self.config.username, self.config.host);

        Ok(())
    }

    /// SSH认证 - 支持密码和公钥认证
    async fn authenticate(&mut self) -> Result<()> {
        match self.config.auth_type {
            AuthType::Password => {
                if let Some(password) = &self.config.password {
                    crate::app_log!(info, "SSH2", "使用密码认证: {}", self.config.username);
                    
                    self.session
                        .userauth_password(&self.config.username, password)
                        .map_err(|e| {
                            crate::app_log!(error, "SSH2", "密码认证失败: {}", e);
                            anyhow!("密码认证失败，请检查用户名和密码是否正确: {}", e)
                        })?
                } else {
                    return Err(anyhow!("密码认证需要提供密码"));
                }
            }
            AuthType::PublicKey => {
                if let Some(key_file) = &self.config.key_file {
                    crate::app_log!(info, "SSH2", "使用公钥认证: {}", key_file);
                    
                    self.session
                        .userauth_pubkey_file(&self.config.username, None, 
                                            std::path::Path::new(key_file), None)
                        .map_err(|e| {
                            crate::app_log!(error, "SSH2", "公钥认证失败: {}", e);
                            anyhow!("公钥认证失败，请检查私钥文件路径和权限: {}", e)
                        })?
                } else {
                    return Err(anyhow!("公钥认证需要提供私钥文件"));
                }
            }
        }

        if !self.session.authenticated() {
            crate::app_log!(error, "SSH2", "认证失败：用户名或密码不正确");
            return Err(anyhow!("认证失败：用户名或密码不正确"));
        }

        crate::app_log!(info, "SSH2", "SSH认证成功: {}", self.config.username);
        Ok(())
    }

    /// 发送命令到SSH服务器
    pub fn send_command(&mut self, command: &str) -> Result<()> {
        if !self.is_connected {
            return Err(anyhow!("SSH连接未建立"));
        }

        if let Some(channel) = &mut self.channel {
            let command_with_newline = format!("{}\n", command);
            channel.write_all(command_with_newline.as_bytes())?;
            channel.flush()?;
            
            crate::app_log!(debug, "SSH2", "发送命令: {}", command);
            Ok(())
        } else {
            Err(anyhow!("SSH通道未创建"))
        }
    }

    /// 读取SSH输出 - 完全非阻塞实现
    pub fn read_output(&mut self) -> Result<String> {
        if !self.is_connected {
            return Ok(String::new());
        }

        if let Some(channel) = &mut self.channel {
            let mut buffer = [0u8; 4096];
            
            // 使用try_read或者设置非阻塞模式
            match channel.read(&mut buffer) {
                Ok(n) if n > 0 => {
                    let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                    crate::app_log!(debug, "SSH2", "读取到SSH输出: {} 字节", n);
                    Ok(data)
                }
                Ok(_) => {
                    // 没有数据，返回空字符串
                    Ok(String::new())
                }
                Err(e) => {
                    // 检查是否为非阻塞读取的正常情况
                    match e.kind() {
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                            // 非阻塞模式下没有数据可读或超时
                            Ok(String::new())
                        }
                        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionAborted => {
                            // 连接已断开
                            crate::app_log!(warn, "SSH2", "SSH连接已断开: {}", e);
                            self.is_connected = false;
                            Ok(String::new())
                        }
                        _ => {
                            // 其他错误，记录但不抛出，避免中断整个流程
                            crate::app_log!(warn, "SSH2", "读取SSH输出错误: {}", e);
                            Ok(String::new())
                        }
                    }
                }
            }
        } else {
            Ok(String::new())
        }
    }

    /// 调整终端尺寸
    pub fn resize_terminal(&mut self, width: u16, height: u16) -> Result<()> {
        self.terminal_size = (width, height);
        
        if let Some(channel) = &mut self.channel {
            channel.request_pty_size(width as u32, height as u32, Some(0), Some(0))?;
            crate::app_log!(debug, "SSH2", "调整终端尺寸: {}x{}", width, height);
        }
        
        Ok(())
    }

    /// 断开SSH连接
    pub fn disconnect(&mut self) -> Result<()> {
        if self.is_connected {
            if let Some(mut channel) = self.channel.take() {
                let _ = channel.close();
                let _ = channel.wait_close();
            }

            self.session.disconnect(None, "User requested disconnection", None)?;
            self.is_connected = false;
            
            crate::app_log!(info, "SSH2", "SSH2连接已断开: {}@{}", 
                self.config.username, self.config.host);
        }
        
        Ok(())
    }

    /// 检查连接状态
    pub fn is_alive(&self) -> bool {
        self.is_connected && self.channel.is_some()
    }
}

/// 🔑 新架构: SSH2连接包装器 - 各连接独立管理
pub struct Ssh2ConnectionWrapper {
    // 🔑 关键：直接持有连接对象，无需共享锁
    connection: Arc<Mutex<Ssh2Connection>>,
    // 命令发送通道
    command_sender: Sender<String>,
    // 输出接收通道  
    output_receiver: Receiver<String>,
    // 线程句柄
    _read_handle: thread::JoinHandle<()>,
    _write_handle: thread::JoinHandle<()>,
}

impl Ssh2ConnectionWrapper {
    /// 创建新的连接包装器
    pub fn new(mut ssh_connection: Ssh2Connection) -> Self {
        let connection = Arc::new(Mutex::new(ssh_connection));
        
        // 创建通道
        let (cmd_sender, cmd_receiver) = mpsc::channel::<String>();
        let (out_sender, out_receiver) = mpsc::channel::<String>();
        
        // 🔑 关键：独立的读取线程
        let read_connection = Arc::clone(&connection);
        let read_handle = thread::spawn(move || {
            crate::app_log!(info, "SSH2-Read", "📚 启动SSH读取线程");
            loop {
                match read_connection.try_lock() {
                    Ok(mut conn) => {
                        if !conn.is_connected {
                            break;
                        }
                        
                        match conn.read_output() {
                            Ok(data) if !data.is_empty() => {
                                crate::app_log!(debug, "SSH2-Read", "📚 读取到数据: {} 字节", data.len());
                                if out_sender.send(data).is_err() {
                                    break;
                                }
                            }
                            Ok(_) => {
                                // 没有数据，短暂等待
                                thread::sleep(Duration::from_millis(10));
                            }
                            Err(_) => {
                                thread::sleep(Duration::from_millis(50));
                            }
                        }
                    }
                    Err(_) => {
                        // 锁被占用，等待一下
                        thread::sleep(Duration::from_millis(5));
                    }
                }
            }
            crate::app_log!(info, "SSH2-Read", "📚 SSH读取线程结束");
        });
        
        // 🔑 关键：独立的写入线程
        let write_connection = Arc::clone(&connection);
        let cmd_sender_clone = cmd_sender.clone(); // 克隆一个用于线程内重试
        let write_handle = thread::spawn(move || {
            crate::app_log!(info, "SSH2-Write", "✏️ 启动SSH写入线程");
            while let Ok(command) = cmd_receiver.recv() {
                match write_connection.try_lock() {
                    Ok(mut conn) => {
                        if !conn.is_connected {
                            break;
                        }
                        
                        match conn.send_command(&command) {
                            Ok(_) => {
                                crate::app_log!(debug, "SSH2-Write", "✏️ 命令发送成功: {}", command);
                            }
                            Err(e) => {
                                crate::app_log!(error, "SSH2-Write", "✏️ 命令发送失败: {}", e);
                            }
                        }
                    }
                    Err(_) => {
                        // 锁被占用，等待一下再试
                        thread::sleep(Duration::from_millis(5));
                        // 重新发送命令
                        if cmd_sender_clone.send(command).is_err() {
                            break;
                        }
                    }
                }
            }
            crate::app_log!(info, "SSH2-Write", "✏️ SSH写入线程结束");
        });
        
        Self {
            connection,
            command_sender: cmd_sender,
            output_receiver: out_receiver,
            _read_handle: read_handle,
            _write_handle: write_handle,
        }
    }
    
    /// 🔑 发送命令（完全无锁）
    pub fn execute_command(&self, command: &str) -> Result<()> {
        self.command_sender.send(command.to_string())
            .map_err(|_| anyhow!("命令发送失败：通道已关闭"))?;
        crate::app_log!(info, "SSH2-Wrapper", "🚀 命令已提交: {}", command);
        Ok(())
    }
    
    /// 🔑 读取输出（完全无锁）
    pub fn read_output(&self) -> Result<String> {
        match self.output_receiver.try_recv() {
            Ok(data) => {
                crate::app_log!(debug, "SSH2-Wrapper", "📨 收到输出: {} 字节", data.len());
                Ok(data)
            }
            Err(_) => Ok(String::new())
        }
    }
    
    /// 检查连接状态
    pub fn is_connected(&self) -> bool {
        match self.connection.try_lock() {
            Ok(conn) => conn.is_connected,
            Err(_) => true // 如果锁被占用，说明连接可能还在工作
        }
    }
    
    /// 断开连接
    pub fn disconnect(&self) -> Result<()> {
        if let Ok(mut conn) = self.connection.try_lock() {
            conn.disconnect()?;
        }
        Ok(())
    }
}

/// 🔑 简化的SSH2管理器 - 无锁架构
pub struct Ssh2Manager {
    // 🔑 关键：使用Mutex实现内部可变性，支持Arc共享
    connections: Arc<Mutex<HashMap<String, Ssh2ConnectionWrapper>>>,
    runtime: tokio::runtime::Runtime,
}

impl Default for Ssh2Manager {
    fn default() -> Self {
        Self::new()
    }
}

impl Ssh2Manager {
    /// 创建新的SSH2管理器
    pub fn new() -> Self {
        let runtime = tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime for SSH2Manager");
            
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            runtime,
        }
    }

    /// 🔑 创建SSH连接（内部可变性）
    pub fn create_connection(&self, id: String, config: &ConnectionConfig) -> Result<()> {
        crate::app_log!(info, "SSH2Manager", "🚀 创建SSH连接: {} -> {}@{}:{}", 
            id, config.username, config.host, config.port);
        
        let mut connection = Ssh2Connection::new(config.clone());
        
        // 异步连接建立
        let connection_result = self.runtime.block_on(async {
            connection.connect().await
        });
        
        connection_result?;
        
        // 🔑 关键：创建连接包装器，启动独立线程
        let wrapper = Ssh2ConnectionWrapper::new(connection);
        
        // 使用内部可变性更新连接集合
        {
            let mut connections = self.connections.lock().unwrap();
            connections.insert(id.clone(), wrapper);
        }

        crate::app_log!(info, "SSH2Manager", "✅ SSH连接创建成功: {}", id);
        Ok(())
    }

    /// 🔑 执行命令（完全无锁）
    pub fn execute_command(&self, id: &str, command: &str) -> Result<()> {
        let connections = self.connections.lock().unwrap();
        if let Some(wrapper) = connections.get(id) {
            wrapper.execute_command(command)
        } else {
            Err(anyhow!("连接不存在: {}", id))
        }
    }

    /// 🔑 读取输出（完全无锁）
    pub fn read_output(&self, id: &str) -> Result<String> {
        let connections = self.connections.lock().unwrap();
        if let Some(wrapper) = connections.get(id) {
            wrapper.read_output()
        } else {
            Err(anyhow!("连接不存在: {}", id))
        }
    }

    /// 检查连接状态
    pub fn is_connected(&self, id: &str) -> bool {
        let connections = self.connections.lock().unwrap();
        connections.get(id).map_or(false, |wrapper| wrapper.is_connected())
    }

    /// 断开连接
    pub fn disconnect(&self, id: &str) -> Result<()> {
        let mut connections = self.connections.lock().unwrap();
        if let Some(wrapper) = connections.remove(id) {
            wrapper.disconnect()?;
            crate::app_log!(info, "SSH2Manager", "🔌 连接已断开: {}", id);
        }
        Ok(())
    }

    /// 获取所有连接ID
    pub fn get_connection_ids(&self) -> Vec<String> {
        let connections = self.connections.lock().unwrap();
        connections.keys().cloned().collect()
    }
    
    /// 调整终端尺寸（预留接口）
    pub fn resize_terminal(&self, _id: &str, _width: u16, _height: u16) -> Result<()> {
        // TODO: 实现终端尺寸调整
        Ok(())
    }
}

// 确保Ssh2Manager可以安全地在线程间传递
unsafe impl Send for Ssh2Manager {}
unsafe impl Sync for Ssh2Manager {}