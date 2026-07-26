use super::chrome::{status_chip, white_card};
use super::shell::{sdr_dock_height, SIDE_SDR, STAGE_GAP};
use super::{ConnectionStatus, RadarApp, MINIMAP_DEFAULT_PAN_Y};
use crate::theme;
use crate::widgets::{MinimapOptions, MinimapWidget, StatusPanels, build_robot_markers};
use crate::shared_data::SharedData;

impl RadarApp {
    pub(super) fn show_sdr_workspace(
        &mut self,
        ctx: &egui::Context,
        live_snapshot: &SharedData,
    ) {
        self.show_left_rail(ctx);
        self.show_right_inspector(ctx, "sdr_inspector", SIDE_SDR, |app, ui| {
            app.show_sdr_sidebar(ui, live_snapshot);
        });
        self.show_main_column(
            ctx,
            |app, ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("SDR Workspace")
                            .color(theme::text())
                            .size(21.0),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("white battle board / live robot overlay")
                            .color(theme::text_muted())
                            .size(13.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Reset View").clicked() {
                            app.minimap_pan = egui::vec2(0.0, MINIMAP_DEFAULT_PAN_Y);
                            app.minimap_zoom = 1.0;
                        }
                    });
                });
            },
            |app, ui| {
                let body = ui.available_rect_before_wrap();
                let full_h = body.height();
                let dock_h = sdr_dock_height(full_h);
                let stage_h = (full_h - dock_h - STAGE_GAP).max(120.0);
                let stage_rect =
                    egui::Rect::from_min_size(body.min, egui::vec2(body.width(), stage_h));
                let dock_rect = egui::Rect::from_min_size(
                    egui::pos2(body.min.x, body.min.y + stage_h + STAGE_GAP),
                    egui::vec2(body.width(), dock_h),
                );

                ui.allocate_ui_at_rect(stage_rect, |ui| {
                    ui.set_min_size(stage_rect.size());
                    ui.set_max_size(stage_rect.size());

                    let map_rect = ui.available_rect_before_wrap();
                    MinimapWidget::new().show_with_state(
                        ui,
                        Some(live_snapshot),
                        app.minimap_texture.as_ref(),
                        &mut app.minimap_pan,
                        &mut app.minimap_zoom,
                        MinimapOptions {
                            show_grid: app.sdr_show_grid,
                            show_labels: app.sdr_show_labels,
                            show_heat: app.sdr_show_heat,
                            selected: app.sdr_selected,
                        },
                        &mut app.sdr_selected,
                    );

                    // Overlay map tools (top-left) + live badge (top-right).
                    let tools_pos = map_rect.min + egui::vec2(14.0, 14.0);
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                            tools_pos,
                            egui::vec2(280.0, 36.0),
                        )),
                        |ui| {
                            ui.horizontal(|ui| {
                                map_tool_chip(ui, "Grid", &mut app.sdr_show_grid);
                                map_tool_chip(ui, "Labels", &mut app.sdr_show_labels);
                                map_tool_chip(ui, "Heat ring", &mut app.sdr_show_heat);
                            });
                        },
                    );

                    let live_ok = true
                        && (app.sdr_demo || app.connection_status == ConnectionStatus::Connected);
                    let badge_w = 150.0;
                    let badge_pos =
                        egui::pos2(map_rect.right() - badge_w - 14.0, map_rect.top() + 14.0);
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                            badge_pos,
                            egui::vec2(badge_w, 30.0),
                        )),
                        |ui| {
                            egui::Frame::new()
                                .fill(theme::card_bg().gamma_multiply(0.92))
                                .stroke(egui::Stroke::new(1.0, theme::border()))
                                .corner_radius(egui::CornerRadius::same(255))
                                .inner_margin(egui::Margin::symmetric(10, 5))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(if live_ok {
                                            "● Signal feed · live"
                                        } else {
                                            "● Signal feed · idle"
                                        })
                                        .color(if live_ok { theme::GREEN } else { theme::RED })
                                        .size(11.0),
                                    );
                                });
                        },
                    );
                });

                ui.allocate_ui_at_rect(dock_rect, |ui| {
                    ui.set_min_size(dock_rect.size());
                    ui.set_max_size(dock_rect.size());
                    app.show_sdr_bottom_dock(ui, live_snapshot);
                });
            },
        );
    }

    pub(super) fn show_sdr_sidebar(
        &mut self,
        ui: &mut egui::Ui,
        radar_snapshot: &SharedData,
    ) {
        white_card(ui, "连接", |ui| {
            status_chip(
                ui,
                self.connection_status == ConnectionStatus::Connected || self.sdr_demo,
                "Signal feed",
            );
            ui.add_space(12.0);
            egui::Grid::new("radar_conn_grid")
                .num_columns(2)
                .min_col_width(78.0)
                .spacing([12.0, 10.0])
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("IP")
                            .color(theme::text_muted())
                            .size(13.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.zmq_addr).desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                });
            ui.add_space(12.0);
            if ui
                .add_sized(
                    [ui.available_width(), 32.0],
                    egui::Button::new("Reconnect radar stream").fill(theme::BLUE),
                )
                .clicked()
            {
                self.reconnect();
            }
            ui.add_space(8.0);
            egui::Grid::new("radar_meta_grid")
                .num_columns(2)
                .min_col_width(78.0)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Packets")
                            .color(theme::text_faint())
                            .size(12.0),
                    );
                    ui.label(
                        egui::RichText::new(self.data_count.to_string())
                            .color(theme::text())
                            .size(12.0),
                    );
                    ui.end_row();
                    ui.label(
                        egui::RichText::new("Last live")
                            .color(theme::text_faint())
                            .size(12.0),
                    );
                    let age = self
                        .last_update
                        .map(|last| format!("{:.1}s", last.elapsed().as_secs_f32()))
                        .unwrap_or_else(|| "--".to_string());
                    ui.label(egui::RichText::new(age).color(theme::text()).size(12.0));
                    ui.end_row();
                });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("模拟数据流 (demo)")
                        .color(theme::text_muted())
                        .size(12.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut self.sdr_demo, "");
                });
            });

            if let Some(err) = &self.error_message {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(err).color(theme::RED).size(12.0));
            }
        });

        ui.add_space(14.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                StatusPanels::new().show(ui, Some(radar_snapshot));
            });
    }

    pub(super) fn show_sdr_bottom_dock(
        &mut self,
        ui: &mut egui::Ui,
        radar_snapshot: &SharedData,
    ) {
        let info = radar_snapshot;

        let robots = build_robot_markers(info);
        let sel = self.sdr_selected.min(robots.len().saturating_sub(1));
        let selected = &robots[sel];

        ui.columns(3, |cols| {
            white_card(&mut cols[0], "选中单位", |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("点击地图 marker 或列表切换")
                            .color(theme::text_muted())
                            .size(11.0),
                    );
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let (sw, _) =
                        ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
                    ui.painter().rect_filled(sw, 12.0, selected.color);
                    ui.painter().rect_stroke(
                        sw,
                        12.0,
                        egui::Stroke::new(2.0, egui::Color32::WHITE),
                        egui::StrokeKind::Outside,
                    );
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(selected.name)
                                .color(theme::text())
                                .size(16.0),
                        );
                        ui.label(
                            egui::RichText::new("主战 · 实时状态")
                                .color(theme::text_muted())
                                .size(12.0),
                        );
                    });
                });
                ui.add_space(10.0);
                if let Some(health) = selected.health {
                    ui.label(
                        egui::RichText::new("血量")
                            .color(theme::text_faint())
                            .size(11.0),
                    );
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{} / {}", health.hp, health.hp_max))
                                .color(theme::text())
                                .size(18.0),
                        );
                    });
                    hp_bar(ui, health.hp as f32 / health.hp_max as f32, selected.color);
                } else {
                    ui.label(
                        egui::RichText::new("血量 N/A")
                            .color(theme::text_muted())
                            .size(11.0),
                    );
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("弹药 {}", selected.ammo))
                            .color(theme::text_muted())
                            .size(13.0),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "x {} · y {}",
                            selected.pos[0], selected.pos[1]
                        ))
                        .color(theme::text_faint())
                        .size(12.0),
                    );
                });
            });

            white_card(&mut cols[1], "机器人列表", |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("血条动画 · 点击选中")
                            .color(theme::text_muted())
                            .size(11.0),
                    );
                });
                ui.add_space(6.0);
                for (i, robot) in robots.iter().enumerate() {
                    let is_sel = i == sel;
                    let fill = if is_sel {
                        egui::Color32::from_rgba_unmultiplied(47, 107, 255, 40)
                    } else {
                        theme::card_bg_muted()
                    };
                    let stroke = if is_sel {
                        egui::Stroke::new(1.0, theme::BLUE.gamma_multiply(0.45))
                    } else {
                        egui::Stroke::NONE
                    };
                    let resp = egui::Frame::new()
                        .fill(fill)
                        .stroke(stroke)
                        .corner_radius(egui::CornerRadius::same(14))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.painter().circle_filled(
                                    ui.cursor().left_center() + egui::vec2(6.0, 0.0),
                                    5.0,
                                    robot.color,
                                );
                                ui.add_space(14.0);
                                ui.label(
                                    egui::RichText::new(robot.name)
                                        .color(theme::text())
                                        .size(13.0),
                                );
                                if let Some(health) = robot.health {
                                    let bar_w = ui.available_width() - 48.0;
                                    if bar_w > 20.0 {
                                        let (bar_rect, _) = ui.allocate_exact_size(
                                            egui::vec2(bar_w.min(80.0), 6.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            bar_rect,
                                            255.0,
                                            theme::border().gamma_multiply(0.35),
                                        );
                                        let r = health.hp as f32 / health.hp_max as f32;
                                        if r > 0.0 {
                                            ui.painter().rect_filled(
                                                egui::Rect::from_min_size(
                                                    bar_rect.min,
                                                    egui::vec2(
                                                        bar_rect.width() * r,
                                                        bar_rect.height(),
                                                    ),
                                                ),
                                                255.0,
                                                robot.color,
                                            );
                                        }
                                    }
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(if robot.ammo > 0 {
                                                robot.ammo.to_string()
                                            } else {
                                                "—".into()
                                            })
                                            .color(theme::text_faint())
                                            .size(12.0),
                                        );
                                    },
                                );
                            });
                        })
                        .response
                        .interact(egui::Sense::click());
                    if resp.clicked() {
                        self.sdr_selected = i;
                    }
                    ui.add_space(4.0);
                }
            });

            white_card(&mut cols[2], "经济 / 占领", |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("当前资源 / 已获得资源")
                            .color(theme::text_muted())
                            .size(11.0),
                    );
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(info.sdr_state.remaining_gold.to_string())
                            .color(theme::text())
                            .size(28.0),
                    );
                    ui.label(
                        egui::RichText::new(format!("/ {}", info.sdr_state.total_gold))
                            .color(theme::text_muted())
                            .size(16.0),
                    );
                });
                let ratio = if info.sdr_state.total_gold > 0 {
                    info.sdr_state.remaining_gold as f32 / info.sdr_state.total_gold as f32
                } else {
                    0.0
                };
                hp_bar(ui, ratio, theme::BLUE);
                ui.add_space(10.0);
                ui.label("（无有效数据）");
            });
        });
    }
}

fn map_tool_chip(ui: &mut egui::Ui, label: &str, on: &mut bool) {
    let fill = if *on {
        theme::BLUE
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

fn hp_bar(ui: &mut egui::Ui, ratio: f32, fill: egui::Color32) {
    let height = 12.0;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(255),
        egui::Color32::from_rgba_unmultiplied(47, 107, 255, 24),
    );
    let w = rect.width() * ratio.clamp(0.0, 1.0);
    if w > 0.0 {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(w, rect.height())),
            egui::CornerRadius::same(255),
            fill,
        );
    }
}
