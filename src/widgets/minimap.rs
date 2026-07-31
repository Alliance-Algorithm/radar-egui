use egui::{Color32, Pos2, Rect, Stroke, Vec2};

use crate::services::script_runner::TeamSide;
use crate::shared_data::SharedData;
use crate::theme;
use crate::ui_layout::{inset_rect, letterbox_rect, MINIMAP_ASPECT, STAGE_PAD};

#[derive(Clone, Copy)]
pub struct MinimapOptions {
    pub show_grid: bool,
    pub show_labels: bool,
    pub show_hp_ring: bool,
    pub selected: usize,
}

impl Default for MinimapOptions {
    fn default() -> Self {
        Self {
            show_grid: true,
            show_labels: true,
            show_hp_ring: true,
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
        team_side: TeamSide,
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

        let robots = build_robot_markers(info, team_side);
        let mut screen_pts: Vec<(usize, Pos2, Color32, f32)> = Vec::with_capacity(robots.len());

        for (i, robot) in robots.iter().enumerate() {
            let screen_pos = Pos2::new(
                center.x + robot.pos[0] as f32 * scale,
                center.y - robot.pos[1] as f32 * scale,
            );
            screen_pts.push((
                i,
                screen_pos,
                robot.role_color,
                robot
                    .health
                    .and_then(hp_arc_style)
                    .map_or(0.0, |style| style.ratio),
            ));

            let selected = opts.selected == i;
            let r_core = if selected { 8.0 } else { 6.5 };
            let r_ring = if selected { 14.0 } else { 11.0 };

            painter.circle_filled(screen_pos, r_ring + 2.0, Color32::from_white_alpha(28));
            painter.circle_filled(screen_pos, r_core, robot.role_color);
            painter.circle_stroke(
                screen_pos,
                r_core,
                Stroke::new(1.5_f32, Color32::from_white_alpha(200)),
            );
            painter.circle_stroke(screen_pos, r_ring, Stroke::new(2.0_f32, robot.team_color));

            if opts.show_hp_ring {
                if let Some(style) = robot.health.and_then(hp_arc_style) {
                    let hp_radius = r_ring + 4.0;
                    painter.circle_stroke(
                        screen_pos,
                        hp_radius,
                        Stroke::new(2.5_f32, Color32::from_black_alpha(105)),
                    );
                    let segment_count = (48.0 * style.ratio).ceil().max(2.0) as usize;
                    let start = -std::f32::consts::FRAC_PI_2;
                    let sweep = std::f32::consts::TAU * style.ratio;
                    let points = (0..=segment_count)
                        .map(|segment| {
                            let angle = start + sweep * segment as f32 / segment_count as f32;
                            screen_pos + Vec2::angled(angle) * hp_radius
                        })
                        .collect();
                    painter.add(egui::Shape::line(points, Stroke::new(2.5_f32, style.color)));
                }
            }

            if selected {
                painter.circle_stroke(screen_pos, r_ring + 1.0, Stroke::new(2.0, theme::BLUE));
                painter.circle_stroke(
                    screen_pos,
                    r_ring + 4.0,
                    Stroke::new(1.0, theme::BLUE.gamma_multiply(0.35)),
                );
            }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RobotHealth {
    pub hp: u16,
    pub hp_max: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HpArcStyle {
    pub ratio: f32,
    pub color: Color32,
}

pub fn hp_arc_style(health: RobotHealth) -> Option<HpArcStyle> {
    if health.hp == 0 || health.hp_max == 0 {
        return None;
    }

    let ratio = (health.hp as f32 / health.hp_max as f32).clamp(0.0, 1.0);
    let color = if ratio > 0.6 {
        theme::GREEN
    } else if ratio > 0.3 {
        theme::YELLOW
    } else {
        theme::RED
    };
    Some(HpArcStyle { ratio, color })
}

pub fn clamp_marker_selection(selected: usize, marker_count: usize) -> usize {
    selected.min(marker_count.saturating_sub(1))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerSide {
    Ally,
    Enemy,
}

pub struct RobotMarker {
    pub name: &'static str,
    pub role_name: &'static str,
    pub side: MarkerSide,
    pub pos: [i16; 2],
    pub role_color: Color32,
    pub team_color: Color32,
    pub health: Option<RobotHealth>,
    pub ammo: Option<u16>,
}

pub fn build_robot_markers(info: &SharedData, our_side: TeamSide) -> [RobotMarker; 12] {
    let (ally_color, enemy_color) = match our_side {
        TeamSide::Red => (theme::RED, theme::BLUE),
        TeamSide::Blue => (theme::BLUE, theme::RED),
    };
    [
        RobotMarker {
            name: "我方 · 英雄",
            role_name: "英雄",
            side: MarkerSide::Ally,
            pos: [info.ally_hero.x, info.ally_hero.y],
            role_color: theme::HERO_COLOR,
            team_color: ally_color,
            health: None,
            ammo: None,
        },
        RobotMarker {
            name: "我方 · 工程",
            role_name: "工程",
            side: MarkerSide::Ally,
            pos: [info.ally_engineer.x, info.ally_engineer.y],
            role_color: theme::ENGINEER_COLOR,
            team_color: ally_color,
            health: None,
            ammo: None,
        },
        RobotMarker {
            name: "我方 · 步兵3",
            role_name: "步兵3",
            side: MarkerSide::Ally,
            pos: [info.ally_infantry_3.x, info.ally_infantry_3.y],
            role_color: theme::INFANTRY1_COLOR,
            team_color: ally_color,
            health: None,
            ammo: None,
        },
        RobotMarker {
            name: "我方 · 步兵4",
            role_name: "步兵4",
            side: MarkerSide::Ally,
            pos: [info.ally_infantry_4.x, info.ally_infantry_4.y],
            role_color: theme::INFANTRY2_COLOR,
            team_color: ally_color,
            health: None,
            ammo: None,
        },
        RobotMarker {
            name: "我方 · 无人机",
            role_name: "无人机",
            side: MarkerSide::Ally,
            pos: [info.ally_aerial.x, info.ally_aerial.y],
            role_color: theme::DRONE_COLOR,
            team_color: ally_color,
            health: None,
            ammo: None,
        },
        RobotMarker {
            name: "我方 · 哨兵",
            role_name: "哨兵",
            side: MarkerSide::Ally,
            pos: [info.ally_sentry.x, info.ally_sentry.y],
            role_color: theme::SENTINEL_COLOR,
            team_color: ally_color,
            health: None,
            ammo: None,
        },
        RobotMarker {
            name: "敌方 · 英雄",
            role_name: "英雄",
            side: MarkerSide::Enemy,
            pos: [info.enemy_hero.x, info.enemy_hero.y],
            role_color: theme::HERO_COLOR,
            team_color: enemy_color,
            health: Some(RobotHealth {
                hp: info.sdr_blood.hero_blood,
                hp_max: 200,
            }),
            ammo: Some(info.sdr_ammo.hero_ammo),
        },
        RobotMarker {
            name: "敌方 · 工程",
            role_name: "工程",
            side: MarkerSide::Enemy,
            pos: [info.enemy_engineer.x, info.enemy_engineer.y],
            role_color: theme::ENGINEER_COLOR,
            team_color: enemy_color,
            health: Some(RobotHealth {
                hp: info.sdr_blood.engineer_blood,
                hp_max: 200,
            }),
            ammo: None,
        },
        RobotMarker {
            name: "敌方 · 步兵3",
            role_name: "步兵3",
            side: MarkerSide::Enemy,
            pos: [info.enemy_infantry_3.x, info.enemy_infantry_3.y],
            role_color: theme::INFANTRY1_COLOR,
            team_color: enemy_color,
            health: Some(RobotHealth {
                hp: info.sdr_blood.infantry_3_blood,
                hp_max: 200,
            }),
            ammo: Some(info.sdr_ammo.infantry_3_ammo),
        },
        RobotMarker {
            name: "敌方 · 步兵4",
            role_name: "步兵4",
            side: MarkerSide::Enemy,
            pos: [info.enemy_infantry_4.x, info.enemy_infantry_4.y],
            role_color: theme::INFANTRY2_COLOR,
            team_color: enemy_color,
            health: Some(RobotHealth {
                hp: info.sdr_blood.infantry_4_blood,
                hp_max: 200,
            }),
            ammo: Some(info.sdr_ammo.infantry_4_ammo),
        },
        RobotMarker {
            name: "敌方 · 无人机",
            role_name: "无人机",
            side: MarkerSide::Enemy,
            pos: [info.enemy_aerial.x, info.enemy_aerial.y],
            role_color: theme::DRONE_COLOR,
            team_color: enemy_color,
            health: None,
            ammo: Some(info.sdr_ammo.aerial_ammo),
        },
        RobotMarker {
            name: "敌方 · 哨兵",
            role_name: "哨兵",
            side: MarkerSide::Enemy,
            pos: [info.enemy_sentry.x, info.enemy_sentry.y],
            role_color: theme::SENTINEL_COLOR,
            team_color: enemy_color,
            health: Some(RobotHealth {
                hp: info.sdr_blood.sentry_blood,
                hp_max: 400,
            }),
            ammo: Some(info.sdr_ammo.sentry_ammo),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::script_runner::TeamSide;

    #[test]
    fn red_team_builds_twelve_markers_with_red_allies_and_blue_enemies() {
        let mut data = SharedData::default();
        data.ally_hero.x = 100;
        data.enemy_hero.x = 900;
        data.sdr_blood.hero_blood = 150;
        data.sdr_ammo.hero_ammo = 42;

        let markers = build_robot_markers(&data, TeamSide::Red);
        assert_eq!(markers.len(), 12);
        assert_eq!(markers[0].side, MarkerSide::Ally);
        assert_eq!(markers[0].team_color, theme::RED);
        assert_eq!(markers[0].pos, [100, 0]);
        assert!(markers[0].health.is_none());
        assert!(markers[0].ammo.is_none());
        assert_eq!(markers[6].side, MarkerSide::Enemy);
        assert_eq!(markers[6].team_color, theme::BLUE);
        assert_eq!(markers[6].pos, [900, 0]);
        assert_eq!(markers[6].health.unwrap().hp, 150);
        assert_eq!(markers[6].ammo, Some(42));
    }

    #[test]
    fn blue_team_swaps_ally_and_enemy_ring_colors() {
        let markers = build_robot_markers(&SharedData::default(), TeamSide::Blue);
        assert_eq!(markers[0].team_color, theme::BLUE);
        assert_eq!(markers[6].team_color, theme::RED);
    }

    #[test]
    fn hp_arc_clamps_ratio_and_uses_green_yellow_red_thresholds() {
        assert_eq!(
            hp_arc_style(RobotHealth {
                hp: 250,
                hp_max: 200
            })
            .unwrap()
            .ratio,
            1.0
        );
        assert_eq!(
            hp_arc_style(RobotHealth {
                hp: 150,
                hp_max: 200
            })
            .unwrap()
            .color,
            theme::GREEN
        );
        assert_eq!(
            hp_arc_style(RobotHealth {
                hp: 100,
                hp_max: 200
            })
            .unwrap()
            .color,
            theme::YELLOW
        );
        assert_eq!(
            hp_arc_style(RobotHealth {
                hp: 40,
                hp_max: 200
            })
            .unwrap()
            .color,
            theme::RED
        );
        assert!(hp_arc_style(RobotHealth { hp: 0, hp_max: 200 }).is_none());
        assert!(hp_arc_style(RobotHealth { hp: 20, hp_max: 0 }).is_none());
    }

    #[test]
    fn marker_selection_clamps_to_twelve_marker_boundary() {
        assert_eq!(clamp_marker_selection(99, 12), 11);
        assert_eq!(clamp_marker_selection(5, 12), 5);
        assert_eq!(clamp_marker_selection(3, 0), 0);
    }
}
