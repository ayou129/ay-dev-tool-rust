pub mod connection_manager;
pub mod terminal_panel;
pub mod plugins_panel;


use serde::{Deserialize, Serialize};

pub use connection_manager::ConnectionManager;
pub use terminal_panel::TerminalPanel;
pub use plugins_panel::PluginsPanel;

// SSH 连接配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub password: Option<String>,
    pub key_file: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuthType {
    Password,
    PublicKey,
}

impl Default for AuthType {
    fn default() -> Self {
        Self::Password
    }
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            port: 22,
            username: String::new(),
            auth_type: AuthType::Password,
            password: None,
            key_file: None,
            description: String::new(),
        }
    }
}
