mod app;
mod laser;
mod pointcloud;
mod rerun_visualizer;
mod robot_interaction_id;
mod runtime;
mod serial;
mod services;
mod shared_data;
mod state;
mod theme;
mod ui_layout;
mod widgets;
mod zmq;

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_title("Radar HUD"),
        ..Default::default()
    };

    eframe::run_native(
        "Radar HUD",
        options,
        Box::new(|_cc| Ok(Box::new(app::RadarApp::default()))),
    )
}
