use egui::{Color32, Pos2, Rect, Stroke, Vec2};

use crate::theme;
use crate::ui_layout::{inset_rect, letterbox_rect, MINIMAP_ASPECT, STAGE_PAD};
use crate::zmq::data_format::ReceiveSdr;

#[derive(Clone, Copy)]
pub struct MinimapOptions {
    pub show_grid: bool,
    pub show_labels: bool,
    pub show_heat: bool,
    pub selected: usize,
}

impl Default for MinimapOptions {
    fn default() -> Self {
        Self {
            show_grid: true,
            show_labels: true,
            show_heat: true,
            selected: 0,
        }
    }
}

pub struct MinimapWidget;

impl MinimapWidget {
    pub fn new() -> Self {
        Self
    }

    pub fn show_with_state(
        &self,
        ui: &mut egui::Ui,
        info: Option<&ReceiveSdr>,
        background: Option<&egui::TextureHandle>,
        pan: &mut Vec2,
        zoom: &mut f32,
        opts: MinimapOptions,
        selected_out: &mut usize,
    ) {
        let available = ui.available_size();
        let size = Vec2::new(available.x.max(1.0), available.y.max(1.0));
        let (response, painter) = ui.allocate_painter(size, egui::Sense::click_and_drag());
        let frame_rect = response.rect;

        painter.rect_filled(frame_rect, 16.0, theme::map_frame());
        painter.rect_stroke(
            frame_rect,
            16.0,
            Stroke::new(1.0, theme::border()),
            egui::StrokeKind::Middle,
        );

        let content = inset_rect(frame_rect, STAGE_PAD);
        let board_rect = letterbox_rect(content, MINIMAP_ASPECT);

        let world_rect = if let Some(background) = background {
            painter.rect_filled(board_rect, 10.0, Color32::from_rgb(0x2a, 0x2e, 0x36));
            if response.hovered() {
                let scroll_delta = ui.ctx().input(|input| input.raw_scroll_delta.y);
                if scroll_delta.abs() > f32::EPSILON {
                    let zoom_factor = (1.0 + scroll_delta * 0.0015).clamp(0.9, 1.1);
                    *zoom = (*zoom * zoom_factor).clamp(0.45, 3.0);
                }
            }

            if response.dragged() {
                *pan += ui.ctx().input(|input| input.pointer.delta());
            }

            let texture_size = background.size_vec2();
            let fit_scale =
                (board_rect.width() / texture_size.x).min(board_rect.height() / texture_size.y);
            let image_size = texture_size * fit_scale * *zoom;
            let max_x = ((image_size.x - board_rect.width()) * 0.5).max(0.0);
            let max_y = ((image_size.y - board_rect.height()) * 0.5).max(0.0);
            pan.x = pan.x.clamp(-max_x, max_x);
            pan.y = pan.y.clamp(-max_y, max_y);

            let image_rect = Rect::from_center_size(board_rect.center() + *pan, image_size);
            painter.image(
                background.id(),
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            if opts.show_grid {
                self.draw_grid(&painter, image_rect.intersect(board_rect));
            }
            image_rect
        } else {
            if response.dragged() {
                *pan = Vec2::ZERO;
            }
            painter.rect_filled(board_rect, 10.0, theme::map_bg());
            if opts.show_grid {
                self.draw_grid(&painter, board_rect);
            }
            board_rect
        };

        let Some(info) = info else {
            return;
        };
        let center = world_rect.center();
        let scale = world_rect.width().min(world_rect.height()) * 0.43 / 3000.0;

        let robots = robot_markers(info);
        let mut screen_pts: Vec<(usize, Pos2, Color32, f32)> = Vec::with_capacity(robots.len());

        for (i, robot) in robots.iter().enumerate() {
            let screen_pos = Pos2::new(
                center.x + robot.pos[0] as f32 * scale,
                center.y - robot.pos[1] as f32 * scale,
            );
            let hp = robot
                .health
                .map(|h| (h.hp as f32 / h.hp_max as f32).clamp(0.0, 1.0));
            screen_pts.push((i, screen_pos, robot.color, hp.map_or(0.0, |value| value)));

            let selected = opts.selected == i;
            let r_core = if selected { 8.0 } else { 6.5 };
            let r_ring = if selected { 14.0 } else { 11.0 };

            painter.circle_filled(screen_pos, r_ring + 2.0, Color32::from_white_alpha(28));

            if opts.show_heat {
                if let Some(hp) = hp {
                    let heat = (1.0 - hp).clamp(0.0, 1.0);
                    if heat > 0.05 {
                        painter.circle_stroke(
                            screen_pos,
                            r_ring + 3.0 + heat * 4.0,
                            Stroke::new(2.0, theme::RED.gamma_multiply(0.35 + heat * 0.45)),
                        );
                    }
                }
            }

            if selected {
                painter.circle_stroke(screen_pos, r_ring + 1.0, Stroke::new(2.0, theme::BLUE));
                painter.circle_stroke(
                    screen_pos,
                    r_ring + 4.0,
                    Stroke::new(1.0, theme::BLUE.gamma_multiply(0.35)),
                );
            } else {
                painter.circle_stroke(
                    screen_pos,
                    r_ring,
                    Stroke::new(1.0, robot.color.gamma_multiply(0.35)),
                );
            }

            painter.circle_filled(screen_pos, r_core, robot.color);
            painter.circle_stroke(
                screen_pos,
                r_core,
                Stroke::new(1.5, Color32::from_white_alpha(200)),
            );

            if opts.show_labels {
                painter.text(
                    screen_pos + Vec2::new(14.0, -12.0),
                    egui::Align2::LEFT_CENTER,
                    robot.name,
                    egui::FontId::proportional(13.0),
                    theme::text_on_dark_muted(),
                );
            }
        }

        if response.clicked() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let mut best: Option<(usize, f32)> = None;
                for (i, pos, _, _) in &screen_pts {
                    let d = pos.distance(pointer);
                    if d < 22.0 {
                        if best.is_none_or(|(_, bd)| d < bd) {
                            best = Some((*i, d));
                        }
                    }
                }
                if let Some((i, _)) = best {
                    *selected_out = i;
                }
            }
        }
    }

    fn draw_grid(&self, painter: &egui::Painter, rect: Rect) {
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            let x = rect.left() + t * rect.width();
            let y = rect.top() + t * rect.height();
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(
                    if i == 5 { 1.0 } else { 0.6 },
                    if i == 5 {
                        theme::grid_strong().gamma_multiply(0.45)
                    } else {
                        theme::grid().gamma_multiply(0.35)
                    },
                ),
            );
            painter.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(
                    if i == 5 { 1.0 } else { 0.6 },
                    if i == 5 {
                        theme::grid_strong().gamma_multiply(0.45)
                    } else {
                        theme::grid().gamma_multiply(0.35)
                    },
                ),
            );
        }
    }
}

#[derive(Clone, Copy)]
pub struct RobotHealth {
    pub hp: u16,
    pub hp_max: u16,
}

pub struct RobotMarker {
    pub name: &'static str,
    pub pos: [i16; 2],
    pub color: Color32,
    pub health: Option<RobotHealth>,
    pub ammo: u16,
}

pub fn robot_markers(info: &ReceiveSdr) -> [RobotMarker; 6] {
    [
        RobotMarker {
            name: "英雄",
            pos: [info.position.hero_x, info.position.hero_y],
            color: theme::HERO_COLOR,
            health: Some(RobotHealth {
                hp: info.blood.hero_blood,
                hp_max: 200,
            }),
            ammo: info.ammo.hero_ammo,
        },
        RobotMarker {
            name: "工程",
            pos: [info.position.engineer_x, info.position.engineer_y],
            color: theme::ENGINEER_COLOR,
            health: Some(RobotHealth {
                hp: info.blood.engineer_blood,
                hp_max: 200,
            }),
            ammo: 0,
        },
        RobotMarker {
            name: "步兵1",
            pos: [info.position.infantry_3_x, info.position.infantry_3_y],
            color: theme::INFANTRY1_COLOR,
            health: Some(RobotHealth {
                hp: info.blood.infantry_3_blood,
                hp_max: 200,
            }),
            ammo: info.ammo.infantry_3_ammo,
        },
        RobotMarker {
            name: "步兵2",
            pos: [info.position.infantry_4_x, info.position.infantry_4_y],
            color: theme::INFANTRY2_COLOR,
            health: Some(RobotHealth {
                hp: info.blood.infantry_4_blood,
                hp_max: 200,
            }),
            ammo: info.ammo.infantry_4_ammo,
        },
        RobotMarker {
            name: "无人机",
            pos: [info.position.aerial_x, info.position.aerial_y],
            color: theme::DRONE_COLOR,
            health: None,
            ammo: info.ammo.aerial_ammo,
        },
        RobotMarker {
            name: "哨兵",
            pos: [info.position.sentry_x, info.position.sentry_y],
            color: theme::SENTINEL_COLOR,
            health: Some(RobotHealth {
                hp: info.blood.sentry_blood,
                hp_max: 400,
            }),
            ammo: info.ammo.sentry_ammo,
        },
    ]
}

pub fn demo_receive_sdr() -> ReceiveSdr {
    use crate::zmq::data_format::*;
    ReceiveSdr {
        cmd_id: 0x2002,
        position: ReceiveSdrPosition {
            hero_x: -420,
            hero_y: 180,
            engineer_x: -780,
            engineer_y: -320,
            infantry_3_x: -180,
            infantry_3_y: 420,
            infantry_4_x: 520,
            infantry_4_y: 120,
            aerial_x: 80,
            aerial_y: 620,
            sentry_x: 780,
            sentry_y: -180,
        },
        blood: ReceiveSdrBlood {
            hero_blood: 168,
            engineer_blood: 200,
            infantry_3_blood: 140,
            infantry_4_blood: 55,
            reserved: 150,
            sentry_blood: 360,
        },
        ammo: ReceiveSdrAmmo {
            hero_ammo: 86,
            infantry_3_ammo: 120,
            infantry_4_ammo: 45,
            aerial_ammo: 30,
            sentry_ammo: 200,
        },
        state: ReceiveSdrState {
            remaining_gold: 320,
            total_gold: 800,
            occupation_status: [1, 1, 1, 0, 0, 1],
            ..Default::default()
        },
        gain: ReceiveSdrGain {
            hero_hp_recovery: 1,
            hero_cooling_acceleration: 120,
            hero_defence: 2,
            hero_negative_defence: 0,
            hero_attack: 15,
            engineer_hp_recovery: 0,
            engineer_cooling_acceleration: 80,
            engineer_defence: 1,
            engineer_negative_defence: 0,
            engineer_attack: 0,
            infantry_3_hp_recovery: 1,
            infantry_3_cooling_acceleration: 100,
            infantry_3_defence: 1,
            infantry_3_negative_defence: 0,
            infantry_3_attack: 10,
            infantry_4_hp_recovery: 0,
            infantry_4_cooling_acceleration: 60,
            infantry_4_defence: 0,
            infantry_4_negative_defence: 1,
            infantry_4_attack: 5,
            sentry_hp_recovery: 2,
            sentry_cooling_acceleration: 140,
            sentry_defence: 3,
            sentry_negative_defence: 0,
            sentry_attack: 20,
            sentry_posture: 0,
            ..Default::default()
        },
        key: ReceiveSdrKey::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zmq::data_format::ReceiveSdr;

    #[test]
    fn aerial_robot_health_is_none_not_fabricated() {
        let sdr = ReceiveSdr::default();
        let markers = robot_markers(&sdr);
        let aerial = &markers[4];
        assert!(
            aerial.health.is_none(),
            "aerial health should be absent, not fabricated"
        );
        for i in 0..markers.len() {
            if i == 4 {
                continue;
            }
            assert!(
                markers[i].health.is_some(),
                "non-aerial robot {} should have health data",
                i
            );
        }
    }
}
