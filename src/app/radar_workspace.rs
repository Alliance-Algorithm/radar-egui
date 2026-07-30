use super::chrome::{status_chip, white_card};
use super::shell::{radar_strip_height, SIDE_RADAR, STAGE_GAP};
use super::RadarApp;
use crate::theme;
use crate::ui_layout::{inset_rect, STAGE_PAD};

fn rerun_status_label() -> &'static str {
    "optional · not monitored"
}

impl RadarApp {
    pub(super) fn show_radar_workspace(&mut self, ctx: &egui::Context) {
        self.ensure_pointcloud_started();

        self.show_left_rail(ctx);
        self.show_right_inspector(ctx, "radar_inspector", SIDE_RADAR, |app, ui| {
            app.show_radar_status_sidebar(ui);
        });
        self.show_main_column(
            ctx,
            |_, ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("ROS2 Radar Workspace")
                            .color(theme::text())
                            .size(21.0),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(
                            "location transport / point-cloud SHM / optional Rerun",
                        )
                        .color(theme::text_muted())
                        .size(13.0),
                    );
                });
            },
            |app, ui| {
                let body = ui.available_rect_before_wrap();
                let full_h = body.height();
                let strip_h = radar_strip_height(full_h);
                let stage_h = (full_h - strip_h - STAGE_GAP).max(160.0);
                let stage_rect =
                    egui::Rect::from_min_size(body.min, egui::vec2(body.width(), stage_h));
                let strip_rect = egui::Rect::from_min_size(
                    egui::pos2(body.min.x, body.min.y + stage_h + STAGE_GAP),
                    egui::vec2(body.width(), strip_h),
                );

                ui.allocate_ui_at_rect(stage_rect, |ui| {
                    ui.set_min_size(stage_rect.size());
                    ui.set_max_size(stage_rect.size());
                    app.show_radar_stage(ui);
                });
                ui.allocate_ui_at_rect(strip_rect, |ui| {
                    ui.set_min_size(strip_rect.size());
                    ui.set_max_size(strip_rect.size());
                    app.show_radar_status_strip(ui);
                });
            },
        );
    }

    pub(super) fn show_radar_stage(&self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        let size = egui::vec2(available.x.max(1.0), available.y.max(120.0));
        let (response, painter) = ui.allocate_painter(size, egui::Sense::hover());
        let frame = response.rect;

        painter.rect_filled(frame, 16.0, theme::map_frame());
        painter.rect_stroke(
            frame,
            16.0,
            egui::Stroke::new(1.0, theme::border()),
            egui::StrokeKind::Middle,
        );
        let content = inset_rect(frame, STAGE_PAD);
        painter.rect_filled(content, 12.0, egui::Color32::from_rgb(0x0b, 0x0d, 0x14));

        let center = content.center();
        painter.text(
            center + egui::vec2(0.0, -36.0),
            egui::Align2::CENTER_CENTER,
            "◉  Point Cloud Radar",
            egui::FontId::proportional(18.0),
            theme::text_on_dark(),
        );
        painter.text(
            center + egui::vec2(0.0, -8.0),
            egui::Align2::CENTER_CENTER,
            "Rerun Viewer 在外部窗口中显示 3D 点云",
            egui::FontId::proportional(14.0),
            theme::text_on_dark_muted(),
        );

        let has_data = self
            .pointcloud_feed
            .with_frame(|f| f.is_some())
            .unwrap_or(false);
        let status = if has_data {
            format!("Receiving · seq {}", self.pointcloud_last_seq)
        } else {
            "Waiting for SHM /pointcloud_frame …".to_string()
        };
        painter.text(
            center + egui::vec2(0.0, 24.0),
            egui::Align2::CENTER_CENTER,
            status,
            egui::FontId::proportional(13.0),
            if has_data {
                theme::GREEN
            } else {
                theme::text_on_dark_muted()
            },
        );
    }

    pub(super) fn show_radar_status_strip(&self, ui: &mut egui::Ui) {
        let points = self
            .pointcloud_feed
            .with_frame(|f| f.map(|frame| frame.points.len()).unwrap_or(0))
            .unwrap_or(0);

        ui.columns(4, |cols| {
            let cells = [
                ("SHM", "/pointcloud_frame".to_string()),
                ("Frame seq", self.pointcloud_last_seq.to_string()),
                ("Points", points.to_string()),
                ("Rerun", "optional".to_string()),
            ];
            for (i, (label, val)) in cells.into_iter().enumerate() {
                egui::Frame::new()
                    .fill(theme::card_bg())
                    .stroke(egui::Stroke::new(1.0, theme::border()))
                    .corner_radius(egui::CornerRadius::same(14))
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(&mut cols[i], |ui| {
                        ui.label(
                            egui::RichText::new(label)
                                .color(theme::text_faint())
                                .size(11.0),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(val).color(theme::text()).size(16.0));
                    });
            }
        });
    }

    pub(super) fn show_radar_status_sidebar(&mut self, ui: &mut egui::Ui) {
        let process_snapshot = self.process_control.snapshot();
        white_card(ui, "ROS2 Radar", |ui| {
            status_chip(
                ui,
                process_snapshot.radar.managed,
                if process_snapshot.radar.managed {
                    "Process running"
                } else {
                    "Process idle"
                },
            );
            ui.add_space(10.0);
            egui::Grid::new("ros2_radar_meta")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    for (label, value) in [
                        (
                            "Launch",
                            "ros2 launch radar_bringup competition.launch.py side:=…",
                        ),
                        ("Location", "ZMQ tcp://127.0.0.1:5556"),
                    ] {
                        ui.label(
                            egui::RichText::new(label)
                                .color(theme::text_faint())
                                .size(12.0),
                        );
                        ui.label(egui::RichText::new(value).color(theme::text()).size(11.0));
                        ui.end_row();
                    }
                });
        });
        ui.add_space(12.0);
        white_card(ui, "点云源", |ui| {
            let has_data = self
                .pointcloud_feed
                .with_frame(|f| f.is_some())
                .unwrap_or(false);
            status_chip(
                ui,
                has_data,
                if has_data {
                    "SHM receiving"
                } else {
                    "SHM idle"
                },
            );
            ui.add_space(10.0);
            egui::Grid::new("radar_shm_meta")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("SHM")
                            .color(theme::text_faint())
                            .size(12.0),
                    );
                    ui.label(
                        egui::RichText::new("/pointcloud_frame")
                            .color(theme::text())
                            .size(12.0),
                    );
                    ui.end_row();
                    ui.label(
                        egui::RichText::new("seq")
                            .color(theme::text_faint())
                            .size(12.0),
                    );
                    ui.label(
                        egui::RichText::new(self.pointcloud_last_seq.to_string())
                            .color(theme::text())
                            .size(12.0),
                    );
                    ui.end_row();
                });
        });
        ui.add_space(12.0);
        white_card(ui, "Rerun", |ui| {
            ui.label(
                egui::RichText::new(rerun_status_label())
                    .color(theme::text_muted())
                    .size(12.0),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Optional 3D visualization feature")
                    .color(theme::text_faint())
                    .size(11.0),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("cargo run --release --features rerun")
                    .color(theme::text_faint())
                    .size(11.0),
            );
        });
        ui.add_space(12.0);
        white_card(ui, "状态", |ui| {
            self.show_pointcloud_status(ui);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::rerun_status_label;

    #[test]
    fn rerun_status_does_not_claim_connection() {
        assert_eq!(rerun_status_label(), "optional · not monitored");
    }
}
