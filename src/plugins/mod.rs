pub mod file_browser;
pub mod software_detector;
pub mod system_monitor;

use anyhow::Result;

pub trait Plugin {
    fn name(&self) -> &str;
    fn is_enabled(&self) -> bool;
    async fn initialize(&mut self) -> Result<()>;
    async fn update(&mut self) -> Result<()>;
    fn render_data(&self) -> serde_json::Value;
}
