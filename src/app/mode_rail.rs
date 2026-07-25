use super::{ActiveTab, RadarApp};
use crate::theme;

impl RadarApp {
    pub(super) fn show_mode_rail(&mut self, ui: &mut egui::Ui) {
        ui.set_min_height(ui.available_height());
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            if let Some(texture) = self.logo_texture.as_ref() {
                ui.add(
                    egui::Image::from_texture(texture)
                        .fit_to_exact_size(egui::vec2(34.0, 34.0))
                        .corner_radius(egui::CornerRadius::same(255)),
                );
            } else {
                let (logo_rect, _) =
                    ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(logo_rect.center(), 17.0, theme::BLUE_SOFT);
                ui.painter().text(
                    logo_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "R",
                    egui::FontId::proportional(16.0),
                    theme::BLUE,
                );
            }

            ui.add_space(8.0);
            self.show_mode_button(ui, "◈", ActiveTab::Laser, "Laser");
            ui.add_space(8.0);
            self.show_mode_button(ui, "◎", ActiveTab::Sdr, "SDR");
            ui.add_space(8.0);
            self.show_mode_button(ui, "◉", ActiveTab::Radar, "Radar");
            ui.add_space(8.0);
            self.show_mode_button(ui, "⇄", ActiveTab::Serial, "Serial");

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!("{} pkt", self.data_count))
                    .color(theme::text_muted())
                    .size(12.0),
            );
            ui.label(
                egui::RichText::new(format!("{}s", self.start_time.elapsed().as_secs()))
                    .color(theme::text_faint())
                    .size(12.0),
            );
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                self.show_theme_toggle(ui);
            });
        });
    }

    fn show_mode_button(&mut self, ui: &mut egui::Ui, title: &str, tab: ActiveTab, subtitle: &str) {
        let selected = self.active_tab == tab;
        let fill = if selected {
            theme::BLUE
        } else {
            theme::card_bg()
        };
        let stroke = if selected {
            egui::Stroke::NONE
        } else {
            egui::Stroke::new(1.0, theme::border())
        };
        let text_color = if selected {
            theme::text_on_dark()
        } else {
            theme::text()
        };
        let sub_color = if selected {
            theme::BLUE_SOFT
        } else {
            theme::text_faint()
        };

        let response = egui::Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(10, 12))
            .show(ui, |ui| {
                ui.set_min_width(42.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(title).color(text_color).size(18.0));
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(subtitle).color(sub_color).size(9.0));
                });
            })
            .response
            .interact(egui::Sense::click());

        if response.clicked() {
            self.active_tab = tab;
        }
    }
}
