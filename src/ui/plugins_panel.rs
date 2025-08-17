use crate::plugins::{
    Plugin, file_browser::FileBrowser, software_detector::SoftwareDetector,
    system_monitor::SystemMonitor,
};
use crate::utils::{format_bytes, format_percentage, truncate_string};
use eframe::egui;
use egui_phosphor::regular;
use egui_plot::{Line, Plot, PlotPoints};
use std::collections::VecDeque;

pub struct PluginsPanel {
    system_monitor: SystemMonitor,
    software_detector: SoftwareDetector,
    file_browser: FileBrowser,
    cpu_history: VecDeque<f64>,
    memory_history: VecDeque<f64>,
    show_system_monitor: bool,
    show_software_list: bool,
    show_file_browser: bool,
}

impl PluginsPanel {
    pub fn new() -> Self {
        Self {
            system_monitor: SystemMonitor::new(1000), // 1秒更新
            software_detector: SoftwareDetector::new(),
            file_browser: FileBrowser::new(),
            cpu_history: VecDeque::with_capacity(100),
            memory_history: VecDeque::with_capacity(100),
            show_system_monitor: true,
            show_software_list: false,
            show_file_browser: false,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        // 系统监控开关
        ui.checkbox(
            &mut self.show_system_monitor,
            format!(
                "启用{} ({})",
                self.system_monitor.name(),
                if self.system_monitor.is_enabled() {
                    "可用"
                } else {
                    "不可用"
                }
            ),
        );

        if self.show_system_monitor {
            ui.collapsing(
                egui::RichText::new(format!("{} 系统监控", regular::CHART_LINE)),
                |ui| {
                    self.show_system_monitor_panel(ui);
                },
            );
        }

        // 文件浏览器开关
        ui.checkbox(
            &mut self.show_file_browser,
            format!(
                "启用{} ({})",
                self.file_browser.name(),
                if self.file_browser.is_enabled() {
                    "可用"
                } else {
                    "不可用"
                }
            ),
        );

        if self.show_file_browser {
            ui.collapsing(
                egui::RichText::new(format!("{} 文件浏览器", regular::FOLDER)),
                |ui| {
                    self.show_file_browser_panel(ui);
                },
            );
        }

        // 软件检测开关
        ui.checkbox(
            &mut self.show_software_list,
            format!(
                "启用{} ({})",
                self.software_detector.name(),
                if self.software_detector.is_enabled() {
                    "可用"
                } else {
                    "不可用"
                }
            ),
        );

        if self.show_software_list {
            ui.collapsing(
                egui::RichText::new(format!("{} 软件检测", regular::GEAR)),
                |ui| {
                    self.show_software_panel(ui);
                },
            );
        }
    }

    fn show_system_monitor_panel(&mut self, ui: &mut egui::Ui) {
        // 更新系统信息
        if let Ok(_) = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(self.system_monitor.update())
        {
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
                    format_percentage(data["cpu"]["average_usage"].as_f64().unwrap_or(0.0)),
                );
            });

            ui.horizontal(|ui| {
                ui.label("内存:");
                ui.colored_label(
                    egui::Color32::from_rgb(255, 150, 100),
                    format_percentage(data["memory"]["usage_percent"].as_f64().unwrap_or(0.0)),
                );
                ui.small(format!(
                    "({} / {})",
                    format_bytes(data["memory"]["used"].as_u64().unwrap_or(0)),
                    format_bytes(data["memory"]["total"].as_u64().unwrap_or(0))
                ));
            });

            // CPU 使用率图表
            if !self.cpu_history.is_empty() {
                let cpu_points: PlotPoints = self
                    .cpu_history
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
                                .color(egui::Color32::from_rgb(100, 150, 255)),
                        );
                    });
            }

            // 内存使用率图表
            if !self.memory_history.is_empty() {
                let memory_points: PlotPoints = self
                    .memory_history
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
                                .color(egui::Color32::from_rgb(255, 150, 100)),
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
                        ui.label(format!(
                            "{:.1}%",
                            disk["usage_percent"].as_f64().unwrap_or(0.0)
                        ));
                    });
                }
            }
        }
    }

    fn show_software_panel(&mut self, ui: &mut egui::Ui) {
        if ui
            .button(
                egui::RichText::new(format!("{} 检测软件", regular::MAGNIFYING_GLASS)).size(14.0),
            )
            .clicked()
        {
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
                        (regular::CHECK_CIRCLE, egui::Color32::GREEN)
                    } else {
                        (regular::X_CIRCLE, egui::Color32::RED)
                    };

                    ui.colored_label(color, egui::RichText::new(icon).size(14.0));
                    ui.label(name);

                    if let Some(ver) = version {
                        ui.small(ver);
                    }

                    if !installed {
                        if let Some(install_cmd) = software["install_command"].as_str() {
                            if ui
                                .small_button(
                                    egui::RichText::new(format!("{} 安装", regular::DOWNLOAD))
                                        .size(12.0),
                                )
                                .on_hover_text(install_cmd)
                                .clicked()
                            {
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
                        format!("{}", summary["installed_count"].as_u64().unwrap_or(0)),
                    );
                    ui.label("总计:");
                    ui.label(format!("{}", summary["total_count"].as_u64().unwrap_or(0)));
                });
            }
        }
    }

    fn show_file_browser_panel(&mut self, ui: &mut egui::Ui) {
        if ui
            .button(egui::RichText::new(format!("{} 刷新", regular::ARROW_CLOCKWISE)).size(14.0))
            .clicked()
        {
            let _ = tokio::runtime::Runtime::new().unwrap().block_on(async {
                self.file_browser.initialize().await?;
                self.file_browser.update().await
            });
        }

        let data = self.file_browser.render_data();

        ui.horizontal(|ui| {
            ui.label("当前路径:");
            ui.small(data["current_path"].as_str().unwrap_or("/"));
        });

        ui.separator();

        if let Some(files) = data["files"].as_array() {
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for file in files {
                        let name = file["name"].as_str().unwrap_or("Unknown");
                        let is_directory = file["is_directory"].as_bool().unwrap_or(false);
                        let size = file["size"].as_u64().unwrap_or(0);

                        ui.horizontal(|ui| {
                            let icon = if is_directory {
                                regular::FOLDER
                            } else {
                                regular::FILE
                            };

                            if is_directory {
                                if ui
                                    .button(
                                        egui::RichText::new(format!(
                                            "{} {}",
                                            icon,
                                            truncate_string(name, 25)
                                        ))
                                        .size(14.0),
                                    )
                                    .clicked()
                                {
                                    // 导航到目录
                                    let mut new_path = std::path::PathBuf::from(
                                        data["current_path"].as_str().unwrap_or("/"),
                                    );
                                    new_path.push(name);
                                    self.file_browser.set_path(new_path);
                                    let _ = tokio::runtime::Runtime::new()
                                        .unwrap()
                                        .block_on(async { self.file_browser.update().await });
                                }
                            } else {
                                ui.label(egui::RichText::new(icon).size(14.0));
                                ui.label(truncate_string(name, 25));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.small(format!("{} bytes", size));
                                    },
                                );
                            }
                        });
                    }
                });

            ui.horizontal(|ui| {
                ui.label("文件数量:");
                ui.label(format!("{}", data["file_count"].as_u64().unwrap_or(0)));
            });
        } else {
            ui.label("无法读取目录内容");
        }
    }
}
