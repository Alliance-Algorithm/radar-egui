use super::RadarApp;
use crate::laser::protocol::{LaserObservation, ModelCandidate};
use crate::theme;
use crate::ui_layout::{inset_rect, letterbox_rect, STAGE_PAD, VIDEO_ASPECT};
use egui::{Color32, Pos2, Rect, Vec2};

fn demo_laser_observation() -> LaserObservation {
    LaserObservation {
        detected: true,
        center: [640.0, 360.0],
        brightness: 0.82,
        contour: Vec::new(),
        candidates: vec![
            ModelCandidate {
                score: 0.91,
                class_id: 0,
                bbox: [730.0, 346.0, 346.0, 302.0],
                center: [640.0, 360.0],
            },
            ModelCandidate {
                score: 0.74,
                class_id: 1,
                bbox: [960.0, 486.0, 230.0, 216.0],
                center: [700.0, 400.0],
            },
        ],
        received_at: Some(std::time::Instant::now()),
    }
}

fn tool_chip(ui: &mut egui::Ui, label: &str, on: &mut bool) {
    let fill = if *on {
        Color32::from_rgba_premultiplied(47, 107, 255, 255)
    } else {
        theme::card_bg().gamma_multiply(0.92)
    };
    let text = if *on {
        theme::text_on_dark()
    } else {
        theme::text_muted()
    };
    let stroke = if *on {
        egui::Stroke::NONE
    } else {
        egui::Stroke::new(1.0, theme::border())
    };
    let resp = egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(egui::CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).color(text).size(11.0));
        })
        .response
        .interact(egui::Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
}

fn live_badge(ui: &mut egui::Ui, live_ok: bool) {
    egui::Frame::new()
        .fill(theme::card_bg().gamma_multiply(0.92))
        .stroke(egui::Stroke::new(1.0, theme::border()))
        .corner_radius(egui::CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            let (dot_color, label) = if live_ok {
                (theme::GREEN, "Receiving")
            } else {
                (theme::RED, "No recent packets")
            };
            ui.horizontal(|ui| {
                ui.painter().circle_filled(
                    ui.cursor().left_center() + egui::vec2(4.0, 0.0),
                    3.5,
                    dot_color,
                );
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(label)
                        .color(theme::text_muted())
                        .size(11.0),
                );
            });
        });
}

fn compute_scale(texture: Option<&egui::TextureHandle>, video_rect: Rect) -> (f32, f32) {
    if let Some(tex) = texture {
        let tex_size = tex.size_vec2();
        (
            video_rect.width() / tex_size.x.max(1.0),
            video_rect.height() / tex_size.y.max(1.0),
        )
    } else {
        (
            video_rect.width() / 1920.0f32.max(1.0),
            video_rect.height() / 1080.0f32.max(1.0),
        )
    }
}

impl RadarApp {
    pub(super) fn show_laser_stage(
        &mut self,
        ui: &mut egui::Ui,
        live_obs: Option<&LaserObservation>,
        texture: Option<&egui::TextureHandle>,
    ) {
        let stage_rect = ui.available_rect_before_wrap();
        ui.allocate_ui_at_rect(stage_rect, |ui| {
            ui.set_min_size(stage_rect.size());
            ui.set_max_size(stage_rect.size());

            let painter = ui.painter();

            painter.rect_filled(stage_rect, 16.0, theme::panel_bg());
            painter.rect_stroke(
                stage_rect,
                16.0,
                egui::Stroke::new(1.0, theme::border()),
                egui::StrokeKind::Middle,
            );

            let content = inset_rect(stage_rect, STAGE_PAD);
            let video_rect = letterbox_rect(content, VIDEO_ASPECT);
            painter.rect_filled(video_rect, 10.0, Color32::from_rgb(0x0f, 0x12, 0x18));

            let obs = if self.laser_stage_demo {
                Some(demo_laser_observation())
            } else {
                live_obs.cloned()
            };

            let has_live_feed = obs.as_ref().is_some_and(|o| o.is_online());

            if let Some(tex) = texture {
                painter.image(
                    tex.id(),
                    video_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }

            if !has_live_feed && !self.laser_stage_demo {
                let center = video_rect.center();
                painter.text(
                    Pos2::new(center.x, center.y - 10.0),
                    egui::Align2::CENTER_CENTER,
                    "等待视频流…",
                    egui::FontId::proportional(18.0),
                    theme::text_faint(),
                );
                painter.text(
                    Pos2::new(center.x, center.y + 16.0),
                    egui::Align2::CENTER_CENTER,
                    "SHM /laser_frame · 16:9 letterbox",
                    egui::FontId::proportional(12.0),
                    theme::text_faint().gamma_multiply(0.8),
                );
            }

            if has_live_feed && self.laser_stage_overlay {
                let (sx, sy) = compute_scale(texture, video_rect);
                if let Some(obs) = obs.as_ref() {
                    for cand in &obs.candidates {
                        let color = theme::class_color(cand.class_id);
                        let x = video_rect.left() + cand.bbox[0] * sx;
                        let y = video_rect.top() + cand.bbox[1] * sy;
                        let w = cand.bbox[2] * sx;
                        let h = cand.bbox[3] * sy;
                        let bbox_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
                        painter.rect_stroke(
                            bbox_rect,
                            4.0,
                            (2.0, color),
                            egui::StrokeKind::Outside,
                        );
                        let label = format!(
                            "{} · {:.2}",
                            LaserObservation::class_name(cand.class_id),
                            cand.score
                        );
                        painter.text(
                            Pos2::new(x, y - 4.0),
                            egui::Align2::LEFT_BOTTOM,
                            &label,
                            egui::FontId::proportional(11.0),
                            color,
                        );
                    }
                    if obs.detected {
                        let cx = video_rect.left() + obs.center[0] * sx;
                        let cy = video_rect.top() + obs.center[1] * sy;
                        painter.circle_stroke(
                            Pos2::new(cx, cy),
                            9.0,
                            egui::Stroke::new(1.5, theme::GREEN),
                        );
                        painter.line_segment(
                            [Pos2::new(cx - 12.0, cy), Pos2::new(cx + 12.0, cy)],
                            (1.0, theme::GREEN),
                        );
                        painter.line_segment(
                            [Pos2::new(cx, cy - 12.0), Pos2::new(cx, cy + 12.0)],
                            (1.0, theme::GREEN),
                        );
                    }
                }
            }

            let tools_pos = stage_rect.min + egui::vec2(18.0, 18.0);
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                    tools_pos,
                    egui::vec2(240.0, 36.0),
                )),
                |ui| {
                    ui.horizontal(|ui| {
                        tool_chip(ui, "Overlay", &mut self.laser_stage_overlay);
                        tool_chip(ui, "Demo frame", &mut self.laser_stage_demo);
                    });
                },
            );

            let live_ok = has_live_feed || self.laser_stage_demo;
            let badge_w = 160.0;
            let badge_pos =
                egui::pos2(stage_rect.right() - badge_w - 18.0, stage_rect.top() + 18.0);
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                    badge_pos,
                    egui::vec2(badge_w, 30.0),
                )),
                |ui| {
                    live_badge(ui, live_ok);
                },
            );
        });
    }
}
