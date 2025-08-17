use anyhow::Result;
use ssh2::Session;
use std::collections::HashMap;
use std::io::prelude::*;
use std::net::TcpStream;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::ui::ConnectionConfig;

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
        let tcp = TcpStream::connect(format!("{}:{}", config.host, config.port))?;
        let mut session = Session::new()?;
        session.set_tcp_stream(tcp.try_clone()?);
        session.handshake()?;
        
        // 根据认证类型进行认证
        match &config.auth_type {
            crate::ui::AuthType::Password => {
                if let Some(password) = &config.password {
                    session.userauth_password(&config.username, password)?;
                } else {
                    return Err(anyhow::anyhow!("密码认证需要密码"));
                }
            }
            crate::ui::AuthType::PublicKey => {
                if let Some(key_file) = &config.key_file {
                    session.userauth_pubkey_file(&config.username, None, key_file.as_ref(), None)?;
                } else {
                    return Err(anyhow::anyhow!("公钥认证需要私钥文件"));
                }
            }
        }

        Ok(Self {
            session,
            stream: tcp,
            connection_info: config.clone(),
        })
    }

    pub async fn execute_command(&mut self, command: &str) -> Result<String> {
        let mut channel = self.session.channel_session()?;
        channel.exec(command)?;
        
        let mut output = String::new();
        channel.read_to_string(&mut output)?;
        channel.wait_close()?;
        
        Ok(output)
    }

    pub fn get_info(&self) -> &ConnectionConfig {
        &self.connection_info
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
        self.connections.insert(id, Arc::new(Mutex::new(connection)));
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
        self.connections.remove(id);
    }

    pub fn is_connected(&self, id: &str) -> bool {
        self.connections.contains_key(id)
    }

    pub fn get_connections(&self) -> Vec<String> {
        self.connections.keys().cloned().collect()
    }
}
