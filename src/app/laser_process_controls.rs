use super::chrome::{status_chip, white_card};
use super::{start_all_options, RadarApp};
use crate::services::script_runner::LaserScript;
use crate::services::{ProcessComponent, ProcessPhase, StartLaserOptions, TeamSide};
use crate::theme;

impl RadarApp {
    pub(super) fn show_laser_process_controls(&mut self, ui: &mut egui::Ui) {
        let snapshot = self.process_control.snapshot();

        white_card(ui, "脚本控制", |ui| {
            let running = snapshot.laser.managed;
            let active_label = snapshot
                .laser
                .active_laser
                .map(|script| script.label())
                .unwrap_or("Idle");

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("状态:")
                        .color(theme::text_muted())
                        .size(13.0),
                );
                status_chip(ui, running, active_label);
            });
            if snapshot.daemon_available && !running {
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
                    egui::RichText::new("我方阵营")
                        .color(theme::text_muted())
                        .size(13.0),
                );
                let old_side = self.team_side;
                egui::ComboBox::from_id_salt("team_side")
                    .selected_text(match self.team_side {
                        TeamSide::Red => "Red",
                        TeamSide::Blue => "Blue",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.team_side, TeamSide::Red, "Red");
                        ui.selectable_value(&mut self.team_side, TeamSide::Blue, "Blue");
                    });
                if self.team_side != old_side {
                    self.update_shared_team_side();
                }
            });
            ui.label(
                egui::RichText::new(format!(
                    "ROS2 Radar: side={} · Laser/SDR: enemy={}",
                    self.team_side.as_str(),
                    self.team_side.enemy().as_str()
                ))
                .color(theme::text_faint())
                .size(11.0),
            );
            ui.add_space(4.0);
            ui.checkbox(&mut self.laser_auto, "Laser Auto");
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
                        if column
                            .add_sized(
                                [column.available_width(), 30.0],
                                egui::Button::new(script.label()),
                            )
                            .clicked()
                        {
                            let result = self.process_control.start_laser(StartLaserOptions {
                                script: *script,
                                side: self.team_side,
                                stream: self.stream_on_start,
                                record: self.record_on_start,
                                laser_auto: self.laser_auto,
                                configure: script.is_daemon(),
                            });
                            self.store_process_command_result(result);
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
                    .add_sized(
                        [ui.available_width(), 30.0],
                        egui::Button::new("Stop Laser"),
                    )
                    .clicked()
                {
                    let result = self.process_control.stop_laser();
                    self.store_process_command_result(result);
                }
            }
        });

        ui.add_space(14.0);

        white_card(ui, "比赛进程", |ui| {
            self.show_component_control(
                ui,
                "Radar",
                snapshot.radar.managed,
                ProcessComponent::Radar,
            );
            ui.add_space(2.0);
            self.show_component_control(ui, "SDR", snapshot.sdr.managed, ProcessComponent::Sdr);
            ui.add_space(2.0);
            self.show_component_control(
                ui,
                "Laser",
                snapshot.laser.managed,
                ProcessComponent::Laser,
            );

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!("Phase: {:?}", snapshot.phase))
                    .color(theme::text_muted())
                    .size(12.0),
            );
            if let Some(error) = snapshot.error.as_deref() {
                ui.label(egui::RichText::new(error).color(theme::RED).size(12.0));
            }
            if let Some(error) = self.process_command_error.as_deref() {
                ui.label(
                    egui::RichText::new(format!("Command: {error}"))
                        .color(theme::RED)
                        .size(12.0),
                );
            }

            ui.add_space(10.0);
            let pending = matches!(
                snapshot.phase,
                ProcessPhase::StartingRadar
                    | ProcessPhase::WaitingForRadar
                    | ProcessPhase::StartingSdr
                    | ProcessPhase::WaitingForSdr
                    | ProcessPhase::StartingLaser
                    | ProcessPhase::ConfiguringLaser
            );
            if ui
                .add_enabled(
                    !pending,
                    egui::Button::new(if pending {
                        "Starting..."
                    } else {
                        "Start All · Radar → SDR → Laser"
                    }),
                )
                .clicked()
            {
                let options = start_all_options(
                    self.team_side,
                    self.stream_on_start,
                    self.record_on_start,
                    self.laser_auto,
                );
                let result = self.process_control.start_all(options);
                self.store_process_command_result(result);
            }
            if matches!(snapshot.phase, ProcessPhase::Failed(_))
                && ui.button("Retry Failed").clicked()
            {
                let result = self.process_control.retry_failed();
                self.store_process_command_result(result);
            }
            if ui
                .add_sized([ui.available_width(), 30.0], egui::Button::new("Stop All"))
                .clicked()
            {
                let result = self.process_control.stop_all();
                self.store_process_command_result(result);
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
                    let result = self.process_control.send_laser_command("stream on");
                    self.store_process_command_result(result);
                }
                if columns[1]
                    .add_sized(
                        [columns[1].available_width(), 32.0],
                        egui::Button::new("Stream off"),
                    )
                    .clicked()
                {
                    let result = self.process_control.send_laser_command("stream off");
                    self.store_process_command_result(result);
                }
            });
        });
    }

    fn show_component_control(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        managed: bool,
        component: ProcessComponent,
    ) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(label)
                    .color(theme::text_muted())
                    .size(13.0),
            );
            status_chip(ui, managed, if managed { "Running" } else { "Idle" });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let clicked = ui
                    .add_sized(
                        [72.0, 24.0],
                        egui::Button::new(if managed { "Stop" } else { "Start" }),
                    )
                    .clicked();
                if clicked {
                    let result = match (component, managed) {
                        (ProcessComponent::Radar, false) => {
                            self.process_control.start_radar(self.team_side)
                        }
                        (ProcessComponent::Sdr, false) => {
                            self.process_control.start_sdr(self.team_side)
                        }
                        (ProcessComponent::Laser, false) => {
                            self.process_control.start_laser(StartLaserOptions {
                                script: LaserScript::Competition,
                                side: self.team_side,
                                stream: self.stream_on_start,
                                record: self.record_on_start,
                                laser_auto: self.laser_auto,
                                configure: true,
                            })
                        }
                        (ProcessComponent::Radar, true) => self.process_control.stop_radar(),
                        (ProcessComponent::Sdr, true) => self.process_control.stop_sdr(),
                        (ProcessComponent::Laser, true) => self.process_control.stop_laser(),
                    };
                    self.store_process_command_result(result);
                }
            });
        });
    }
}
