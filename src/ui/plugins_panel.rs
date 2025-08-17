use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use std::collections::VecDeque;
use crate::plugins::{system_monitor::SystemMonitor, software_detector::SoftwareDetector, Plugin};
use crate::utils::{format_bytes, format_percentage};

pub struct PluginsPanel {
    system_monitor: SystemMonitor,
    software_detector: SoftwareDetector,
    cpu_history: VecDeque<f64>,
    memory_history: VecDeque<f64>,
    show_system_monitor: bool,
    show_software_list: bool,
}

impl PluginsPanel {
    pub fn new() -> Self {
        Self {
            system_monitor: SystemMonitor::new(1000), // 1秒更新
            software_detector: SoftwareDetector::new(),
            cpu_history: VecDeque::with_capacity(100),
            memory_history: VecDeque::with_capacity(100),
            show_system_monitor: true,
            show_software_list: false,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // 系统监控开关
        ui.checkbox(&mut self.show_system_monitor, "启用系统监控");
        
        if self.show_system_monitor {
            ui.collapsing("📊 系统监控", |ui| {
                self.show_system_monitor_panel(ui);
            });
        }
        
        ui.collapsing("📁 文件浏览器", |ui| {
            ui.label("文件浏览器 - 开发中");
            ui.small("将显示当前连接的远程目录结构");
        });
        
        // 软件检测开关
        ui.checkbox(&mut self.show_software_list, "启用软件检测");
        
        if self.show_software_list {
            ui.collapsing("⚙️ 软件检测", |ui| {
                self.show_software_panel(ui);
            });
        }
    }

    fn show_system_monitor_panel(&mut self, ui: &mut egui::Ui) {
        // 更新系统信息
        if let Ok(_) = tokio::runtime::Runtime::new().unwrap().block_on(self.system_monitor.update()) {
            let data = self.system_monitor.render_data();
            
            if let Some(cpu_avg) = data["cpu"]["average_usage"].as_f64() {
                self.cpu_history.push_back(cpu_avg);
                if self.cpu_history.len() > 100 {
                    self.cpu_history.pop_front();
                }
            }
            
            if let Some(memory_percent) = data["memory"]["usage_percent"].as_f64() {
                self.memory_history.push_back(memory_percent);
                if self.memory_history.len() > 100 {
                    self.memory_history.pop_front();
                }
            }
            
            // 显示实时数据
            ui.horizontal(|ui| {
                ui.label("CPU:");
                ui.colored_label(
                    egui::Color32::from_rgb(100, 150, 255),
                    format_percentage(data["cpu"]["average_usage"].as_f64().unwrap_or(0.0))
                );
            });
            
            ui.horizontal(|ui| {
                ui.label("内存:");
                ui.colored_label(
                    egui::Color32::from_rgb(255, 150, 100),
                    format_percentage(data["memory"]["usage_percent"].as_f64().unwrap_or(0.0))
                );
                ui.small(format!("({} / {})", 
                    format_bytes(data["memory"]["used"].as_u64().unwrap_or(0)),
                    format_bytes(data["memory"]["total"].as_u64().unwrap_or(0))
                ));
            });
            
            // CPU 使用率图表
            if !self.cpu_history.is_empty() {
                let cpu_points: PlotPoints = self.cpu_history
                    .iter()
                    .enumerate()
                    .map(|(i, &cpu)| [i as f64, cpu])
                    .collect();
                
                Plot::new("cpu_plot")
                    .height(80.0)
                    .show_axes([false, true])
                    .allow_zoom(false)
                    .allow_drag(false)
                    .show(ui, |plot_ui| {
                        plot_ui.line(
                            Line::new("CPU %", cpu_points)
                                .color(egui::Color32::from_rgb(100, 150, 255))
                        );
                    });
            }
            
            // 内存使用率图表
            if !self.memory_history.is_empty() {
                let memory_points: PlotPoints = self.memory_history
                    .iter()
                    .enumerate()
                    .map(|(i, &mem)| [i as f64, mem])
                    .collect();
                
                Plot::new("memory_plot")
                    .height(80.0)
                    .show_axes([false, true])
                    .allow_zoom(false)
                    .allow_drag(false)
                    .show(ui, |plot_ui| {
                        plot_ui.line(
                            Line::new("Memory %", memory_points)
                                .color(egui::Color32::from_rgb(255, 150, 100))
                        );
                    });
            }
            
            // 磁盘使用情况
            if let Some(disks) = data["disks"].as_array() {
                ui.separator();
                ui.strong("磁盘使用:");
                for disk in disks {
                    ui.horizontal(|ui| {
                        ui.label(disk["mount_point"].as_str().unwrap_or("Unknown"));
                        ui.label(format!("{:.1}%", disk["usage_percent"].as_f64().unwrap_or(0.0)));
                    });
                }
            }
        }
    }

    fn show_software_panel(&mut self, ui: &mut egui::Ui) {
        if ui.button("🔍 检测软件").clicked() {
            // 启动软件检测
            let _ = tokio::runtime::Runtime::new().unwrap().block_on(async {
                self.software_detector.initialize().await?;
                self.software_detector.update().await
            });
        }
        
        let data = self.software_detector.render_data();
        
        if let Some(software_list) = data["software"].as_array() {
            ui.separator();
            
            for software in software_list {
                let name = software["name"].as_str().unwrap_or("Unknown");
                let installed = software["installed"].as_bool().unwrap_or(false);
                let version = software["version"].as_str();
                
                ui.horizontal(|ui| {
                    let (icon, color) = if installed {
                        ("✅", egui::Color32::GREEN)
                    } else {
                        ("❌", egui::Color32::RED)
                    };
                    
                    ui.colored_label(color, icon);
                    ui.label(name);
                    
                    if let Some(ver) = version {
                        ui.small(ver);
                    }
                    
                    if !installed {
                        if let Some(install_cmd) = software["install_command"].as_str() {
                            if ui.small_button("📥").on_hover_text(install_cmd).clicked() {
                                // TODO: 执行安装命令
                            }
                        }
                    }
                });
            }
            
            // 统计信息
            if let Some(summary) = data["summary"].as_object() {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("已安装:");
                    ui.colored_label(
                        egui::Color32::GREEN,
                        format!("{}", summary["installed_count"].as_u64().unwrap_or(0))
                    );
                    ui.label("总计:");
                    ui.label(format!("{}", summary["total_count"].as_u64().unwrap_or(0)));
                });
            }
        }
    }
}
