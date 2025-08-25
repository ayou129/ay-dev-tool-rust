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

/// 🎭 Actor模式 - SSH消息类型
#[derive(Debug, Clone)]
pub enum SshMessage {
    /// 发送命令到SSH服务器
    SendCommand(String),
    /// 读取SSH输出数据
    ReadOutput,
    /// 断开SSH连接
    Disconnect,
    /// 检查连接状态
    CheckStatus,
}

/// 🎭 Actor模式 - SSH响应类型  
pub enum SshResponse {
    /// 命令执行结果
    CommandResult(Result<()>),
    /// SSH输出数据
    OutputData(String),
    /// 连接状态
    ConnectionStatus(bool),
    /// 错误信息
    Error(String),
}

/// 🎭 SSH Actor - 独占管理一个SSH连接（Actor模式核心）
pub struct SshActor {
    /// SSH连接实例（Actor独占访问）
    connection: Ssh2Connection,
    /// 消息接收器 - 接收来自外部的操作请求
    message_receiver: Receiver<SshMessage>,
    /// 输出发送器 - 向UI发送SSH输出数据
    output_sender: Sender<String>,
    /// 响应发送器 - 发送操作结果
    response_sender: Option<Sender<SshResponse>>,
}

impl SshActor {
    /// 创建SSH Actor
    pub fn new(
        connection: Ssh2Connection,
        message_receiver: Receiver<SshMessage>,
        output_sender: Sender<String>,
    ) -> Self {
        Self {
            connection,
            message_receiver,
            output_sender,
            response_sender: None,
        }
    }
    
    /// Actor主循环 - 处理消息和管理SSH连接
    pub fn run(mut self) {
        crate::app_log!(info, "SshActor", "🎭 启动SSH Actor主循环");
        
        // 主消息处理循环，同时处理输出读取
        loop {
            // 非阻塞读取SSH输出
            if let Ok(output) = self.connection.read_output() {
                if !output.is_empty() {
                    if let Err(_) = self.output_sender.send(output) {
                        crate::app_log!(warn, "SshActor", "🎭 输出发送失败，接收器已关闭");
                        break;
                    }
                }
            }
            
            // 非阻塞接收消息，给出Some(超时时间)
            match self.message_receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(message) => {
                    match message {
                        SshMessage::SendCommand(cmd) => {
                            self.handle_send_command(&cmd);
                        }
                        SshMessage::ReadOutput => {
                            // 输出在上面的循环中处理
                        }
                        SshMessage::CheckStatus => {
                            self.handle_check_status();
                        }
                        SshMessage::Disconnect => {
                            crate::app_log!(info, "SshActor", "🎭 收到断开请求，退出Actor");
                            break;
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // 超时是正常情况，继续循环
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    crate::app_log!(info, "SshActor", "🎭 消息通道已断开，退出Actor");
                    break;
                }
            }
        }
        
        // 清理资源
        self.cleanup();
        crate::app_log!(info, "SshActor", "🎭 SSH Actor主循环结束");
    }
    
    /// 处理发送命令
    fn handle_send_command(&mut self, command: &str) {
        match self.connection.send_command(command) {
            Ok(_) => {
                crate::app_log!(debug, "SshActor", "🎭 命令发送成功: {}", command);
            }
            Err(e) => {
                crate::app_log!(error, "SshActor", "🎭 命令发送失败: {}", e);
            }
        }
    }
    
    /// 处理状态检查
    fn handle_check_status(&self) {
        // 可以添加状态检查逻辑
        crate::app_log!(debug, "SshActor", "🎭 连接状态: {}", self.connection.is_connected);
    }
    
    /// 清理资源
    fn cleanup(&mut self) {
        if let Err(e) = self.connection.disconnect() {
            crate::app_log!(error, "SshActor", "🎭 断开连接失败: {}", e);
        }
    }
}

/// 🎭 Actor句柄 - 用于与Actor通信
pub struct SshActorHandle {
    /// 消息发送器 - 向Actor发送操作请求
    message_sender: Sender<SshMessage>,
    /// 输出接收器 - 接收来自Actor的SSH输出
    output_receiver: Receiver<String>,
    /// Actor线程句柄
    _actor_handle: thread::JoinHandle<()>,
}

impl SshActorHandle {
    /// 创建SSH Actor和对应的句柄
    pub fn spawn(connection: Ssh2Connection) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel::<SshMessage>();
        let (out_tx, out_rx) = mpsc::channel::<String>();
        
        let actor = SshActor::new(connection, msg_rx, out_tx);
        let actor_handle = thread::spawn(move || {
            actor.run();
        });
        
        Self {
            message_sender: msg_tx,
            output_receiver: out_rx,
            _actor_handle: actor_handle,
        }
    }
    
    /// 发送命令到SSH Actor
    pub fn execute_command(&self, command: &str) -> Result<()> {
        self.message_sender
            .send(SshMessage::SendCommand(command.to_string()))
            .map_err(|_| anyhow!("命令发送失败：Actor已关闭"))?;
        crate::app_log!(info, "SshActorHandle", "🚀 命令已提交给Actor: {}", command);
        Ok(())
    }
    
    /// 从 SSH Actor 读取输出
    pub fn read_output(&self) -> Result<String> {
        match self.output_receiver.try_recv() {
            Ok(data) => {
                crate::app_log!(debug, "SshActorHandle", "📨 从Actor收到输出: {} 字节", data.len());
                Ok(data)
            }
            Err(_) => Ok(String::new())
        }
    }
    
    /// 断开SSH Actor
    pub fn disconnect(&self) -> Result<()> {
        self.message_sender
            .send(SshMessage::Disconnect)
            .map_err(|_| anyhow!("断开请求发送失败：Actor已关闭"))?;
        Ok(())
    }
}

/// SSH2连接结构体 - 简化版本（被Actor管理）
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
        
        // 🔑 关键：优化的写入线程 - 减少锁竞争
        let write_connection = Arc::clone(&connection);
        let write_handle = thread::spawn(move || {
            crate::app_log!(info, "SSH2-Write", "✏️ 启动SSH写入线程");
            while let Ok(command) = cmd_receiver.recv() {
                // 🔑 简化策略：减少重试次数，增加等待时间
                let mut retry_count = 0;
                let max_retries = 20; // 减少最大重试次数
                
                loop {
                    match write_connection.try_lock() {
                        Ok(mut conn) => {
                            if !conn.is_connected {
                                break;
                            }
                            
                            match conn.send_command(&command) {
                                Ok(_) => {
                                    crate::app_log!(debug, "SSH2-Write", "✏️ 命令发送成功: {}", command);
                                    break; // 成功，退出重试循环
                                }
                                Err(e) => {
                                    crate::app_log!(error, "SSH2-Write", "✏️ 命令发送失败: {}", e);
                                    break; // 发送失败，退出重试循环
                                }
                            }
                        }
                        Err(_) => {
                            retry_count += 1;
                            if retry_count >= max_retries {
                                crate::app_log!(warn, "SSH2-Write", "✏️ 命令发送超时，放弃: {}", command);
                                break;
                            }
                            
                            // 🔑 简化：固定5ms等待，减少CPU使用
                            thread::sleep(Duration::from_millis(5));
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

/// 🔑 简化的SSH2管理器 - Actor模式架构
pub struct Ssh2Manager {
    // 🔑 关键：使用Actor句柄管理SSH连接，彻底消除锁竞争
    connections: Arc<Mutex<HashMap<String, SshActorHandle>>>,
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
        
        // 🔑 关键：创建 SSH Actor 句柄，彻底消除锁竞争
        let actor_handle = SshActorHandle::spawn(connection);
        
        // 使用内部可变性更新连接集合
        {
            let mut connections = self.connections.lock().unwrap();
            connections.insert(id.clone(), actor_handle);
        }

        crate::app_log!(info, "SSH2Manager", "✅ SSH连接创建成功: {}", id);
        Ok(())
    }

    /// 🔑 执行命令（Actor模式）
    pub fn execute_command(&self, id: &str, command: &str) -> Result<()> {
        let connections = self.connections.lock().unwrap();
        if let Some(actor_handle) = connections.get(id) {
            actor_handle.execute_command(command)
        } else {
            Err(anyhow!("连接不存在: {}", id))
        }
    }

    /// 🔑 读取输出（Actor模式）
    pub fn read_output(&self, id: &str) -> Result<String> {
        let connections = self.connections.lock().unwrap();
        if let Some(actor_handle) = connections.get(id) {
            actor_handle.read_output()
        } else {
            Err(anyhow!("连接不存在: {}", id))
        }
    }

    /// 检查连接状态
    pub fn is_connected(&self, id: &str) -> bool {
        let connections = self.connections.lock().unwrap();
        connections.get(id).map_or(false, |_actor_handle| {
            // TODO: 实现Actor的连接状态检查
            true // 暂时返回true，后续实现
        })
    }

    /// 断开连接
    pub fn disconnect(&self, id: &str) -> Result<()> {
        let mut connections = self.connections.lock().unwrap();
        if let Some(actor_handle) = connections.remove(id) {
            actor_handle.disconnect()?;
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