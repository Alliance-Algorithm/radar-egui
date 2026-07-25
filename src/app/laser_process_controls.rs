use super::chrome::{status_chip, white_card};
use super::{EnemyColor, RadarApp};
use crate::services::script_runner::LaserScript;
use crate::theme;

fn stream_cmd(on: bool) -> String {
    format!("stream {}", if on { "on" } else { "off" })
}

fn record_cmd(on: bool) -> String {
    format!("record {}", if on { "on" } else { "off" })
}

impl RadarApp {
    pub(super) fn show_laser_process_controls(&mut self, ui: &mut egui::Ui) {
        white_card(ui, "脚本控制", |ui| {
            let running = self.process_control.is_running();
            let daemon_ok = self.process_control.daemon_alive();
            let active_label = self
                .process_control
                .active()
                .map(|s| s.label())
                .unwrap_or("Idle");

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("状态:")
                        .color(theme::text_muted())
                        .size(13.0),
                );
                status_chip(ui, running, active_label);
            });
            if daemon_ok && !running {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("daemon 存活 (可通过流控制发送命令)")
                        .color(theme::text_faint())
                        .size(11.0),
                );
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("敌方")
                        .color(theme::text_muted())
                        .size(13.0),
                );
                egui::ComboBox::from_id_salt("enemy_color")
                    .selected_text(self.enemy_color.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.enemy_color, EnemyColor::Red, "Red");
                        ui.selectable_value(&mut self.enemy_color, EnemyColor::Blue, "Blue");
                        ui.selectable_value(&mut self.enemy_color, EnemyColor::Auto, "Auto");
                    });
            });
            ui.add_space(4.0);
            ui.checkbox(&mut self.stream_on_start, "启动时推流");
            ui.checkbox(&mut self.record_on_start, "启动时内录");
            ui.add_space(6.0);
            let scripts = [
                [LaserScript::Competition, LaserScript::Preview],
                [LaserScript::Stream, LaserScript::Record],
            ];
            ui.columns(2, |columns| {
                for (row_index, row) in scripts.iter().enumerate() {
                    for (column, script) in columns.iter_mut().zip(row.iter()) {
                        let label = script.label();
                        if column
                            .add_sized([column.available_width(), 30.0], egui::Button::new(label))
                            .clicked()
                        {
                            let result = if script.is_daemon() {
                                self.process_control.start_script_with_daemon_config(
                                    *script,
                                    &self.camera_device,
                                    self.enemy_color.fifo_cmd().to_owned(),
                                    stream_cmd(self.stream_on_start),
                                    record_cmd(self.record_on_start),
                                )
                            } else {
                                self.process_control
                                    .start_script(*script, &self.camera_device)
                            };
                            if let Err(e) = result {
                                log::error!("Failed to start {}: {}", label, e);
                            }
                        }
                    }
                    if row_index + 1 < scripts.len() {
                        for column in &mut columns[..] {
                            column.add_space(6.0);
                        }
                    }
                }
            });
            if running {
                ui.add_space(10.0);
                if ui
                    .add_sized([ui.available_width(), 30.0], egui::Button::new("Stop"))
                    .clicked()
                {
                    self.process_control.stop_script();
                }
            }
        });

        ui.add_space(14.0);

        white_card(ui, "比赛进程", |ui| {
            let sdr_ok = self.process_control.is_sdr_running();
            let start_all_pending = self.process_control.has_pending_start_all();

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("SDR")
                        .color(theme::text_muted())
                        .size(13.0),
                );
                status_chip(ui, sdr_ok, if sdr_ok { "Running" } else { "Idle" });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if sdr_ok {
                        if ui
                            .add_sized([72.0, 24.0], egui::Button::new("Stop"))
                            .clicked()
                        {
                            self.process_control.stop_sdr();
                        }
                    } else if ui
                        .add_sized([72.0, 24.0], egui::Button::new("Start"))
                        .clicked()
                    {
                        if let Err(e) = self.process_control.start_sdr(self.enemy_color.sdr_arg()) {
                            log::error!("Failed to start SDR: {}", e);
                        }
                    }
                });
            });
            ui.add_space(2.0);

            let radar_ok = self.process_control.is_radar_running();
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Radar")
                        .color(theme::text_muted())
                        .size(13.0),
                );
                status_chip(ui, radar_ok, if radar_ok { "Running" } else { "Idle" });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if radar_ok {
                        if ui
                            .add_sized([72.0, 24.0], egui::Button::new("Stop"))
                            .clicked()
                        {
                            self.process_control.stop_radar();
                        }
                    } else {
                        egui::ComboBox::from_id_salt("radar_side")
                            .selected_text(self.radar_side.as_str())
                            .width(48.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.radar_side, "red".to_string(), "Red");
                                ui.selectable_value(
                                    &mut self.radar_side,
                                    "blue".to_string(),
                                    "Blue",
                                );
                            });
                        if ui
                            .add_sized([60.0, 24.0], egui::Button::new("Start"))
                            .clicked()
                        {
                            if let Err(e) = self.process_control.start_radar(&self.radar_side) {
                                log::error!("Failed to start Radar: {}", e);
                            }
                        }
                    }
                });
            });

            ui.add_space(10.0);
            if ui
                .add_enabled(
                    !start_all_pending,
                    egui::Button::new(if start_all_pending {
                        "Starting..."
                    } else {
                        "Start All (SDR → Laser)"
                    }),
                )
                .clicked()
            {
                if let Err(e) = self.process_control.schedule_start_all(
                    self.enemy_color.sdr_arg(),
                    &self.camera_device,
                    self.enemy_color.fifo_cmd().to_owned(),
                    stream_cmd(self.stream_on_start),
                    record_cmd(self.record_on_start),
                ) {
                    log::error!("Start All failed: {}", e);
                }
            }

            if sdr_ok || radar_ok || self.process_control.is_running() {
                ui.add_space(6.0);
                if ui
                    .add_sized([ui.available_width(), 30.0], egui::Button::new("Stop All"))
                    .clicked()
                {
                    self.process_control.stop_all();
                }
            }
        });

        ui.add_space(14.0);

        white_card(ui, "流控制", |ui| {
            ui.columns(2, |columns| {
                if columns[0]
                    .add_sized(
                        [columns[0].available_width(), 32.0],
                        egui::Button::new("Stream on"),
                    )
                    .clicked()
                {
                    self.process_control.send_laser_command("stream on");
                }
                if columns[1]
                    .add_sized(
                        [columns[1].available_width(), 32.0],
                        egui::Button::new("Stream off"),
                    )
                    .clicked()
                {
                    self.process_control.send_laser_command("stream off");
                }
            });
        });
    }
}
