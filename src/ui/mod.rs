pub mod connection_manager;
pub mod plugins_panel;
pub mod terminal_emulator;
pub mod simple_terminal;
pub mod tab_system;

use serde::{Deserialize, Serialize};

pub use connection_manager::ConnectionManager;
pub use plugins_panel::PluginsPanel;
pub use simple_terminal::SimpleTerminalPanel;
pub use tab_system::{TabManager, TabEvent, TabObserver};

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
