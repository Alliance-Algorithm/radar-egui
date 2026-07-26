use egui::{Color32, Pos2, Rect, Stroke, Vec2};

use crate::shared_data::SharedData;
use crate::theme;
use crate::ui_layout::{inset_rect, letterbox_rect, MINIMAP_ASPECT, STAGE_PAD};

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
        info: Option<&SharedData>,
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

        let robots = build_robot_markers(info);
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

pub fn build_robot_markers(info: &SharedData) -> [RobotMarker; 6] {
    [
        RobotMarker {
            name: "英雄",
            pos: [info.enemy_hero.x, info.enemy_hero.y],
            color: theme::HERO_COLOR,
            health: Some(RobotHealth {
                hp: info.sdr_blood.hero_blood,
                hp_max: 200,
            }),
            ammo: info.sdr_ammo.hero_ammo,
        },
        RobotMarker {
            name: "工程",
            pos: [info.enemy_engineer.x, info.enemy_engineer.y],
            color: theme::ENGINEER_COLOR,
            health: Some(RobotHealth {
                hp: info.sdr_blood.engineer_blood,
                hp_max: 200,
            }),
            ammo: 0,
        },
        RobotMarker {
            name: "步兵1",
            pos: [info.enemy_infantry_3.x, info.enemy_infantry_3.y],
            color: theme::INFANTRY1_COLOR,
            health: Some(RobotHealth {
                hp: info.sdr_blood.infantry_3_blood,
                hp_max: 200,
            }),
            ammo: info.sdr_ammo.infantry_3_ammo,
        },
        RobotMarker {
            name: "步兵2",
            pos: [info.enemy_infantry_4.x, info.enemy_infantry_4.y],
            color: theme::INFANTRY2_COLOR,
            health: Some(RobotHealth {
                hp: info.sdr_blood.infantry_4_blood,
                hp_max: 200,
            }),
            ammo: info.sdr_ammo.infantry_4_ammo,
        },
        RobotMarker {
            name: "无人机",
            pos: [info.enemy_aerial.x, info.enemy_aerial.y],
            color: theme::DRONE_COLOR,
            health: None,
            ammo: info.sdr_ammo.aerial_ammo,
        },
        RobotMarker {
            name: "哨兵",
            pos: [info.enemy_sentry.x, info.enemy_sentry.y],
            color: theme::SENTINEL_COLOR,
            health: Some(RobotHealth {
                hp: info.sdr_blood.sentry_blood,
                hp_max: 400,
            }),
            ammo: info.sdr_ammo.sentry_ammo,
        },
    ]
}
