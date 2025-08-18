use anyhow::Result;
use ssh2::Session;
use std::collections::HashMap;
use std::io::prelude::*;
use std::net::TcpStream;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::ui::ConnectionConfig;
use crate::utils::logger::{
    log_ssh_authentication_method, log_ssh_command_execution, log_ssh_command_failed,
    log_ssh_command_success, log_ssh_connection_failed, log_ssh_connection_success,
    log_ssh_disconnection,
};

pub struct SshConnection {
    session: Session,
    stream: TcpStream,
    connection_info: ConnectionConfig,
    // ✅ 持久的shell channel - 真正的终端会话
    shell_channel: Option<ssh2::Channel>,
}

impl std::fmt::Debug for SshConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshConnection")
            .field("connection_info", &self.connection_info)
            .field("has_shell_channel", &self.shell_channel.is_some())
            .finish_non_exhaustive()
    }
}

impl SshConnection {
    pub async fn connect(config: &ConnectionConfig) -> Result<Self> {
        // 移除连接尝试日志 - 冗余，有成功/失败日志即可

        let tcp = match TcpStream::connect(format!("{}:{}", config.host, config.port)) {
            Ok(stream) => {
                crate::app_log!(
                    debug,
                    "SSH",
                    "TCP连接建立成功: {}:{}",
                    config.host,
                    config.port
                );
                stream
            }
            Err(e) => {
                let error_msg = format!("TCP连接失败: {}", e);
                log_ssh_connection_failed(&config.host, config.port, &config.username, &error_msg);
                return Err(anyhow::anyhow!(error_msg));
            }
        };

        let mut session = Session::new()?;
        session.set_tcp_stream(tcp.try_clone()?);

        if let Err(e) = session.handshake() {
            let error_msg = format!("SSH握手失败: {}", e);
            log_ssh_connection_failed(&config.host, config.port, &config.username, &error_msg);
            return Err(anyhow::anyhow!(error_msg));
        }

        // 根据认证类型进行认证
        let auth_result = match &config.auth_type {
            crate::ui::AuthType::Password => {
                log_ssh_authentication_method(&config.username, "密码认证");
                if let Some(password) = &config.password {
                    session
                        .userauth_password(&config.username, password)
                        .map_err(|e| anyhow::anyhow!("密码认证失败: {}", e))
                } else {
                    Err(anyhow::anyhow!("密码认证需要密码"))
                }
            }
            crate::ui::AuthType::PublicKey => {
                log_ssh_authentication_method(&config.username, "公钥认证");
                if let Some(key_file) = &config.key_file {
                    session
                        .userauth_pubkey_file(&config.username, None, key_file.as_ref(), None)
                        .map_err(|e| anyhow::anyhow!("公钥认证失败: {}", e))
                } else {
                    Err(anyhow::anyhow!("公钥认证需要私钥文件"))
                }
            }
        };

        // 检查认证结果
        if let Err(e) = auth_result {
            log_ssh_connection_failed(&config.host, config.port, &config.username, &e.to_string());
            return Err(e);
        }

        // 验证用户是否已认证
        if !session.authenticated() {
            let error_msg = "用户认证失败";
            log_ssh_connection_failed(&config.host, config.port, &config.username, error_msg);
            return Err(anyhow::anyhow!(error_msg));
        }

        log_ssh_connection_success(&config.host, config.port, &config.username);

        // ✅ 立即创建持久的shell channel
        let mut shell_channel = session.channel_session()?;
        // 使用PTY请求设置TERM
        shell_channel.request_pty("xterm-256color", None, None)?;
        // 使用setenv在会话环境中设置编码（如果服务端允许）
        if let Err(e) = shell_channel.setenv("LANG", "en_US.UTF-8") {
            crate::app_log!(warn, "SSH", "设置 LANG 环境变量失败: {}", e);
        }
        if let Err(e) = shell_channel.setenv("LC_ALL", "en_US.UTF-8") {
            crate::app_log!(warn, "SSH", "设置 LC_ALL 环境变量失败: {}", e);
        }
        shell_channel.shell()?;
        crate::app_log!(info, "SSH", "已创建持久shell channel (TERM=xterm-256color, LANG/LC_ALL 通过 setenv 尝试设置)");

        Ok(Self {
            session,
            stream: tcp,
            connection_info: config.clone(),
            shell_channel: Some(shell_channel),
        })
    }

    pub async fn execute_command(&mut self, command: &str) -> Result<String> {
        let connection_id = format!(
            "{}@{}:{}",
            self.connection_info.username, self.connection_info.host, self.connection_info.port
        );

        log_ssh_command_execution(command, &connection_id);

        // 🔥 新设计：每个命令使用独立的channel，像iTerm2一样
        let mut channel = self.session.channel_session()?;
        
        // 设置环境以保持一致性
        channel.request_pty("xterm-256color", None, None)?;
        if let Err(_) = channel.setenv("LANG", "en_US.UTF-8") {
            // 忽略setenv失败，某些服务器不支持
        }
        if let Err(_) = channel.setenv("LC_ALL", "en_US.UTF-8") {
            // 忽略setenv失败
        }
        
        // 直接执行命令（不是shell模式）
        channel.exec(command)?;
        
        crate::app_log!(debug, "SSH", "创建独立channel执行命令: {}", command);

        // 使用SSH2的标准读取方式
        let mut output = String::new();
        
        // 使用BufReader进行高效读取
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(&mut channel);
        let mut line = String::new();
        
        crate::app_log!(debug, "SSH", "开始读取命令输出");
        
        // 逐行读取直到EOF
        while reader.read_line(&mut line)? > 0 {
            output.push_str(&line);
            crate::app_log!(debug, "SSH", "读取一行: {} 字节", line.len());
            line.clear();
        }
        
        crate::app_log!(debug, "SSH", "读取完成，等待通道关闭");
        
        // 等待命令执行完毕
        channel.wait_close()?;
        let exit_status = channel.exit_status().unwrap_or(-1);
        
        crate::app_log!(debug, "SSH", "命令执行完成，退出状态: {}", exit_status);

        log_ssh_command_success(command, &connection_id, output.len());
        crate::app_log!(debug, "SSH", "命令执行成功: '{}' -> {} 字符", command, output.len());
        
        Ok(output)
    }

    pub fn get_info(&self) -> &ConnectionConfig {
        &self.connection_info
    }

    // 获取SSH会话建立后的初始输出（包括Last login等信息）
    pub async fn get_shell_initial_output(&mut self) -> Result<String> {
        let connection_id = format!(
            "{}@{}:{}",
            self.connection_info.username, self.connection_info.host, self.connection_info.port
        );

        crate::app_log!(info, "SSH", "获取shell初始输出: {}", connection_id);

        // 创建临时通道获取初始输出
        let mut channel = self.session.channel_session()?;
        channel.request_pty("xterm-256color", None, None)?;
        // 优先尝试通过 setenv 设置编码
        if let Err(e) = channel.setenv("LANG", "en_US.UTF-8") {
            crate::app_log!(warn, "SSH", "初始输出通道设置 LANG 失败: {}", e);
        }
        if let Err(e) = channel.setenv("LC_ALL", "en_US.UTF-8") {
            crate::app_log!(warn, "SSH", "初始输出通道设置 LC_ALL 失败: {}", e);
        }
        channel.shell()?;

        // 等待服务器发送初始数据
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let mut output = String::new();
        let mut buffer = vec![0; 8192];

        // 尝试读取所有可用数据
        match channel.read(&mut buffer) {
            Ok(bytes_read) => {
                if bytes_read > 0 {
                    let text = String::from_utf8_lossy(&buffer[..bytes_read]);
                    crate::app_log!(info, "SSH", "读取到初始输出 {} 字节", bytes_read);
                    output.push_str(&text);
                } else {
                    // 没有初始输出，发送换行符获取提示符
                    crate::app_log!(info, "SSH", "无初始输出，发送换行符获取提示符");
                    let _ = channel.write_all(b"\n");
                    let _ = channel.flush();
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                    if let Ok(bytes) = channel.read(&mut buffer) {
                        if bytes > 0 {
                            let text = String::from_utf8_lossy(&buffer[..bytes]);
                            output.push_str(&text);
                        }
                    }
                }
            }
            Err(e) => {
                crate::app_log!(warn, "SSH", "读取初始输出失败: {}", e);
            }
        }

        // 优雅关闭通道，忽略关闭错误
        let _ = channel.close();
        let _ = channel.wait_close();

        crate::app_log!(info, "SSH", "完成，输出长度: {} 字符", output.len());
        Ok(output)
    }

    // 检查TCP连接状态
    pub fn is_alive(&self) -> bool {
        // 尝试读取TCP流的状态来判断连接是否仍然活跃
        // 这里使用stream的peer_addr方法来检查连接状态
        self.stream.peer_addr().is_ok()
    }
}

#[derive(Debug)]
pub struct SshManager {
    connections: HashMap<String, Arc<Mutex<SshConnection>>>,
}

impl SshManager {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    pub async fn connect(&mut self, id: String, config: &ConnectionConfig) -> Result<()> {
        let connection = SshConnection::connect(config).await?;
        self.connections
            .insert(id, Arc::new(Mutex::new(connection)));
        Ok(())
    }

    // 获取shell会话初始输出
    pub async fn get_shell_initial_output(&self, id: &str) -> Result<String> {
        crate::app_log!(
            info,
            "SSH",
            "SshManager.get_shell_initial_output 被调用，id: {}",
            id
        );
        crate::app_log!(
            info,
            "SSH",
            "当前连接数: {}, 连接列表: {:?}",
            self.connections.len(),
            self.connections.keys().collect::<Vec<_>>()
        );

        if let Some(connection) = self.connections.get(id) {
            crate::app_log!(info, "SSH", "找到连接 {}, 开始获取shell输出", id);
            let mut conn = connection.lock().await;
            conn.get_shell_initial_output().await
        } else {
            let error_msg = format!("连接不存在: {}", id);
            crate::app_log!(error, "SSH", "{}", error_msg);
            Err(anyhow::anyhow!(error_msg))
        }
    }

    pub async fn execute_command(&self, id: &str, command: &str) -> Result<String> {
        if let Some(connection) = self.connections.get(id) {
            let mut conn = connection.lock().await;
            conn.execute_command(command).await
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
            // 尝试检查连接是否真正活跃
            if let Ok(conn) = connection.try_lock() {
                conn.is_alive()
            } else {
                // 如果无法获取锁，假设连接存在
                true
            }
        } else {
            false
        }
    }

    pub fn get_connections(&self) -> Vec<String> {
        self.connections.keys().cloned().collect()
    }

    // 获取连接信息
    pub fn get_connection_info(&self, id: &str) -> Option<ConnectionConfig> {
        if let Some(connection) = self.connections.get(id) {
            if let Ok(conn) = connection.try_lock() {
                Some(conn.get_info().clone())
            } else {
                None
            }
        } else {
            None
        }
    }

    // 获取初始shell输出
}
