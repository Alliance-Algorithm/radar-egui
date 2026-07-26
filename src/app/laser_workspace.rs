use super::shell::SIDE_LASER;
use super::RadarApp;
use crate::theme;

impl RadarApp {
    pub(super) fn show_laser_workspace(&mut self, ctx: &egui::Context) {
        self.ensure_video_started();
        self.laser_video_texture.refresh(ctx, &self.video_feed);

        let laser_listening = self.zmq_sub.is_started();
        let laser_snapshot = self.laser_feed.snapshot();
        let laser_snapshot_stage = laser_snapshot.clone();

        self.show_left_rail(ctx);
        self.show_right_inspector(ctx, "laser_inspector", SIDE_LASER, |app, ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    app.show_laser_inspector_data(ui, laser_snapshot.as_ref(), laser_listening);
                    ui.add_space(14.0);
                    app.show_laser_process_controls(ui);
                });
        });
        self.show_main_column(
            ctx,
            |_, ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Laser Workspace")
                            .color(theme::text())
                            .size(21.0),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("video feed / target overlay / live detections")
                            .color(theme::text_muted())
                            .size(13.0),
                    );
                });
            },
            |app, ui| {
                let texture = app.laser_video_texture.texture().cloned();
                let live_obs = laser_snapshot_stage.as_ref().map(|s| &s.observation);
                app.show_laser_stage(ui, live_obs, texture.as_ref());
            },
        );
    }
}
