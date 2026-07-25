use super::RadarApp;
use crate::theme;

pub(super) const RAIL_WIDTH: f32 = 68.0;
pub(super) const SIDE_LASER: f32 = 360.0;
pub(super) const SIDE_SDR: f32 = 360.0;
pub(super) const SIDE_RADAR: f32 = 340.0;
pub(super) const SIDE_SERIAL: f32 = 360.0;
pub(super) const MAIN_PAD: f32 = 12.0;
pub(super) const TOPBAR_GAP: f32 = 10.0;
pub(super) const STAGE_GAP: f32 = 10.0;

pub(super) fn sdr_dock_height(main_h: f32) -> f32 {
    (main_h * 0.20).clamp(156.0, 200.0)
}

pub(super) fn radar_strip_height(main_h: f32) -> f32 {
    (main_h * 0.12).clamp(88.0, 110.0)
}

impl RadarApp {
    pub(super) fn show_left_rail(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("mode_rail")
            .exact_width(RAIL_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::app_bg())
                    .inner_margin(egui::Margin {
                        left: 4,
                        right: 4,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ctx, |ui| {
                self.show_mode_rail(ui);
            });
    }

    pub(super) fn show_right_inspector(
        &mut self,
        ctx: &egui::Context,
        id: &'static str,
        width: f32,
        add_contents: impl FnOnce(&mut Self, &mut egui::Ui),
    ) {
        egui::SidePanel::right(id)
            .exact_width(width)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel_bg())
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ctx, |ui| {
                add_contents(self, ui);
            });
    }

    pub(super) fn show_main_column(
        &mut self,
        ctx: &egui::Context,
        topbar: impl FnOnce(&mut Self, &mut egui::Ui),
        body: impl FnOnce(&mut Self, &mut egui::Ui),
    ) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::app_bg())
                    .inner_margin(egui::Margin {
                        left: MAIN_PAD as i8,
                        right: MAIN_PAD as i8,
                        top: MAIN_PAD as i8,
                        bottom: MAIN_PAD as i8,
                    }),
            )
            .show(ctx, |ui| {
                topbar(self, ui);
                ui.add_space(TOPBAR_GAP);
                let body_rect = ui.available_rect_before_wrap();
                let body_size = body_rect.size().max(egui::vec2(1.0, 1.0));
                ui.allocate_ui_at_rect(body_rect, |ui| {
                    ui.set_min_size(body_size);
                    ui.set_max_size(body_size);
                    body(self, ui);
                });
            });
    }
}
