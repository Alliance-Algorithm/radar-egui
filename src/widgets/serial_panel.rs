use std::collections::VecDeque;

use egui::{RichText, Ui};

use crate::shared_data::{
    GameStateData, MinimapReceiveRadarData, RadarMarkProcessData, SharedData, SiteEventData,
};
use crate::theme;

pub struct SerialPanel;

#[derive(Clone)]
pub struct SerialFrameLogLine {
    pub text: String,
    pub kind: SerialLogKind,
}

#[derive(Clone, Copy)]
pub enum SerialLogKind {
    Ok,
    Err,
    Rx,
    Tx,
    Info,
}

impl SerialPanel {
    pub fn new() -> Self {
        Self
    }

    pub fn show_monitor(
        &self,
        ui: &mut Ui,
        data: &SharedData,
        serial_open: bool,
        port_label: &str,
        baud: u32,
        frame_log: &VecDeque<SerialFrameLogLine>,
    ) {
        let body = ui.available_rect_before_wrap();
        ui.allocate_ui_at_rect(body, |ui| {
            ui.set_min_size(body.size());
            ui.set_max_size(body.size());

            let metrics_h = 86.0;
            let gap = 10.0;
            let rest_h = (body.height() - metrics_h - gap).max(200.0);
            let metrics_rect =
                egui::Rect::from_min_size(body.min, egui::vec2(body.width(), metrics_h));
            let stage_rect = egui::Rect::from_min_size(
                egui::pos2(body.min.x, body.min.y + metrics_h + gap),
                egui::vec2(body.width(), rest_h),
            );

            ui.allocate_ui_at_rect(metrics_rect, |ui| {
                ui.set_min_size(metrics_rect.size());
                ui.set_max_size(metrics_rect.size());
                self.show_hero_metrics(ui, serial_open, port_label, baud);
            });

            ui.allocate_ui_at_rect(stage_rect, |ui| {
                ui.set_min_size(stage_rect.size());
                ui.set_max_size(stage_rect.size());
                let half_w = (stage_rect.width() - gap) * 0.5;
                let half_h = (stage_rect.height() - gap) * 0.5;
                let tl = egui::Rect::from_min_size(stage_rect.min, egui::vec2(half_w, half_h));
                let tr = egui::Rect::from_min_size(
                    egui::pos2(stage_rect.min.x + half_w + gap, stage_rect.min.y),
                    egui::vec2(half_w, half_h),
                );
                let bl = egui::Rect::from_min_size(
                    egui::pos2(stage_rect.min.x, stage_rect.min.y + half_h + gap),
                    egui::vec2(half_w, half_h),
                );
                let br = egui::Rect::from_min_size(
                    egui::pos2(
                        stage_rect.min.x + half_w + gap,
                        stage_rect.min.y + half_h + gap,
                    ),
                    egui::vec2(half_w, half_h),
                );

                ui.allocate_ui_at_rect(tl, |ui| {
                    ui.set_min_size(tl.size());
                    ui.set_max_size(tl.size());
                    self.card(ui, "比赛状态", "0x0001 GameState · 1 Hz", |ui| {
                        show_game_state(ui, &data.game_state, data.game_result.winner);
                    });
                });
                ui.allocate_ui_at_rect(tr, |ui| {
                    ui.set_min_size(tr.size());
                    ui.set_max_size(tr.size());
                    self.card(ui, "场地事件", "0x0101 SiteEvent · 1 Hz", |ui| {
                        show_site_events(ui, &data.site_event, &data.dart_launch);
                    });
                });
                ui.allocate_ui_at_rect(bl, |ui| {
                    ui.set_min_size(bl.size());
                    ui.set_max_size(bl.size());
                    self.card(
                        ui,
                        "雷达标记",
                        "0x020C MarkProcess · 12 机易伤 / 标记",
                        |ui| {
                            show_mark_grid(ui, &data.radar_mark_process);
                        },
                    );
                });
                ui.allocate_ui_at_rect(br, |ui| {
                    ui.set_min_size(br.size());
                    ui.set_max_size(br.size());
                    self.card(ui, "帧日志", "SOF 0xA5 · CRC8/16 · 最近帧", |ui| {
                        show_frame_log(ui, frame_log);
                    });
                });
            });
        });
    }

    fn show_hero_metrics(&self, ui: &mut Ui, serial_open: bool, port_label: &str, baud: u32) {
        ui.columns(4, |cols| {
            let link_sub = if serial_open {
                format!("{port_label} @ {baud} — active until app exit")
            } else {
                "未打开串口".into()
            };
            metric_card(
                &mut cols[0],
                "链路",
                if serial_open { "Active" } else { "Idle" },
                &link_sub,
                if serial_open {
                    theme::GREEN
                } else {
                    theme::text()
                },
            );
            metric_card(
                &mut cols[1],
                "帧率",
                "—",
                "frames / s · 无 parser 统计",
                theme::text(),
            );
            metric_card(
                &mut cols[2],
                "CRC 错误",
                "—",
                "累计 · 无 parser 统计",
                theme::text(),
            );
            metric_card(&mut cols[3], "吞吐", "—", "RX · last 1s", theme::text());
        });
    }

    pub fn show_minimap_sidebar(&self, ui: &mut Ui, mini: &MinimapReceiveRadarData) {
        self.card(ui, "小地图雷达 0x0305", "12 × (x, y) u16", |ui| {
            show_minimap_table(ui, mini);
        });
    }

    pub fn show_dirty_flags(&self, ui: &mut Ui, data: &SharedData) {
        self.card(ui, "脏标志", "serial_produced / zmq_produced", |ui| {
            egui::Grid::new("serial_dirty_meta")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("serial_produced")
                            .color(theme::text_faint())
                            .size(12.0),
                    );
                    ui.label(
                        RichText::new(bits_preview(&data.serial_produced))
                            .color(theme::text())
                            .size(12.0),
                    );
                    ui.end_row();
                    ui.label(
                        RichText::new("zmq_produced")
                            .color(theme::text_faint())
                            .size(12.0),
                    );
                    ui.label(
                        RichText::new(bits_preview(&data.zmq_produced))
                            .color(theme::text())
                            .size(12.0),
                    );
                    ui.end_row();
                    ui.label(
                        RichText::new("last cmd")
                            .color(theme::text_faint())
                            .size(12.0),
                    );
                    ui.label(
                        RichText::new(last_cmd_hint(&data.serial_produced))
                            .color(theme::text())
                            .size(12.0),
                    );
                    ui.end_row();
                });
        });
    }

    fn card(&self, ui: &mut Ui, title: &str, subtitle: &str, add: impl FnOnce(&mut Ui)) {
        let avail = ui.available_size();
        egui::Frame::new()
            .fill(theme::card_bg())
            .stroke(egui::Stroke::new(1.0, theme::border()))
            .corner_radius(egui::CornerRadius::same(16))
            .shadow(egui::epaint::Shadow {
                offset: [0, 6],
                blur: 18,
                spread: 0,
                color: theme::shadow(),
            })
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_min_size(avail);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(title).color(theme::text()).size(15.0));
                    ui.label(
                        RichText::new(subtitle)
                            .color(theme::text_muted())
                            .size(11.0),
                    );
                });
                ui.add_space(10.0);
                add(ui);
            });
    }
}

fn metric_card(ui: &mut Ui, label: &str, val: &str, sub: &str, val_color: egui::Color32) {
    egui::Frame::new()
        .fill(theme::card_bg())
        .stroke(egui::Stroke::new(1.0, theme::border()))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(label).color(theme::text_faint()).size(11.0));
            ui.add_space(4.0);
            ui.label(RichText::new(val).color(val_color).size(18.0));
            ui.add_space(2.0);
            ui.label(RichText::new(sub).color(theme::text_muted()).size(11.0));
        });
}

fn show_game_state(ui: &mut Ui, gs: &GameStateData, winner: u8) {
    let phase = phase_label(gs.game_progress);
    let remain = gs.stage_remain_time;
    let m = remain / 60;
    let s = remain % 60;
    let total = 420u16;
    let elapsed = total.saturating_sub(remain.min(total));
    let pct = if total == 0 {
        0.0
    } else {
        f32::from(elapsed) / f32::from(total)
    };

    ui.horizontal(|ui| {
        let ring = ui.available_height().min(72.0).max(56.0);
        let (ring_rect, _) = ui.allocate_exact_size(egui::vec2(ring, ring), egui::Sense::hover());
        draw_phase_ring(ui, ring_rect, pct);
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(phase).color(theme::text()).size(18.0));
            ui.label(
                RichText::new(format!(
                    "剩余 {m:02}:{s:02} · UNIX {}",
                    if gs.sync_timestamp == 0 {
                        "—".into()
                    } else {
                        gs.sync_timestamp.to_string()
                    }
                ))
                .color(theme::text_muted())
                .size(12.0),
            );
        });
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("比赛进度")
                .color(theme::text_muted())
                .size(12.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{elapsed} / {total} s"))
                    .color(theme::text_muted())
                    .size(12.0),
            );
        });
    });
    progress_bar(ui, pct.clamp(0.0, 1.0), theme::BLUE);
    ui.add_space(10.0);
    egui::Grid::new("serial_game_meta")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            meta_row(ui, "game_type", &gs.game_type.to_string());
            meta_row(ui, "game_progress", &gs.game_progress.to_string());
            meta_row(ui, "winner", &winner_label(winner));
        });
}

fn draw_phase_ring(ui: &Ui, rect: egui::Rect, pct: f32) {
    let painter = ui.painter();
    let center = rect.center();
    let r = rect.width() * 0.5;
    painter.circle_filled(
        center,
        r,
        egui::Color32::from_rgba_unmultiplied(47, 107, 255, 28),
    );
    let start = -std::f32::consts::FRAC_PI_2;
    let end = start + pct.clamp(0.0, 1.0) * std::f32::consts::TAU;
    let steps = 48;
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let a = start + (end - start) * t;
        pts.push(center + egui::vec2(a.cos(), a.sin()) * (r - 2.0));
    }
    if pts.len() >= 2 {
        painter.add(egui::Shape::line(pts, egui::Stroke::new(4.0, theme::BLUE)));
    }
    painter.circle_filled(center, r * 0.72, theme::card_bg());
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        format!("{:.0}%", pct * 100.0),
        egui::FontId::proportional(13.0),
        theme::BLUE,
    );
}

fn show_site_events(
    ui: &mut Ui,
    site: &SiteEventData,
    dart: &crate::shared_data::DartLaunchData,
) {
    ui.horizontal_wrapped(|ui| {
        event_chip(ui, "补给站", site.supply_zone_status != 0);
        event_chip(
            ui,
            "能量机关",
            site.energy_small_status != 0 || site.energy_large_status != 0,
        );
        event_chip(
            ui,
            "高地",
            site.central_highland_status != 0 || site.trapezoid_highland_status != 0,
        );
        event_chip(
            ui,
            "飞镖击中",
            site.dart_hit_target != 0 || dart.dart_hit_count != 0,
        );
        event_chip(ui, "前哨站", site.outpost_gain_status != 0);
        event_chip(ui, "基地护甲", site.base_gain_status != 0);
        event_chip(
            ui,
            "增益点",
            site.center_gain_status != 0 || site.outpost_gain_status != 0,
        );
        event_chip(ui, "堡垒", site.fortress_gain_status != 0);
    });
    ui.add_space(12.0);
    egui::Grid::new("serial_dart_meta")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            let remain = if dart.dart_remaining_time == 0 {
                "—".to_string()
            } else {
                dart.dart_remaining_time.to_string()
            };
            let hit = if dart.dart_hit_target == 0 {
                "—".to_string()
            } else {
                dart.dart_hit_target.to_string()
            };
            meta_row(ui, "飞镖剩余", &remain);
            meta_row(ui, "击中目标", &hit);
        });
}

fn show_mark_grid(ui: &mut Ui, mark: &RadarMarkProcessData) {
    let red: [(&str, bool, bool); 6] = [
        ("敌 1", mark.opponent_hero_vulnerable != 0, false),
        ("敌 2", mark.opponent_engineer_vulnerable != 0, false),
        ("敌 3", mark.opponent_infantry_3_vulnerable != 0, false),
        ("敌 4", mark.opponent_infantry_4_vulnerable != 0, false),
        (
            "敌 5",
            mark.opponent_aerial_marked != 0,
            mark.opponent_aerial_targeted != 0,
        ),
        ("敌 6", mark.opponent_sentry_vulnerable != 0, false),
    ];
    let blue: [(&str, bool, bool); 6] = [
        ("我 1", false, mark.ally_hero_marked != 0),
        ("我 2", false, mark.ally_engineer_marked != 0),
        ("我 3", false, mark.ally_infantry_3_marked != 0),
        ("我 4", false, mark.ally_infantry_4_marked != 0),
        (
            "我 5",
            mark.ally_aerial_targeted != 0,
            mark.ally_aerial_marked != 0,
        ),
        ("我 6", false, mark.ally_sentry_marked != 0),
    ];

    let cell_w = ((ui.available_width() - 5.0 * 6.0) / 6.0).max(36.0);
    let cell_h = cell_w.min(ui.available_height() * 0.38).max(40.0);

    ui.horizontal(|ui| {
        for (lab, vuln, marked) in red {
            mark_cell(ui, lab, vuln, marked, cell_w, cell_h);
            ui.add_space(6.0);
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        for (lab, vuln, marked) in blue {
            mark_cell(ui, lab, vuln, marked, cell_w, cell_h);
            ui.add_space(6.0);
        }
    });
    ui.add_space(8.0);
    ui.label(
        RichText::new("图例  蓝=已标记 · 红=易伤")
            .color(theme::text_muted())
            .size(11.0),
    );
}

fn mark_cell(ui: &mut Ui, label: &str, vuln: bool, marked: bool, w: f32, h: f32) {
    let (fill, stroke, id_color, status, status_color) = if vuln {
        (
            egui::Color32::from_rgba_unmultiplied(239, 68, 68, 28),
            egui::Color32::from_rgba_unmultiplied(239, 68, 68, 100),
            theme::RED,
            "vuln",
            theme::RED,
        )
    } else if marked {
        (
            egui::Color32::from_rgba_unmultiplied(47, 107, 255, 36),
            egui::Color32::from_rgba_unmultiplied(47, 107, 255, 100),
            theme::BLUE,
            "mark",
            theme::BLUE,
        )
    } else {
        (
            theme::card_bg_muted(),
            theme::border(),
            theme::text_muted(),
            "idle",
            theme::text_faint(),
        )
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(4))
        .show(ui, |ui| {
            ui.set_min_size(egui::vec2(w, h));
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(label).color(id_color).size(12.0));
                ui.label(RichText::new(status).color(status_color).size(10.0));
            });
        });
}

fn show_frame_log(ui: &mut Ui, log: &VecDeque<SerialFrameLogLine>) {
    let avail = ui.available_size();
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(0x0f, 0x12, 0x18))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_min_size(avail);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(false)
                .show(ui, |ui| {
                    if log.is_empty() {
                        ui.label(
                            RichText::new("READY  serial · open port to start RX/TX")
                                .color(theme::text_on_dark_muted())
                                .size(11.0),
                        );
                    } else {
                        for line in log.iter().rev().take(80) {
                            let color = match line.kind {
                                SerialLogKind::Ok => theme::GREEN,
                                SerialLogKind::Err => theme::RED,
                                SerialLogKind::Rx => egui::Color32::from_rgb(0x94, 0xe2, 0xd5),
                                SerialLogKind::Tx => egui::Color32::from_rgb(0xf9, 0xe2, 0xaf),
                                SerialLogKind::Info => theme::text_on_dark_muted(),
                            };
                            ui.label(RichText::new(&line.text).color(color).size(11.0));
                        }
                    }
                });
        });
}

fn show_minimap_table(ui: &mut Ui, mini: &MinimapReceiveRadarData) {
    let rows: [(&str, u16, u16); 12] = [
        ("敌英雄", mini.opponent_hero_x, mini.opponent_hero_y),
        ("敌工程", mini.opponent_engineer_x, mini.opponent_engineer_y),
        (
            "敌步3",
            mini.opponent_infantry_3_x,
            mini.opponent_infantry_3_y,
        ),
        (
            "敌步4",
            mini.opponent_infantry_4_x,
            mini.opponent_infantry_4_y,
        ),
        ("敌空中", mini.opponent_aerial_x, mini.opponent_aerial_y),
        ("敌哨兵", mini.opponent_sentry_x, mini.opponent_sentry_y),
        ("我英雄", mini.ally_hero_x, mini.ally_hero_y),
        ("我工程", mini.ally_engineer_x, mini.ally_engineer_y),
        ("我步3", mini.ally_infantry_3_x, mini.ally_infantry_3_y),
        ("我步4", mini.ally_infantry_4_x, mini.ally_infantry_4_y),
        ("我空中", mini.ally_aerial_x, mini.ally_aerial_y),
        ("我哨兵", mini.ally_sentry_x, mini.ally_sentry_y),
    ];
    let all_zero = rows.iter().all(|(_, x, y)| *x == 0 && *y == 0);
    if all_zero {
        ui.label(
            RichText::new("无数据")
                .color(theme::text_faint())
                .size(12.0),
        );
        return;
    }
    egui::Grid::new("serial_minimap_xy")
        .num_columns(3)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            ui.label(RichText::new("机").color(theme::text_faint()).size(11.0));
            ui.label(RichText::new("X").color(theme::text_faint()).size(11.0));
            ui.label(RichText::new("Y").color(theme::text_faint()).size(11.0));
            ui.end_row();
            for (name, x, y) in rows {
                ui.label(RichText::new(name).color(theme::text_muted()).size(12.0));
                ui.label(RichText::new(x.to_string()).color(theme::text()).size(12.0));
                ui.label(RichText::new(y.to_string()).color(theme::text()).size(12.0));
                ui.end_row();
            }
        });
}

fn event_chip(ui: &mut Ui, label: &str, on: bool) {
    let fill = if on {
        theme::success_bg()
    } else {
        theme::card_bg_muted()
    };
    let stroke = if on { theme::GREEN } else { theme::border() };
    let text = if on {
        theme::GREEN
    } else {
        theme::text_faint()
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(egui::CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.label(RichText::new(format!("● {label}")).color(text).size(11.0));
        });
}

fn progress_bar(ui: &mut Ui, ratio: f32, fill: egui::Color32) {
    let height = 10.0;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(255), theme::card_bg_muted());
    let fill_w = rect.width() * ratio.clamp(0.0, 1.0);
    if fill_w > 0.0 {
        let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height()));
        ui.painter()
            .rect_filled(fill_rect, egui::CornerRadius::same(255), fill);
    }
}

fn meta_row(ui: &mut Ui, k: &str, v: &str) {
    ui.label(RichText::new(k).color(theme::text_faint()).size(12.0));
    ui.label(RichText::new(v).color(theme::text()).size(12.0));
    ui.end_row();
}

fn phase_label(progress: u8) -> &'static str {
    match progress {
        0 => "未开始",
        1 => "准备阶段",
        2 => "十五秒倒计时",
        3 => "比赛中",
        4 => "结算中",
        5 => "比赛结束",
        _ => "未知阶段",
    }
}

fn winner_label(winner: u8) -> String {
    match winner {
        0 => "—".into(),
        1 => "红方".into(),
        2 => "蓝方".into(),
        3 => "平局".into(),
        n => format!("{n}"),
    }
}

fn bits_preview(bits: &[u8; 15]) -> String {
    let s: String = bits
        .iter()
        .take(8)
        .map(|b| if *b != 0 { '1' } else { '0' })
        .collect();
    format!("{s}…")
}

fn last_cmd_hint(bits: &[u8; 15]) -> String {
    const NAMES: [&str; 9] = [
        "0x0001", "0x0002", "0x0101", "0x0105", "0x020C", "0x020E", "0x0301", "0x0121", "0x0305",
    ];
    for (i, name) in NAMES.iter().enumerate() {
        if bits.get(i).copied().unwrap_or(0) != 0 {
            return (*name).into();
        }
    }
    "—".into()
}
