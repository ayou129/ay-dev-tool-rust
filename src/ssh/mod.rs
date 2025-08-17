use anyhow::Result;
use ssh2::Session;
use std::collections::HashMap;
use std::io::prelude::*;
use std::net::TcpStream;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::ui::ConnectionConfig;
use crate::utils::logger::{
    log_ssh_connection_attempt, log_ssh_connection_success, log_ssh_connection_failed,
    log_ssh_command_execution, log_ssh_command_success, log_ssh_command_failed,
    log_ssh_disconnection, log_ssh_authentication_method
};

pub struct SshConnection {
    session: Session,
    stream: TcpStream,
    connection_info: ConnectionConfig,
}

impl std::fmt::Debug for SshConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshConnection")
            .field("connection_info", &self.connection_info)
            .finish_non_exhaustive()
    }
}

impl SshConnection {
    pub async fn connect(config: &ConnectionConfig) -> Result<Self> {
        // 记录连接尝试
        log_ssh_connection_attempt(&config.host, config.port, &config.username);
        
        let tcp = match TcpStream::connect(format!("{}:{}", config.host, config.port)) {
            Ok(stream) => {
                crate::app_log!(debug, "SSH", "TCP连接建立成功: {}:{}", config.host, config.port);
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
                    session.userauth_password(&config.username, password)
                        .map_err(|e| anyhow::anyhow!("密码认证失败: {}", e))
                } else {
                    Err(anyhow::anyhow!("密码认证需要密码"))
                }
            }
            crate::ui::AuthType::PublicKey => {
                log_ssh_authentication_method(&config.username, "公钥认证");
                if let Some(key_file) = &config.key_file {
                    session.userauth_pubkey_file(
                        &config.username,
                        None,
                        key_file.as_ref(),
                        None,
                    ).map_err(|e| anyhow::anyhow!("公钥认证失败: {}", e))
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

        Ok(Self {
            session,
            stream: tcp,
            connection_info: config.clone(),
        })
    }

    pub async fn execute_command(&mut self, command: &str) -> Result<String> {
        let connection_id = format!("{}@{}:{}", 
            self.connection_info.username, 
            self.connection_info.host, 
            self.connection_info.port);
        
        log_ssh_command_execution(command, &connection_id);

        let result = || -> Result<String> {
            let mut channel = self.session.channel_session()?;
            channel.exec(command)?;

            let mut output = String::new();
            channel.read_to_string(&mut output)?;
            channel.wait_close()?;

            let exit_status = channel.exit_status()?;
            
            if exit_status == 0 {
                log_ssh_command_success(command, &connection_id, output.len());
            } else {
                let error_msg = format!("命令退出状态: {}", exit_status);
                log_ssh_command_failed(command, &connection_id, &error_msg);
            }

            Ok(output)
        }();

        match &result {
            Ok(output) => {
                crate::app_log!(debug, "SSH", "命令执行成功: '{}' -> {} 字符", command, output.len());
            }
            Err(e) => {
                log_ssh_command_failed(command, &connection_id, &e.to_string());
            }
        }

        result
    }

    pub fn get_info(&self) -> &ConnectionConfig {
        &self.connection_info
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
}

