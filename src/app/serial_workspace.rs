use super::chrome::{status_chip, white_card};
use super::shell::SIDE_SERIAL;
use super::RadarApp;
use crate::shared_data::SharedData;
use crate::theme;
use crate::widgets::{SerialFrameLogLine, SerialLogKind, SerialPanel};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SerialObservedState {
    game: (u8, u8, u16, u64),
    site: (u8, u8, u8, u8, u8, u16, u8, u8, u8, u8, u8),
    radar: [u8; 5],
}

impl SerialObservedState {
    fn from_shared(data: &SharedData) -> Self {
        let game = &data.game_state;
        let site = &data.site_event;
        let radar = &data.radar_mark_process;
        Self {
            game: (
                game.game_type,
                game.game_progress,
                game.stage_remain_time,
                game.sync_timestamp,
            ),
            site: (
                site.supply_zone_status,
                site.energy_small_status,
                site.energy_large_status,
                site.central_highland_status,
                site.trapezoid_highland_status,
                site.dart_hit_time,
                site.dart_hit_target,
                site.center_gain_status,
                site.fortress_gain_status,
                site.outpost_gain_status,
                site.base_gain_status,
            ),
            radar: [
                radar.opponent_hero_vulnerable,
                radar.opponent_engineer_vulnerable,
                radar.opponent_infantry_3_vulnerable,
                radar.opponent_infantry_4_vulnerable,
                radar.opponent_sentry_vulnerable,
            ],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SerialLogEvent {
    kind: SerialLogKind,
    text: String,
}

impl SerialLogEvent {
    fn rx(text: impl Into<String>) -> Self {
        Self {
            kind: SerialLogKind::Rx,
            text: text.into(),
        }
    }
}

fn diff_serial_state(
    previous: &SerialObservedState,
    current: &SerialObservedState,
) -> Vec<SerialLogEvent> {
    let mut events = Vec::new();
    if previous.game != current.game {
        events.push(SerialLogEvent::rx(format!(
            "0x0001 GameState · remain={}s",
            current.game.2
        )));
    }
    if previous.site != current.site {
        events.push(SerialLogEvent::rx(format!(
            "0x0101 SiteEvent · supply={} energy={}/{} highland={}/{} dart={}:{} center={} fortress={} outpost={} base={}",
            current.site.0,
            current.site.1,
            current.site.2,
            current.site.3,
            current.site.4,
            current.site.5,
            current.site.6,
            current.site.7,
            current.site.8,
            current.site.9,
            current.site.10,
        )));
    }
    if previous.radar != current.radar {
        const LABELS: [&str; 5] = ["Hero", "Engineer", "Infantry 3", "Infantry 4", "Sentry"];
        let changes = LABELS
            .iter()
            .zip(current.radar)
            .zip(previous.radar)
            .filter(|((_, current), previous)| current != previous)
            .map(|((label, value), _)| {
                format!("{label}={}", if value != 0 { "vulnerable" } else { "idle" })
            })
            .collect::<Vec<_>>()
            .join(", ");
        events.push(SerialLogEvent::rx(format!(
            "0x020C RadarMarkProcess · {changes}"
        )));
    }
    events
}

fn observe_serial_state(
    previous: &mut Option<SerialObservedState>,
    data: &SharedData,
) -> Vec<SerialLogEvent> {
    let current = SerialObservedState::from_shared(data);
    let events = previous
        .as_ref()
        .map(|previous| diff_serial_state(previous, &current))
        .unwrap_or_default();
    *previous = Some(current);
    events
}

fn push_bounded_serial_log(
    log: &mut std::collections::VecDeque<SerialFrameLogLine>,
    line: SerialFrameLogLine,
) {
    log.push_back(line);
    while log.len() > 80 {
        log.pop_front();
    }
}

impl RadarApp {
    pub(super) fn show_serial_workspace(&mut self, ctx: &egui::Context, snapshot: &SharedData) {
        self.show_left_rail(ctx);
        self.show_right_inspector(ctx, "serial_inspector", SIDE_SERIAL, |app, ui| {
            app.show_serial_sidebar(ui, snapshot);
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
                    SerialPanel::new().show_monitor(
                        ui,
                        snapshot,
                        app.serial_open,
                        &app.serial_port_name,
                        app.serial_baud,
                        &app.serial_frame_log,
                    );
                });
            },
        );
    }

    pub(super) fn show_serial_sidebar(&mut self, ui: &mut egui::Ui, snapshot: &SharedData) {
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
                                        "/dev/ttyACM0",
                                        "/dev/ttyUSB0",
                                        "/dev/ttyUSB1",
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
                        });
                    ui.add_space(10.0);
                    if self.serial_open {
                        if ui
                            .button("Close serial")
                            .on_hover_text("Stop serial RX/TX workers")
                            .clicked()
                        {
                            self.close_serial();
                            self.push_serial_log(SerialLogKind::Info, "CLOSE serial".to_string());
                        }
                    } else if ui
                        .add(egui::Button::new("Open serial").fill(theme::BLUE))
                        .clicked()
                    {
                        self.open_serial();
                        if self.serial_open {
                            self.push_serial_log(
                                SerialLogKind::Ok,
                                format!("OPEN {}", self.serial_port_name),
                            );
                        } else if let Some(err) = &self.serial_error {
                            self.push_serial_log(SerialLogKind::Err, format!("OPEN fail {err}"));
                        }
                    }
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
                SerialPanel::new().show_minimap_sidebar(ui, &snapshot.minimap_receive);
                ui.add_space(12.0);
            });
    }

    pub(super) fn push_serial_log(&mut self, kind: SerialLogKind, text: String) {
        let ts = chrono_like_now();
        push_bounded_serial_log(
            &mut self.serial_frame_log,
            SerialFrameLogLine {
                text: format!("{ts}  {text}"),
                kind,
            },
        );
    }

    pub(super) fn update_serial_state_log(&mut self, data: &SharedData) {
        for event in observe_serial_state(&mut self.serial_last_observed, data) {
            self.push_serial_log(event.kind, event.text);
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
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{h:02}:{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_diff_logs_only_changed_observable_groups() {
        let before = SerialObservedState::default();
        let mut after = before.clone();
        after.game.2 = 419;
        after.radar[0] = 1;
        assert_eq!(
            diff_serial_state(&before, &after),
            vec![
                SerialLogEvent::rx("0x0001 GameState · remain=419s"),
                SerialLogEvent::rx("0x020C RadarMarkProcess · Hero=vulnerable"),
            ]
        );
    }

    #[test]
    fn identical_serial_snapshots_do_not_create_fake_frames() {
        let state = SerialObservedState::default();
        assert!(diff_serial_state(&state, &state).is_empty());
    }

    #[test]
    fn first_serial_observation_sets_baseline_without_log_entries() {
        let mut previous = None;
        let mut data = SharedData::default();
        data.game_state.stage_remain_time = 317;
        data.radar_mark_process.opponent_sentry_vulnerable = 1;
        let expected = SerialObservedState::from_shared(&data);
        let events = observe_serial_state(&mut previous, &data);

        assert!(events.is_empty());
        assert_eq!(previous, Some(expected));
    }

    #[test]
    fn serial_log_keeps_only_the_latest_eighty_entries() {
        let mut log = std::collections::VecDeque::new();
        for index in 0..81 {
            push_bounded_serial_log(
                &mut log,
                SerialFrameLogLine {
                    text: index.to_string(),
                    kind: SerialLogKind::Info,
                },
            );
        }

        assert_eq!(log.len(), 80);
        assert_eq!(log.front().unwrap().text, "1");
        assert_eq!(log.back().unwrap().text, "80");
    }
}
