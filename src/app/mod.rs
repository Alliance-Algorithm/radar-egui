use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Once;

use self::video_texture::VideoTextureCache;
use crate::laser::video::VideoFrameReader;
use crate::pointcloud::rerun_visualizer::PointCloudVisualizer;
use crate::rerun_visualizer::RerunVisualizer;
use crate::runtime::{PointCloudRuntime, VideoRuntime, ZmqPubRuntime, ZmqSubRuntime};
use crate::services::process_control::ProcessControl;
use crate::state::{LaserObservationReader, PointCloudFrameReader, SharedReader};
use crate::theme;

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

#[derive(Clone, Copy, PartialEq)]
enum EnemyColor {
    Red,
    Blue,
    Auto,
}

impl EnemyColor {
    fn label(&self) -> &str {
        match self {
            EnemyColor::Red => "Red",
            EnemyColor::Blue => "Blue",
            EnemyColor::Auto => "Auto",
        }
    }

    fn fifo_cmd(&self) -> &str {
        match self {
            EnemyColor::Red => "enemy red",
            EnemyColor::Blue => "enemy blue",
            EnemyColor::Auto => "enemy auto",
        }
    }

    fn sdr_arg(&self) -> &str {
        match self {
            EnemyColor::Red | EnemyColor::Auto => "red",
            EnemyColor::Blue => "blue",
        }
    }
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
    sdr_show_heat: bool,
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

    process_control: ProcessControl,
    camera_device: String,
    enemy_color: EnemyColor,
    radar_side: String,
    stream_on_start: bool,
    record_on_start: bool,

    laser_stage_overlay: bool,
    laser_stage_demo: bool,

    serial_port_name: String,
    serial_baud: u32,
    serial_timeout_ms: u32,
    serial_open: bool,
    serial_error: Option<String>,
    serial_parse_enable: [bool; 6],
    serial_frame_log: std::collections::VecDeque<crate::widgets::SerialFrameLogLine>,
    serial_rx_handle: Option<std::thread::JoinHandle<()>>,
    serial_tx_handle: Option<std::thread::JoinHandle<()>>,
    serial_stop: Option<Arc<AtomicBool>>,
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
            sdr_show_heat: true,
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
            process_control: ProcessControl::new(),
            camera_device: "/dev/laser_capture".to_string(),
            enemy_color: EnemyColor::Auto,
            radar_side: "red".to_string(),
            stream_on_start: true,
            record_on_start: false,
            laser_stage_overlay: true,
            laser_stage_demo: false,
            serial_port_name: "/dev/ttyUSB0".to_string(),
            serial_baud: 115_200,
            serial_timeout_ms: 50,
            serial_open: false,
            serial_error: None,
            serial_parse_enable: [true; 6],
            serial_frame_log: std::collections::VecDeque::new(),
            serial_rx_handle: None,
            serial_tx_handle: None,
            serial_stop: None,
        }
    }
}

impl RadarApp {
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
            timeout: u64::from(self.serial_timeout_ms),
        };
        match Serial::new(config) {
            Ok(port) => match port.clone_serial_port() {
                Ok(port_tx) => {
                    let shared = self.shared_reader.inner();
                    let stop = Arc::new(AtomicBool::new(false));
                    let pub_tx = self.zmq_pub.pub_tx.lock().unwrap().clone();
                    let rx = serial_start_receiver(
                        port,
                        shared.clone(),
                        pub_tx,
                        stop.clone(),
                    );
                    let tx = serial_start_transmitter(port_tx, shared.clone(), stop.clone());
                    self.serial_rx_handle = Some(rx);
                    self.serial_tx_handle = Some(tx);
                    self.serial_stop = Some(stop);
                    self.serial_open = true;
                    self.serial_error = None;
                    log::info!("Serial opened on {}", self.serial_port_name);
                }
                Err(e) => {
                    self.serial_error = Some(format!("clone port: {e}"));
                    log::error!("Serial clone failed: {e}");
                }
            },
            Err(e) => {
                self.serial_error = Some(format!("open: {e}"));
                log::error!("Serial open failed: {e}");
            }
        }
    }

    fn close_serial(&mut self) {
        if let Some(ref stop) = self.serial_stop {
            stop.store(true, Ordering::Relaxed);
        }
        self.serial_stop = None;
        if let Some(handle) = self.serial_rx_handle.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.serial_tx_handle.take() {
            let _ = handle.join();
        }
        self.serial_open = false;
        log::info!("Serial closed");
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

impl eframe::App for RadarApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        app_clear_color()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.setup_fonts(ctx);
        theme::set_dark_mode(self.dark_mode);
        self.ensure_minimap_texture(ctx);
        self.ensure_logo_texture(ctx);
        let snapshot = self.shared_reader.snapshot();
        self.update_connection_status(&snapshot);
        self.apply_theme(ctx);
        self.process_control.trigger_pending_start_all();
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

    #[test]
    fn app_clear_color_matches_theme_app_bg_light() {
        theme::set_dark_mode(false);
        let color = app_clear_color();
        let expected = egui::Color32::from_rgb(0xf5, 0xf7, 0xfb).to_normalized_gamma_f32();
        assert_eq!(color, expected);
    }

    #[test]
    fn app_clear_color_matches_theme_app_bg_dark() {
        theme::set_dark_mode(true);
        let color = app_clear_color();
        let expected = egui::Color32::from_rgb(0x11, 0x11, 0x1b).to_normalized_gamma_f32();
        assert_eq!(color, expected);
    }
}
