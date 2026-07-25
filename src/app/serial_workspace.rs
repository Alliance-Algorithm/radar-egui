use super::chrome::{status_chip, white_card};
use super::shell::SIDE_SERIAL;
use super::RadarApp;
use crate::theme;
use crate::widgets::{SerialFrameLogLine, SerialLogKind, SerialPanel};

impl RadarApp {
    pub(super) fn show_serial_workspace(&mut self, ctx: &egui::Context) {
        self.show_left_rail(ctx);
        self.show_right_inspector(ctx, "serial_inspector", SIDE_SERIAL, |app, ui| {
            app.show_serial_sidebar(ui);
        });
        self.show_main_column(
            ctx,
            |_, ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Serial Workspace")
                            .color(theme::text())
                            .size(21.0),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("referee UART · listen / operate · DJI protocol")
                            .color(theme::text_muted())
                            .size(13.0),
                    );
                });
            },
            |app, ui| {
                let body = ui.available_rect_before_wrap();
                ui.allocate_ui_at_rect(body, |ui| {
                    ui.set_min_size(body.size());
                    ui.set_max_size(body.size());
                    let data = app.serial_reader.inner().lock().ok().map(|g| g.clone());
                    if let Some(data) = data {
                        SerialPanel::new().show_monitor(
                            ui,
                            &data,
                            app.serial_open,
                            &app.serial_port_name,
                            app.serial_baud,
                            &app.serial_frame_log,
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("无法读取 SerialData")
                                .color(theme::RED)
                                .size(13.0),
                        );
                    }
                });
            },
        );
    }

    pub(super) fn show_serial_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                white_card(ui, "串口连接", |ui| {
                    status_chip(
                        ui,
                        self.serial_open,
                        if self.serial_open { "Open" } else { "Closed" },
                    );
                    ui.add_space(4.0);
                    status_chip(
                        ui,
                        self.serial_open,
                        if self.serial_open {
                            "Parser running"
                        } else {
                            "Parser idle"
                        },
                    );
                    ui.add_space(10.0);
                    egui::Grid::new("serial_conn_grid")
                        .num_columns(2)
                        .min_col_width(72.0)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("Port")
                                    .color(theme::text_muted())
                                    .size(13.0),
                            );
                            egui::ComboBox::from_id_salt("serial_port")
                                .selected_text(self.serial_port_name.as_str())
                                .width(ui.available_width().max(120.0))
                                .show_ui(ui, |ui| {
                                    for p in [
                                        "/dev/ttyUSB0",
                                        "/dev/ttyUSB1",
                                        "/dev/ttyACM0",
                                        "/dev/ttyCH341USB0",
                                    ] {
                                        ui.selectable_value(
                                            &mut self.serial_port_name,
                                            p.to_string(),
                                            p,
                                        );
                                    }
                                });
                            ui.end_row();
                            ui.label(
                                egui::RichText::new("Baud")
                                    .color(theme::text_muted())
                                    .size(13.0),
                            );
                            egui::ComboBox::from_id_salt("serial_baud")
                                .selected_text(self.serial_baud.to_string())
                                .show_ui(ui, |ui| {
                                    for b in [115_200_u32, 230_400, 921_600, 1_500_000] {
                                        ui.selectable_value(
                                            &mut self.serial_baud,
                                            b,
                                            b.to_string(),
                                        );
                                    }
                                });
                            ui.end_row();
                            ui.label(
                                egui::RichText::new("Timeout")
                                    .color(theme::text_muted())
                                    .size(13.0),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.serial_timeout_ms)
                                    .range(10..=500)
                                    .suffix(" ms"),
                            );
                            ui.end_row();
                        });
                    ui.add_space(10.0);
                    ui.columns(2, |cols| {
                        if cols[0]
                            .add_enabled(
                                !self.serial_open,
                                egui::Button::new("Open").fill(theme::BLUE),
                            )
                            .clicked()
                        {
                            self.open_serial();
                            if self.serial_open {
                                self.push_serial_log(
                                    SerialLogKind::Ok,
                                    format!("OPEN {}", self.serial_port_name),
                                );
                            } else if let Some(err) = &self.serial_error {
                                self.push_serial_log(
                                    SerialLogKind::Err,
                                    format!("OPEN fail {err}"),
                                );
                            }
                        }
                        if cols[1]
                            .add_enabled(self.serial_open, egui::Button::new("Close"))
                            .clicked()
                        {
                            self.serial_open = false;
                            self.push_serial_log(SerialLogKind::Err, "CLOSE".into());
                        }
                    });
                    if let Some(err) = &self.serial_error {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(err).color(theme::RED).size(12.0));
                    }
                    ui.add_space(8.0);
                    egui::Grid::new("serial_cfg_meta")
                        .num_columns(2)
                        .spacing([10.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("SerialConfig")
                                    .color(theme::text_faint())
                                    .size(12.0),
                            );
                            ui.label(egui::RichText::new("8N1").color(theme::text()).size(12.0));
                            ui.end_row();
                            ui.label(
                                egui::RichText::new("RX / TX")
                                    .color(theme::text_faint())
                                    .size(12.0),
                            );
                            ui.label(
                                egui::RichText::new("try_clone")
                                    .color(theme::text())
                                    .size(12.0),
                            );
                            ui.end_row();
                        });
                });

                ui.add_space(12.0);
                white_card(ui, "解析开关", |ui| {
                    let labels = [
                        ("0x0001 比赛状态", 0usize),
                        ("0x0101 场地事件", 1),
                        ("0x020C 雷达标记", 2),
                        ("0x0305 小地图雷达", 3),
                        ("0x0A0x SDR 透传", 4),
                        ("写回 ZMQ PUB", 5),
                    ];
                    for (label, idx) in labels {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(label)
                                    .color(theme::text_muted())
                                    .size(12.0),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.checkbox(&mut self.serial_parse_enable[idx], "");
                                },
                            );
                        });
                        ui.add_space(4.0);
                    }
                });

                ui.add_space(12.0);
                if let Ok(guard) = self.serial_reader.inner().lock() {
                    SerialPanel::new().show_minimap_sidebar(ui, &guard.minimap_receive_radar_data);
                    ui.add_space(12.0);
                    SerialPanel::new().show_dirty_flags(ui, &guard);
                }
            });
    }

    fn push_serial_log(&mut self, kind: SerialLogKind, text: String) {
        let ts = chrono_like_now();
        self.serial_frame_log.push_back(SerialFrameLogLine {
            text: format!("{ts}  {text}"),
            kind,
        });
        while self.serial_frame_log.len() > 80 {
            self.serial_frame_log.pop_front();
        }
    }
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}
