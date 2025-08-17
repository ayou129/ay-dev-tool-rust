use anyhow::Result;
use serde_json::{json, Value};
use sysinfo::System;
use std::time::{Duration, Instant};

use super::Plugin;
use crate::utils::current_timestamp;

pub struct SystemMonitor {
    system: System,
    last_update: Option<Instant>,
    update_interval: Duration,
}

impl SystemMonitor {
    pub fn new(update_interval_ms: u64) -> Self {
        Self {
            system: System::new_all(),
            last_update: None,
            update_interval: Duration::from_millis(update_interval_ms),
        }
    }

    fn should_update(&self) -> bool {
        match self.last_update {
            None => true,
            Some(last) => last.elapsed() >= self.update_interval,
        }
    }
}

impl Plugin for SystemMonitor {
    fn name(&self) -> &str {
        "System Monitor"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn initialize(&mut self) -> Result<()> {
        self.system.refresh_all();
        Ok(())
    }

    async fn update(&mut self) -> Result<()> {
        if self.should_update() {
            self.system.refresh_all();
            self.last_update = Some(Instant::now());
        }
        Ok(())
    }

    fn render_data(&self) -> Value {
        let cpu_usage: Vec<f32> = self.system.cpus().iter().map(|cpu| cpu.cpu_usage()).collect();
        let memory_used = self.system.used_memory();
        let memory_total = self.system.total_memory();
        let swap_used = self.system.used_swap();
        let swap_total = self.system.total_swap();

        let disks: Vec<Value> = sysinfo::Disks::new_with_refreshed_list().iter().map(|disk| {
            json!({
                "name": disk.name().to_string_lossy(),
                "mount_point": disk.mount_point().to_string_lossy(),
                "total_space": disk.total_space(),
                "available_space": disk.available_space(),
                "usage_percent": if disk.total_space() > 0 {
                    ((disk.total_space() - disk.available_space()) as f64 / disk.total_space() as f64) * 100.0
                } else {
                    0.0
                }
            })
        }).collect();

        json!({
            "timestamp": current_timestamp(),
            "cpu": {
                "usage_per_core": cpu_usage,
                "average_usage": if !cpu_usage.is_empty() {
                    cpu_usage.iter().sum::<f32>() / cpu_usage.len() as f32
                } else {
                    0.0
                }
            },
            "memory": {
                "used": memory_used,
                "total": memory_total,
                "usage_percent": if memory_total > 0 {
                    (memory_used as f64 / memory_total as f64) * 100.0
                } else {
                    0.0
                }
            },
            "swap": {
                "used": swap_used,
                "total": swap_total,
                "usage_percent": if swap_total > 0 {
                    (swap_used as f64 / swap_total as f64) * 100.0
                } else {
                    0.0
                }
            },
            "disks": disks
        })
    }
}
