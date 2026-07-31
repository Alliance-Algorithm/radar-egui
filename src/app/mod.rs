use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Once;

use self::video_texture::VideoTextureCache;
use crate::laser::video::VideoFrameReader;
use crate::pointcloud::pcd_viewer::PcdViewerRuntime;
use crate::pointcloud::rerun_visualizer::PointCloudVisualizer;
use crate::rerun_visualizer::RerunVisualizer;
use crate::runtime::{PointCloudRuntime, VideoRuntime, ZmqPubRuntime, ZmqSubRuntime};
use crate::services::process_control::ProcessControl;
use crate::services::{ProcessSendError, StartAllOptions, TeamSide};
use crate::state::{LaserObservationReader, PointCloudFrameReader, SharedReader};
use crate::theme;
use crate::widgets::SerialLogKind;

mod assets;
mod chrome;
mod connection;
mod laser_inspector;
mod laser_process_controls;
mod laser_stage;
mod laser_workspace;
mod mode_rail;
mod radar_workspace;
mod sdr_workspace;
mod serial_workspace;
mod shell;
mod theme_apply;
mod video_texture;

static FONT_ONCE: Once = Once::new();
const MINIMAP_BG_PATH: &str = "assets/minimap_bg.png";
const LOGO_PATH: &str = "assets/logo.png";
pub(super) const MINIMAP_DEFAULT_PAN_Y: f32 = 18.0;

#[derive(PartialEq, Clone, Copy)]
enum ActiveTab {
    Sdr,
    Laser,
    Radar,
    Serial,
}

pub struct RadarApp {
    active_tab: ActiveTab,
    dark_mode: bool,
    minimap_texture: Option<egui::TextureHandle>,
    minimap_texture_failed: bool,
    minimap_pan: egui::Vec2,
    minimap_zoom: f32,
    sdr_selected: usize,
    sdr_show_grid: bool,
    sdr_show_labels: bool,
    sdr_show_hp_ring: bool,
    sdr_demo: bool,
    logo_texture: Option<egui::TextureHandle>,
    logo_texture_failed: bool,

    shared_reader: SharedReader,
    connection_status: ConnectionStatus,
    last_update: Option<std::time::Instant>,
    zmq_sub: ZmqSubRuntime,
    zmq_pub: ZmqPubRuntime,
    zmq_addr: String,
    error_message: Option<String>,
    data_count: u64,
    last_logged_radar_version: u64,
    start_time: std::time::Instant,
    rerun_viz: RerunVisualizer,
    pointcloud_viz: PointCloudVisualizer,

    laser_feed: LaserObservationReader,
    video_feed: VideoFrameReader,
    video_runtime: VideoRuntime,
    laser_video_texture: VideoTextureCache,

    pointcloud_feed: PointCloudFrameReader,
    pointcloud_runtime: PointCloudRuntime,
    pointcloud_last_seq: u32,
    pcd_viewer: PcdViewerRuntime,

    process_control: ProcessControl,
    team_side: TeamSide,
    laser_auto: bool,
    stream_on_start: bool,
    record_on_start: bool,
    process_command_error: Option<String>,

    laser_stage_overlay: bool,
    laser_stage_demo: bool,

    serial_port_name: String,
    serial_baud: u32,
    serial_open: bool,
    serial_error: Option<String>,
    serial_frame_log: std::collections::VecDeque<crate::widgets::SerialFrameLogLine>,
    serial_last_observed: Option<serial_workspace::SerialObservedState>,
    serial_rx_handle: Option<std::thread::JoinHandle<()>>,
    serial_tx_handle: Option<std::thread::JoinHandle<()>>,
    serial_stop: Option<Arc<AtomicBool>>,
    serial_worker_health: Option<Arc<AtomicBool>>,
}

#[derive(PartialEq)]
enum ConnectionStatus {
    Disconnected,
    Connected,
}

impl Default for RadarApp {
    fn default() -> Self {
        let (shared_reader, _shared_writer) = SharedReader::new_pair();
        let shared = shared_reader.inner();
        let (laser_feed, _laser_writer) = LaserObservationReader::new_pair();

        let zmq_sub = ZmqSubRuntime::start(
            &["tcp://127.0.0.1:5555".into(), "tcp://127.0.0.1:5556".into()],
            shared.clone(),
        );

        let zmq_pub = ZmqPubRuntime::start("tcp://*:5557", shared.clone());

        if let Ok(mut guard) = shared.lock() {
            guard.radar_side = "red".to_string();
        }

        let (video_feed, video_writer) = VideoFrameReader::new_pair();
        let video_runtime = VideoRuntime::new(video_writer);

        let (pointcloud_feed, pointcloud_writer) = PointCloudFrameReader::new_pair();
        let pointcloud_runtime = PointCloudRuntime::new(pointcloud_writer);

        Self {
            active_tab: ActiveTab::Laser,
            dark_mode: false,
            minimap_texture: None,
            minimap_texture_failed: false,
            minimap_pan: egui::vec2(0.0, MINIMAP_DEFAULT_PAN_Y),
            minimap_zoom: 1.0,
            sdr_selected: 0,
            sdr_show_grid: true,
            sdr_show_labels: true,
            sdr_show_hp_ring: true,
            sdr_demo: false,
            logo_texture: None,
            logo_texture_failed: false,
            shared_reader,
            connection_status: ConnectionStatus::Disconnected,
            last_update: None,
            zmq_sub,
            zmq_pub,
            zmq_addr: "tcp://127.0.0.1:5555".to_string(),
            error_message: None,
            data_count: 0,
            last_logged_radar_version: 0,
            start_time: std::time::Instant::now(),
            rerun_viz: RerunVisualizer::new(),
            pointcloud_viz: PointCloudVisualizer::default(),
            laser_feed,
            video_feed,
            video_runtime,
            laser_video_texture: VideoTextureCache::default(),
            pointcloud_feed,
            pointcloud_runtime,
            pointcloud_last_seq: 0,
            pcd_viewer: PcdViewerRuntime::new(),
            process_control: ProcessControl::new(),
            team_side: TeamSide::Red,
            laser_auto: false,
            stream_on_start: true,
            record_on_start: false,
            process_command_error: None,
            laser_stage_overlay: true,
            laser_stage_demo: false,
            serial_port_name: "/dev/ttyUSB0".to_string(),
            serial_baud: 115_200,
            serial_open: false,
            serial_error: None,
            serial_frame_log: std::collections::VecDeque::new(),
            serial_last_observed: None,
            serial_rx_handle: None,
            serial_tx_handle: None,
            serial_stop: None,
            serial_worker_health: None,
        }
    }
}

fn start_all_options(
    side: TeamSide,
    stream: bool,
    record: bool,
    laser_auto: bool,
) -> StartAllOptions {
    StartAllOptions {
        side,
        stream,
        record,
        laser_auto,
    }
}

fn store_process_command_result(
    error_state: &mut Option<String>,
    result: Result<(), ProcessSendError>,
) {
    *error_state = result.err().map(|error| error.to_string());
}

impl RadarApp {
    fn store_process_command_result(&mut self, result: Result<(), ProcessSendError>) {
        store_process_command_result(&mut self.process_command_error, result);
    }

    fn update_shared_team_side(&self) {
        if let Ok(mut shared) = self.shared_reader.inner().lock() {
            shared.radar_side = self.team_side.as_str().to_owned();
        }
    }

    fn reconnect(&mut self) {
        self.connection_status = ConnectionStatus::Disconnected;
        self.last_update = None;
        self.error_message = None;
        self.data_count = 0;
        self.last_logged_radar_version = 0;
    }

    fn ensure_video_started(&mut self) {
        self.video_runtime.ensure_started();
    }

    fn ensure_pointcloud_started(&mut self) {
        self.pointcloud_runtime.ensure_started();
    }

    fn open_serial(&mut self) {
        use crate::serial::serial::{serial_start_receiver, serial_start_transmitter, Serial};
        use crate::serial::serialconfig::SerialConfig;

        if self.serial_open {
            return;
        }
        let config = SerialConfig {
            port_name: self.serial_port_name.clone(),
            baud_rate: self.serial_baud,
        };
        match Serial::new(config) {
            Ok(port) => match port.clone_serial_port() {
                Ok(port_tx) => {
                    let shared = self.shared_reader.inner();
                    let stop = Arc::new(AtomicBool::new(false));
                    let worker_health = Arc::new(AtomicBool::new(true));
                    let pub_tx = self
                        .zmq_pub
                        .pub_tx
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clone();
                    let (tx_tx, tx_rx) = std::sync::mpsc::channel();
                    let notify_all = match pub_tx {
                        Some(zmq_tx) => vec![zmq_tx, tx_tx],
                        None => vec![tx_tx],
                    };
                    let rx = serial_start_receiver(
                        port,
                        shared.clone(),
                        notify_all,
                        stop.clone(),
                        worker_health.clone(),
                    );
                    let tx = serial_start_transmitter(
                        port_tx,
                        shared.clone(),
                        tx_rx,
                        stop.clone(),
                        worker_health.clone(),
                    );
                    self.serial_rx_handle = Some(rx);
                    self.serial_tx_handle = Some(tx);
                    self.serial_stop = Some(stop);
                    self.serial_worker_health = Some(worker_health);
                    self.serial_open = true;
                    self.serial_error = None;
                    log::info!("Serial opened on {}", self.serial_port_name);
                }
                Err(e) => {
                    serial_open_failed(
                        &mut self.serial_open,
                        &mut self.serial_error,
                        format!("clone port: {e}"),
                    );
                    log::error!("Serial clone failed: {e}");
                }
            },
            Err(e) => {
                serial_open_failed(
                    &mut self.serial_open,
                    &mut self.serial_error,
                    format!("open: {e}"),
                );
                log::error!("Serial open failed: {e}");
            }
        }
    }

    fn close_serial(&mut self) {
        close_serial_workers(
            &mut self.serial_stop,
            &mut self.serial_rx_handle,
            &mut self.serial_tx_handle,
        );
        self.serial_worker_health = None;
        self.serial_open = false;
        self.serial_error = None;
        log::info!("Serial closed");
    }

    fn reconcile_serial_workers(&mut self) {
        let worker_failed = self
            .serial_worker_health
            .as_ref()
            .is_some_and(|health| !health.load(Ordering::Relaxed));
        let worker_finished = self
            .serial_rx_handle
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
            || self
                .serial_tx_handle
                .as_ref()
                .is_some_and(std::thread::JoinHandle::is_finished);
        if self.serial_open && (worker_failed || worker_finished) {
            self.serial_error = Some("serial worker stopped".to_owned());
            self.push_serial_log(SerialLogKind::Err, "serial worker stopped".to_owned());
            close_serial_workers(
                &mut self.serial_stop,
                &mut self.serial_rx_handle,
                &mut self.serial_tx_handle,
            );
            self.serial_worker_health = None;
            self.serial_open = false;
        }
    }

    fn update_pointcloud(&mut self) {
        let Some(rec) = self.rerun_viz.recording_stream() else {
            return;
        };
        self.pointcloud_feed.with_frame(|frame| {
            if let Some(frame) = frame {
                if frame.seq != self.pointcloud_last_seq {
                    self.pointcloud_last_seq = frame.seq;
                    self.pointcloud_viz.log_point_cloud(&rec, frame);
                }
            }
        });
    }

    fn show_pointcloud_status(&self, ui: &mut egui::Ui) {
        let has_data = self
            .pointcloud_feed
            .with_frame(|f| f.is_some())
            .unwrap_or(false);
        let (status, color) = if has_data {
            ("Receiving point cloud", theme::GREEN)
        } else {
            ("Waiting for SHM /pointcloud_frame ...", theme::text_muted())
        };
        ui.label(egui::RichText::new(status).color(color).size(13.0));
        if has_data {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("frame seq: {}", self.pointcloud_last_seq))
                    .color(theme::text_faint())
                    .size(12.0),
            );
        }
    }
}

fn close_serial_workers(
    stop: &mut Option<Arc<AtomicBool>>,
    rx_handle: &mut Option<std::thread::JoinHandle<()>>,
    tx_handle: &mut Option<std::thread::JoinHandle<()>>,
) {
    if let Some(stop) = stop.take() {
        stop.store(true, Ordering::Relaxed);
    }
    if let Some(handle) = rx_handle.take() {
        let _ = handle.join();
    }
    if let Some(handle) = tx_handle.take() {
        let _ = handle.join();
    }
}

fn serial_open_failed(serial_open: &mut bool, serial_error: &mut Option<String>, error: String) {
    *serial_open = false;
    *serial_error = Some(error);
}

impl eframe::App for RadarApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        app_clear_color()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.reconcile_serial_workers();
        self.setup_fonts(ctx);
        theme::set_dark_mode(self.dark_mode);
        self.ensure_minimap_texture(ctx);
        self.ensure_logo_texture(ctx);
        let snapshot = self.shared_reader.snapshot();
        self.update_connection_status(&snapshot);
        if self.serial_open {
            self.update_serial_state_log(&snapshot);
        }
        self.apply_theme(ctx);
        self.pcd_viewer.poll();
        if self.active_tab == ActiveTab::Radar {
            self.update_pointcloud();
        }

        match self.active_tab {
            ActiveTab::Sdr => self.show_sdr_workspace(ctx, &snapshot),
            ActiveTab::Laser => self.show_laser_workspace(ctx),
            ActiveTab::Radar => self.show_radar_workspace(ctx),
            ActiveTab::Serial => self.show_serial_workspace(ctx, &snapshot),
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.close_serial();
    }
}

/// Returns the clear color that matches [`theme::app_bg()`] for the current theme.
///
/// This is the intended value for `eframe::NativeOptions::clear_color` and
/// `eframe::App::clear_color` — it prevents a black hairline at panel boundaries
/// caused by the default clear color when fractional-DPI gaps appear.
pub(super) fn app_clear_color() -> [f32; 4] {
    theme::app_bg().to_normalized_gamma_f32()
}

#[cfg(test)]
mod tests {
    use super::*;

    static ZMQ_TEST_PORT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// `RadarApp::default()` starts ZMQ runtimes and binds `tcp://*:5557`; tests
    /// must serialize that window and release the port before dropping the app.
    fn radar_app_for_test() -> (std::sync::MutexGuard<'static, ()>, RadarApp) {
        let guard = ZMQ_TEST_PORT_LOCK.lock().unwrap();
        let mut app = RadarApp::default();
        app.zmq_pub.stop();
        app.zmq_sub.stop();
        (guard, app)
    }

    fn lifecycle_handles() -> (
        Option<Arc<AtomicBool>>,
        Option<std::thread::JoinHandle<()>>,
        Option<std::thread::JoinHandle<()>>,
    ) {
        let stop = Arc::new(AtomicBool::new(false));
        let rx = std::thread::spawn(|| {});
        let tx = std::thread::spawn(|| panic!("test worker panic"));
        (Some(stop), Some(rx), Some(tx))
    }

    fn install_serial_lifecycle_state(app: &mut RadarApp) -> Arc<AtomicBool> {
        let (stop, rx, tx) = lifecycle_handles();
        let stop_ref = stop.as_ref().unwrap().clone();
        app.serial_stop = stop;
        app.serial_rx_handle = rx;
        app.serial_tx_handle = tx;
        app.serial_open = true;
        app.serial_error = Some("stale serial error".to_owned());
        stop_ref
    }

    #[test]
    fn reconcile_serial_workers_closes_after_worker_exit() {
        let (_zmq_guard, mut app) = radar_app_for_test();
        let stop = install_serial_lifecycle_state(&mut app);
        let health = Arc::new(AtomicBool::new(false));
        app.serial_worker_health = Some(health);

        app.reconcile_serial_workers();

        assert!(!app.serial_open);
        assert_eq!(app.serial_error.as_deref(), Some("serial worker stopped"));
        assert!(app.serial_stop.is_none());
        assert!(app.serial_rx_handle.is_none());
        assert!(app.serial_tx_handle.is_none());
        assert!(stop.load(Ordering::Relaxed));
    }

    #[test]
    fn close_serial_workers_stops_and_clears_all_handles_even_after_panic() {
        let (mut stop, mut rx, mut tx) = lifecycle_handles();
        let stop_ref = stop.as_ref().unwrap().clone();

        close_serial_workers(&mut stop, &mut rx, &mut tx);

        assert!(stop.is_none());
        assert!(stop_ref.load(Ordering::Relaxed));
        assert!(rx.is_none());
        assert!(tx.is_none());
    }

    #[test]
    fn closing_serial_workers_twice_is_harmless() {
        let (mut stop, mut rx, mut tx) = lifecycle_handles();
        close_serial_workers(&mut stop, &mut rx, &mut tx);

        close_serial_workers(&mut stop, &mut rx, &mut tx);

        assert!(stop.is_none());
        assert!(rx.is_none());
        assert!(tx.is_none());
    }

    #[test]
    fn serial_open_failure_keeps_connection_closed() {
        let mut serial_open = true;
        let mut serial_error = None;

        serial_open_failed(
            &mut serial_open,
            &mut serial_error,
            "open: test failure".to_owned(),
        );

        assert!(!serial_open);
        assert_eq!(serial_error.as_deref(), Some("open: test failure"));
    }

    #[test]
    fn close_serial_clears_connection_state_and_all_workers() {
        let (_zmq_guard, mut app) = radar_app_for_test();
        let stop = install_serial_lifecycle_state(&mut app);

        app.close_serial();

        assert!(!app.serial_open);
        assert!(app.serial_error.is_none());
        assert!(app.serial_stop.is_none());
        assert!(app.serial_rx_handle.is_none());
        assert!(app.serial_tx_handle.is_none());
        assert!(stop.load(Ordering::Relaxed));
    }

    #[test]
    fn repeated_close_serial_is_harmless() {
        let (_zmq_guard, mut app) = radar_app_for_test();
        install_serial_lifecycle_state(&mut app);

        app.close_serial();
        app.close_serial();

        assert!(!app.serial_open);
        assert!(app.serial_error.is_none());
        assert!(app.serial_stop.is_none());
        assert!(app.serial_rx_handle.is_none());
        assert!(app.serial_tx_handle.is_none());
    }

    #[test]
    fn open_serial_failure_after_close_keeps_connection_closed() {
        let (_zmq_guard, mut app) = radar_app_for_test();
        install_serial_lifecycle_state(&mut app);
        app.close_serial();

        app.serial_port_name = "/definitely-not-a-serial-device".to_owned();
        app.open_serial();
        assert!(!app.serial_open);
        assert!(app.serial_error.is_some());
        assert!(app.serial_stop.is_none());
        assert!(app.serial_rx_handle.is_none());
        assert!(app.serial_tx_handle.is_none());
    }

    #[test]
    fn on_exit_clears_connection_state_and_workers_repeatedly() {
        let (_zmq_guard, mut app) = radar_app_for_test();
        let stop = install_serial_lifecycle_state(&mut app);
        eframe::App::on_exit(&mut app, None);
        eframe::App::on_exit(&mut app, None);

        assert!(!app.serial_open);
        assert!(app.serial_error.is_none());
        assert!(app.serial_stop.is_none());
        assert!(app.serial_rx_handle.is_none());
        assert!(app.serial_tx_handle.is_none());
        assert!(stop.load(Ordering::Relaxed));
    }

    #[test]
    fn start_all_options_preserve_laser_flags() {
        let options = start_all_options(TeamSide::Blue, true, false, true);
        assert_eq!(options.side, TeamSide::Blue);
        assert!(options.stream);
        assert!(!options.record);
        assert!(options.laser_auto);
    }

    #[test]
    fn successful_process_command_clears_previous_send_error() {
        let mut error = Some("previous failure".to_owned());

        store_process_command_result(&mut error, Ok(()));

        assert_eq!(error, None);
    }

    #[test]
    fn failed_process_command_surfaces_send_error() {
        let mut error = None;

        store_process_command_result(&mut error, Err(ProcessSendError));

        assert_eq!(error.as_deref(), Some("process runtime is not available"));
    }

    #[test]
    fn app_clear_color_switches_between_light_and_dark() {
        theme::set_dark_mode(false);
        let light = app_clear_color();
        let expected_light = egui::Color32::from_rgb(0xf5, 0xf7, 0xfb).to_normalized_gamma_f32();
        assert_eq!(light, expected_light);

        theme::set_dark_mode(true);
        let dark = app_clear_color();
        let expected_dark = egui::Color32::from_rgb(0x11, 0x11, 0x1b).to_normalized_gamma_f32();
        assert_eq!(dark, expected_dark);
    }
}
