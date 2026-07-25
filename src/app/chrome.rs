use crate::theme;

pub(super) fn white_card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::card_bg())
        .stroke(egui::Stroke::new(1.0, theme::border()))
        .corner_radius(egui::CornerRadius::same(18))
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: theme::shadow(),
        })
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(title).color(theme::text()).size(16.0));
            });
            ui.add_space(10.0);
            add_contents(ui);
        });
}

pub(super) fn status_chip(ui: &mut egui::Ui, ok: bool, label: &str) {
    let fill = if ok {
        theme::success_bg()
    } else {
        theme::error_bg()
    };
    let text = if ok { theme::GREEN } else { theme::RED };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("● {}", label))
                    .color(text)
                    .size(12.0),
            );
        });
}
