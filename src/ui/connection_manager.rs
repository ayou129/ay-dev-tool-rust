use eframe::egui;
use crate::config::AppConfig;
use crate::ui::{ConnectionConfig, AuthType};

pub struct ConnectionManager {
    show_add_dialog: bool,
    edit_connection: Option<ConnectionConfig>,
    selected_connection: Option<usize>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            show_add_dialog: false,
            edit_connection: None,
            selected_connection: None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, config: &mut AppConfig) -> Option<ConnectionConfig> {
        let mut connection_to_establish = None;
        ui.heading("快速连接");
        
        ui.horizontal(|ui| {
            if ui.button("➕ 添加终端").clicked() {
                self.show_add_dialog = true;
                self.edit_connection = Some(ConnectionConfig::default());
            }
            
            if ui.button("🗑️ 清空所有").clicked() {
                config.connections.clear();
            }
        });
        
        ui.separator();
        
        // 连接列表
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut to_remove = None;
            let mut to_connect = None;
            
            for (i, connection) in config.connections.iter().enumerate() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.strong(&connection.name);
                            ui.label(format!("{}@{}:{}", connection.username, connection.host, connection.port));
                            if !connection.description.is_empty() {
                                ui.small(connection.description.clone());
                            }
                        });
                        
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("🗑️").clicked() {
                                to_remove = Some(i);
                            }
                            if ui.button("✏️").clicked() {
                                self.edit_connection = Some(connection.clone());
                                self.show_add_dialog = true;
                                self.selected_connection = Some(i);
                            }
                            if ui.button("🔗 连接").clicked() {
                                to_connect = Some(i);
                            }
                        });
                    });
                });
                ui.add_space(5.0);
            }
            
            // 处理删除
            if let Some(index) = to_remove {
                config.connections.remove(index);
            }
            
            // 处理连接
            if let Some(index) = to_connect {
                log::info!("Connecting to: {:?}", config.connections[index]);
                connection_to_establish = Some(config.connections[index].clone());
            }
        });
        
        // 添加/编辑对话框
        self.show_add_edit_dialog(ui, config);
        
        connection_to_establish
    }

    fn show_add_edit_dialog(&mut self, ui: &mut egui::Ui, config: &mut AppConfig) {
        if self.show_add_dialog {
            let mut connection = if let Some(connection) = self.edit_connection.take() {
                connection
            } else {
                return;
            };
            
            let mut should_save = false;
            let mut should_cancel = false;
            
            egui::Window::new("Add/Edit Connection")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                        egui::Grid::new("connection_form")
                            .num_columns(2)
                            .spacing([40.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("名称:");
                                ui.text_edit_singleline(&mut connection.name);
                                ui.end_row();

                                ui.label("主机:");
                                ui.text_edit_singleline(&mut connection.host);
                                ui.end_row();

                                ui.label("端口:");
                                ui.add(egui::DragValue::new(&mut connection.port).range(1..=65535));
                                ui.end_row();

                                ui.label("用户名:");
                                ui.text_edit_singleline(&mut connection.username);
                                ui.end_row();

                                ui.label("认证类型:");
                                egui::ComboBox::from_label("")
                                    .selected_text(match connection.auth_type {
                                        AuthType::Password => "密码",
                                        AuthType::PublicKey => "公钥",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut connection.auth_type, AuthType::Password, "密码");
                                        ui.selectable_value(&mut connection.auth_type, AuthType::PublicKey, "公钥");
                                    });
                                ui.end_row();

                                match connection.auth_type {
                                    AuthType::Password => {
                                        ui.label("密码:");
                                        let mut password = connection.password.clone().unwrap_or_default();
                                        ui.add(egui::TextEdit::singleline(&mut password).password(true));
                                        connection.password = Some(password);
                                        ui.end_row();
                                    }
                                    AuthType::PublicKey => {
                                        ui.label("私钥文件:");
                                        let mut key_file = connection.key_file.clone().unwrap_or_default();
                                        ui.horizontal(|ui| {
                                            ui.text_edit_singleline(&mut key_file);
                                            if ui.button("📁").clicked() {
                                                // TODO: 实现文件选择器
                                            }
                                        });
                                        connection.key_file = Some(key_file);
                                        ui.end_row();
                                    }
                                }

                                ui.label("描述:");
                                ui.text_edit_multiline(&mut connection.description);
                                ui.end_row();
                            });

                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("✅ 保存").clicked() {
                                should_save = true;
                            }

                            if ui.button("❌ 取消").clicked() {
                                should_cancel = true;
                            }
                        });
                    });
            
            // 处理按钮事件
            if should_save {
                if let Some(index) = self.selected_connection {
                    config.connections[index] = connection.clone();
                } else {
                    config.connections.push(connection.clone());
                }
                
                self.show_add_dialog = false;
                self.selected_connection = None;
                
                // 保存配置
                let _ = config.save();
            } else if should_cancel {
                self.show_add_dialog = false;
                self.selected_connection = None;
            } else {
                // 如果没有保存或取消，将连接放回去
                self.edit_connection = Some(connection);
            }
        }
    }
}
