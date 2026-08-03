use super::chrome::{status_chip, white_card};
use super::shell::{radar_strip_height, SIDE_RADAR, STAGE_GAP};
use super::RadarApp;
use crate::pointcloud::pcd_viewer::PcdViewerStatus;
use crate::theme;
use crate::ui_layout::{inset_rect, STAGE_PAD};

struct PcdStatusDetails {
    phase: &'static str,
    path: String,
    detail: Option<String>,
    failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PcdStatusStyle {
    Neutral,
    Progress,
    Success,
    Error,
}

fn pcd_status_style(status: &PcdViewerStatus) -> PcdStatusStyle {
    match status {
        PcdViewerStatus::Idle => PcdStatusStyle::Neutral,
        PcdViewerStatus::Loading { .. } | PcdViewerStatus::Launching { .. } => {
            PcdStatusStyle::Progress
        }
        PcdViewerStatus::Ready { .. } => PcdStatusStyle::Success,
        PcdViewerStatus::Failed { .. } => PcdStatusStyle::Error,
    }
}

fn pcd_status_chip(ui: &mut egui::Ui, status: &PcdViewerStatus, label: &str) {
    let (fill, text) = match pcd_status_style(status) {
        PcdStatusStyle::Neutral => (theme::card_bg_muted(), theme::text_muted()),
        PcdStatusStyle::Progress => (theme::card_bg_muted(), theme::BLUE),
        PcdStatusStyle::Success => (theme::success_bg(), theme::GREEN),
        PcdStatusStyle::Error => (theme::error_bg(), theme::RED),
    };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("● {label}"))
                    .color(text)
                    .size(12.0),
            );
        });
}

fn pcd_action_enabled(busy: bool, feature_enabled: bool) -> bool {
    feature_enabled && !busy
}

fn format_load_result(result: &crate::pointcloud::pcd_viewer::PcdLoadResult) -> String {
    let stats = result.stats;
    format!(
        "{} · {} valid / {} declared · {} skipped · {:.3} s",
        stats.encoding,
        stats.valid_points,
        stats.declared_points,
        stats.skipped_points,
        result.elapsed.as_secs_f64()
    )
}

fn pcd_status_details(status: &PcdViewerStatus) -> PcdStatusDetails {
    match status {
        PcdViewerStatus::Idle => PcdStatusDetails {
            phase: "Idle",
            path: "Select a .pcd file to open in Rerun".to_owned(),
            detail: None,
            failed: false,
        },
        PcdViewerStatus::Loading {
            path,
            loaded_points,
            total_points,
        } => PcdStatusDetails {
            phase: "Loading",
            path: path.display().to_string(),
            detail: Some(format!("{loaded_points} / {total_points} points")),
            failed: false,
        },
        PcdViewerStatus::Launching { path, result } => PcdStatusDetails {
            phase: "Launching",
            path: path.display().to_string(),
            detail: Some(format_load_result(result)),
            failed: false,
        },
        PcdViewerStatus::Ready { path, result } => PcdStatusDetails {
            phase: "Ready",
            path: path.display().to_string(),
            detail: Some(format_load_result(result)),
            failed: false,
        },
        PcdViewerStatus::Failed {
            path,
            message,
            loaded,
        } => PcdStatusDetails {
            phase: "Failed",
            path: path.display().to_string(),
            detail: Some(match loaded {
                Some(result) => format!("{message}\n{}", format_load_result(result)),
                None => message.clone(),
            }),
            failed: true,
        },
    }
}

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
        white_card(ui, "Radar 启动详情", |ui| {
            self.refresh_radar_nodes();
            if let Some((_, nodes)) = &self.radar_node_check {
                egui::Grid::new("radar_node_check")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        for (name, ok) in nodes {
                            ui.label(
                                egui::RichText::new(if *ok { "●" } else { "○" })
                                    .color(if *ok { theme::GREEN } else { theme::RED })
                                    .size(13.0),
                            );
                            ui.label(egui::RichText::new(name).color(theme::text()).size(12.0));
                            ui.end_row();
                        }
                    });
            } else {
                ui.label(
                    egui::RichText::new("（未检查，等待容器）")
                        .color(theme::text_faint())
                        .size(12.0),
                );
            }
            ui.add_space(8.0);
            self.refresh_radar_log();
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .auto_shrink([false, false])
                .show(ui, |ui| match &self.radar_log_tail {
                    Some((_, tail)) if !tail.is_empty() => {
                        ui.label(
                            egui::RichText::new(tail)
                                .monospace()
                                .color(theme::text_muted())
                                .size(11.0),
                        );
                    }
                    _ => {
                        ui.label(
                            egui::RichText::new("（launch 日志为空）")
                                .color(theme::text_faint())
                                .size(12.0),
                        );
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
        white_card(ui, "Offline PCD", |ui| {
            let status = self.pcd_viewer.status();
            let details = pcd_status_details(status);
            pcd_status_chip(ui, status, details.phase);
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(details.path)
                    .color(if details.failed {
                        theme::RED
                    } else {
                        theme::text_muted()
                    })
                    .size(12.0),
            );
            if let Some(detail) = details.detail {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(detail)
                        .color(if details.failed {
                            theme::RED
                        } else {
                            theme::text_faint()
                        })
                        .size(11.0),
                );
            }
            ui.add_space(10.0);

            let enabled = pcd_action_enabled(self.pcd_viewer.is_busy(), cfg!(feature = "rerun"));
            if ui
                .add_enabled(enabled, egui::Button::new("Open PCD in Rerun"))
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("PCD point cloud", &["pcd"])
                    .pick_file()
                {
                    self.pcd_viewer.start(path);
                }
            }

            #[cfg(not(feature = "rerun"))]
            {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "Native PCD viewing requires the `rerun` feature. Run with --features rerun.",
                    )
                    .color(theme::text_muted())
                    .size(11.0),
                );
            }
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

    /// 每 5 秒检查一次容器内雷达节点是否齐全。
    fn refresh_radar_nodes(&mut self) {
        const INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
        if self
            .radar_node_check
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() < INTERVAL)
        {
            return;
        }
        let container = crate::services::script_runner::radar_container();
        let expected = [
            "hikcamera_ros_driver_node",
            "host_sdk_sample",
            "radar_lidar_node",
            "radar_camera_node",
            "radar_fusion_node",
            "radar_bridge_node",
            "match_recorder",
        ];
        let nodes = std::process::Command::new("docker")
            .args([
                "exec",
                container,
                "bash",
                "-lc",
                "source /opt/ros/jazzy/setup.bash && ros2 node list 2>/dev/null || true",
            ])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .unwrap_or_default();
        let status = expected
            .iter()
            .map(|name| {
                let ok = nodes.lines().any(|line| line.trim() == *name);
                ((*name).to_owned(), ok)
            })
            .collect();
        self.radar_node_check = Some((std::time::Instant::now(), status));
    }

    /// 每 2 秒读取一次 launch stderr 日志尾部。
    fn refresh_radar_log(&mut self) {
        const INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
        if self
            .radar_log_tail
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() < INTERVAL)
        {
            return;
        }
        let tail = std::fs::read_to_string("/tmp/radar-egui-radar.stderr.log")
            .map(|content| {
                let keep = content.chars().rev().take(8000).collect::<String>();
                keep.chars().rev().collect()
            })
            .unwrap_or_default();
        self.radar_log_tail = Some((std::time::Instant::now(), tail));
    }
}

#[cfg(test)]
mod tests {
    use super::{pcd_action_enabled, pcd_status_details, pcd_status_style, PcdStatusStyle};
    use crate::pointcloud::pcd_loader::PcdEncoding;
    use crate::pointcloud::pcd_viewer::{PcdLoadResult, PcdLoadStats, PcdViewerStatus};
    use std::path::PathBuf;
    use std::time::Duration;

    fn load_result() -> PcdLoadResult {
        PcdLoadResult {
            stats: PcdLoadStats {
                encoding: PcdEncoding::Ascii,
                valid_points: 98,
                skipped_points: 2,
                declared_points: 100,
            },
            elapsed: Duration::from_millis(1250),
        }
    }

    #[test]
    fn pcd_action_is_disabled_while_the_runtime_is_busy() {
        assert!(!pcd_action_enabled(true, true));
        assert!(!pcd_action_enabled(true, false));
        assert!(!pcd_action_enabled(false, false));
        assert!(pcd_action_enabled(false, true));
    }

    #[test]
    fn pcd_status_style_classifies_every_runtime_phase() {
        let cases = [
            (PcdViewerStatus::Idle, PcdStatusStyle::Neutral),
            (
                PcdViewerStatus::Loading {
                    path: PathBuf::from("scan.pcd"),
                    loaded_points: 25,
                    total_points: 100,
                },
                PcdStatusStyle::Progress,
            ),
            (
                PcdViewerStatus::Launching {
                    path: PathBuf::from("scan.pcd"),
                    result: load_result(),
                },
                PcdStatusStyle::Progress,
            ),
            (
                PcdViewerStatus::Ready {
                    path: PathBuf::from("scan.pcd"),
                    result: load_result(),
                },
                PcdStatusStyle::Success,
            ),
            (
                PcdViewerStatus::Failed {
                    path: PathBuf::from("broken.pcd"),
                    message: "invalid header".to_owned(),
                    loaded: None,
                },
                PcdStatusStyle::Error,
            ),
        ];

        for (status, expected) in cases {
            assert_eq!(pcd_status_style(&status), expected);
        }
    }

    #[test]
    fn pcd_status_details_cover_every_runtime_phase() {
        let cases = [
            (
                PcdViewerStatus::Idle,
                ("Idle", "Select a .pcd file to open in Rerun", None, false),
            ),
            (
                PcdViewerStatus::Loading {
                    path: PathBuf::from("scan.pcd"),
                    loaded_points: 25,
                    total_points: 100,
                },
                (
                    "Loading",
                    "scan.pcd",
                    Some("25 / 100 points".to_owned()),
                    false,
                ),
            ),
            (
                PcdViewerStatus::Launching {
                    path: PathBuf::from("scan.pcd"),
                    result: load_result(),
                },
                (
                    "Launching",
                    "scan.pcd",
                    Some("ASCII · 98 valid / 100 declared · 2 skipped · 1.250 s".to_owned()),
                    false,
                ),
            ),
            (
                PcdViewerStatus::Ready {
                    path: PathBuf::from("scan.pcd"),
                    result: load_result(),
                },
                (
                    "Ready",
                    "scan.pcd",
                    Some("ASCII · 98 valid / 100 declared · 2 skipped · 1.250 s".to_owned()),
                    false,
                ),
            ),
            (
                PcdViewerStatus::Failed {
                    path: PathBuf::from("broken.pcd"),
                    message: "invalid header".to_owned(),
                    loaded: None,
                },
                (
                    "Failed",
                    "broken.pcd",
                    Some("invalid header".to_owned()),
                    true,
                ),
            ),
            (
                PcdViewerStatus::Failed {
                    path: PathBuf::from("scan.pcd"),
                    message: "viewer unavailable".to_owned(),
                    loaded: Some(load_result()),
                },
                (
                    "Failed",
                    "scan.pcd",
                    Some(
                        "viewer unavailable\nASCII · 98 valid / 100 declared · 2 skipped · 1.250 s"
                            .to_owned(),
                    ),
                    true,
                ),
            ),
        ];

        for (status, expected) in cases {
            let actual = pcd_status_details(&status);
            assert_eq!(
                (
                    actual.phase,
                    actual.path.as_str(),
                    actual.detail,
                    actual.failed
                ),
                expected
            );
        }
    }

    use super::rerun_status_label;

    #[test]
    fn rerun_status_does_not_claim_connection() {
        assert_eq!(rerun_status_label(), "optional · not monitored");
    }
}
