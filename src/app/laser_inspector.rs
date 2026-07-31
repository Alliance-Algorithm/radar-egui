use super::chrome::{status_chip, white_card};
use super::RadarApp;
use crate::laser::protocol::LaserObservation;
use crate::state::LaserSnapshot;
use crate::theme;

impl RadarApp {
    pub(super) fn show_laser_inspector_data(
        &mut self,
        ui: &mut egui::Ui,
        laser_snapshot: Option<&LaserSnapshot>,
        laser_listening: bool,
    ) {
        let laser_online = laser_snapshot.is_some_and(|s| s.online);
        let obs = laser_snapshot.map(|s| &s.observation);
        let video_available = self
            .video_feed
            .with_frame(|frame| frame.is_some())
            .unwrap_or(false)
            || self.laser_video_texture.texture().is_some();

        white_card(ui, "数据源", |ui| {
            status_chip(ui, laser_listening, "Laser ZMQ");
            ui.add_space(8.0);
            status_chip(
                ui,
                laser_online,
                if laser_online {
                    "Receiving"
                } else {
                    "No recent packets"
                },
            );
            ui.add_space(8.0);
            status_chip(
                ui,
                video_available,
                if video_available {
                    "Video SHM receiving"
                } else {
                    "Video SHM idle"
                },
            );
        });

        ui.add_space(14.0);

        white_card(ui, "相机", |ui| {
            egui::Grid::new("hikcamera_ownership")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    for (label, value) in [
                        ("Camera backend", "HikCamera"),
                        ("Configuration", "managed by laser_guidance"),
                        ("Selection", "auto when one device is present"),
                    ] {
                        ui.label(
                            egui::RichText::new(label)
                                .color(theme::text_faint())
                                .size(12.0),
                        );
                        ui.label(egui::RichText::new(value).color(theme::text()).size(12.0));
                        ui.end_row();
                    }
                });
        });

        ui.add_space(14.0);

        white_card(ui, "目标检测", |ui| {
            if let Some(obs) = obs {
                if obs.detected {
                    ui.label(
                        egui::RichText::new("已检测到目标")
                            .color(theme::GREEN)
                            .size(15.0),
                    );
                    ui.add_space(6.0);
                    egui::Grid::new("target_grid")
                        .num_columns(2)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("中心 X")
                                    .color(theme::text_muted())
                                    .size(12.0),
                            );
                            ui.label(
                                egui::RichText::new(format!("{:.1}", obs.center[0]))
                                    .color(theme::text())
                                    .size(14.0),
                            );
                            ui.end_row();
                            ui.label(
                                egui::RichText::new("中心 Y")
                                    .color(theme::text_muted())
                                    .size(12.0),
                            );
                            ui.label(
                                egui::RichText::new(format!("{:.1}", obs.center[1]))
                                    .color(theme::text())
                                    .size(14.0),
                            );
                            ui.end_row();
                            ui.label(
                                egui::RichText::new("亮度")
                                    .color(theme::text_muted())
                                    .size(12.0),
                            );
                            ui.label(
                                egui::RichText::new(format!("{:.2}", obs.brightness))
                                    .color(theme::text())
                                    .size(14.0),
                            );
                            ui.end_row();
                            ui.label(
                                egui::RichText::new("轮廓点数")
                                    .color(theme::text_muted())
                                    .size(12.0),
                            );
                            ui.label(
                                egui::RichText::new(obs.contour.len().to_string())
                                    .color(theme::text())
                                    .size(14.0),
                            );
                            ui.end_row();
                        });
                } else {
                    ui.label(
                        egui::RichText::new("未检测到目标")
                            .color(theme::text_faint())
                            .size(15.0),
                    );
                }
            } else {
                ui.label(
                    egui::RichText::new("未检测到目标")
                        .color(theme::text_faint())
                        .size(15.0),
                );
            }
        });

        ui.add_space(14.0);

        white_card(ui, "模型候选", |ui| {
            let candidates: Vec<_> = obs
                .map(|o| o.candidates.iter().collect::<Vec<_>>())
                .unwrap_or_default();
            if candidates.is_empty() {
                ui.label(
                    egui::RichText::new("无候选")
                        .color(theme::text_faint())
                        .size(15.0),
                );
            } else {
                for cand in &candidates {
                    let class_color = theme::class_color(cand.class_id);
                    egui::Frame::new()
                        .fill(theme::card_bg_muted())
                        .stroke(egui::Stroke::new(1.0, theme::border()))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} · {:.2}",
                                        LaserObservation::class_name(cand.class_id),
                                        cand.score,
                                    ))
                                    .color(class_color)
                                    .size(13.0),
                                );
                            });
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "({:.0}, {:.0})  {:.0}×{:.0}",
                                    cand.center[0], cand.center[1], cand.bbox[2], cand.bbox[3]
                                ))
                                .color(theme::text_muted())
                                .size(11.0),
                            );
                            let bar_height = 6.0;
                            let (bg_rect, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), bar_height),
                                egui::Sense::hover(),
                            );
                            let fill = class_color.gamma_multiply(0.25);
                            ui.painter().rect_filled(bg_rect, 255.0, fill);
                            if cand.score > 0.0 {
                                let w = bg_rect.width() * cand.score.min(1.0);
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(
                                        bg_rect.min,
                                        egui::vec2(w, bg_rect.height()),
                                    ),
                                    255.0,
                                    class_color,
                                );
                            }
                        });
                    ui.add_space(6.0);
                }
            }
        });
    }
}
