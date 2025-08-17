use anyhow::Result;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::process::Command;

use super::Plugin;

pub struct SoftwareDetector {
    detected_software: HashMap<String, SoftwareInfo>,
}

#[derive(Debug, Clone)]
struct SoftwareInfo {
    name: String,
    version: Option<String>,
    installed: bool,
    install_command: Option<String>,
    download_url: Option<String>,
}

impl SoftwareDetector {
    pub fn new() -> Self {
        Self {
            detected_software: HashMap::new(),
        }
    }

    async fn detect_software(&mut self) -> Result<()> {
        let software_list = vec![
            (
                "php",
                "php --version",
                Some("apt install php"),
                Some("https://php.net"),
            ),
            (
                "mysql",
                "mysql --version",
                Some("apt install mysql-server"),
                Some("https://mysql.com"),
            ),
            (
                "redis",
                "redis-server --version",
                Some("apt install redis-server"),
                Some("https://redis.io"),
            ),
            (
                "docker",
                "docker --version",
                Some("apt install docker.io"),
                Some("https://docker.com"),
            ),
            (
                "node",
                "node --version",
                Some("apt install nodejs"),
                Some("https://nodejs.org"),
            ),
            (
                "python",
                "python --version",
                Some("apt install python3"),
                Some("https://python.org"),
            ),
            (
                "python3",
                "python3 --version",
                Some("apt install python3"),
                Some("https://python.org"),
            ),
            (
                "conda",
                "conda --version",
                None,
                Some("https://anaconda.com"),
            ),
            (
                "nvcc",
                "nvcc --version",
                None,
                Some("https://developer.nvidia.com/cuda-downloads"),
            ),
            (
                "nvidia-smi",
                "nvidia-smi",
                None,
                Some("https://nvidia.com/drivers"),
            ),
        ];

        for (name, check_cmd, install_cmd, download_url) in software_list {
            let parts: Vec<&str> = check_cmd.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let result = Command::new(parts[0]).args(&parts[1..]).output();

            let (installed, version) = match result {
                Ok(output) if output.status.success() => {
                    let version_output = String::from_utf8_lossy(&output.stdout);
                    (true, Some(version_output.trim().to_string()))
                }
                _ => (false, None),
            };

            self.detected_software.insert(
                name.to_string(),
                SoftwareInfo {
                    name: name.to_string(),
                    version,
                    installed,
                    install_command: install_cmd.map(|s| s.to_string()),
                    download_url: download_url.map(|s| s.to_string()),
                },
            );
        }

        Ok(())
    }
}

impl Plugin for SoftwareDetector {
    fn name(&self) -> &str {
        "Software Detector"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn initialize(&mut self) -> Result<()> {
        self.detect_software().await?;
        Ok(())
    }

    async fn update(&mut self) -> Result<()> {
        self.detect_software().await?;
        Ok(())
    }

    fn render_data(&self) -> Value {
        let software: Vec<Value> = self
            .detected_software
            .values()
            .map(|info| {
                json!({
                    "name": info.name,
                    "version": info.version,
                    "installed": info.installed,
                    "install_command": info.install_command,
                    "download_url": info.download_url,
                    "status": if info.installed { "installed" } else { "not_installed" }
                })
            })
            .collect();

        let installed_count = self
            .detected_software
            .values()
            .filter(|info| info.installed)
            .count();
        let total_count = self.detected_software.len();

        json!({
            "software": software,
            "summary": {
                "installed_count": installed_count,
                "total_count": total_count,
                "detection_complete": true
            }
        })
    }
}
