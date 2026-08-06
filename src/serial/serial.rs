use super::serial_package::serial_package;
use super::serial_parser::SerialParser;
use super::serialconfig::SerialConfig;
use crate::robot_interaction_id::DeviceId;
use crate::shared_data::{RobotInteractionData, SharedData};
use crate::shared_data::{
    IDX_MINIMAP_RECEIVE_RADAR, IDX_ROBOT_INTERACTION, IDX_ROBOT_INTERACTION_DECISION,
    MINIMAP_RECEIVE_RADAR_CMD_ID, RADAR_AUTONOMOUS_DECISION_DATA_CMD_ID,
    RADAR_INTERACTION_SUBCONTEXT_CMD_ID, ROBOT_INTERACTION_CMD_ID,
};
use deku::prelude::*;
use serial2::{SerialPort, Settings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Serial port handle for raw byte I/O via `serial2`.
pub struct Serial {
    serial_port: SerialPort,
    /// Bytes written since construction (used by tests to inspect output).
    pub sent: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Serial {
    /// Open a serial port with the given config.
    pub fn new(config: SerialConfig) -> std::io::Result<Self> {
        let mut port = SerialPort::open(config.port_name, |mut s: Settings| {
            s.set_raw();
            s.set_baud_rate(config.baud_rate)?;
            s.set_char_size(serial2::CharSize::Bits8);
            s.set_stop_bits(serial2::StopBits::One);
            Ok(s)
        })?;
        port.set_read_timeout(Duration::from_millis(1))?;

        Ok(Self {
            serial_port: port,
            sent: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Read raw bytes from the serial port (max 1024 bytes per call).
    pub fn receive_data(&mut self) -> std::io::Result<Vec<u8>> {
        let mut buffer = vec![0u8; 2048];
        loop {
            match self.serial_port.read(&mut buffer) {
                Ok(n) => {
                    buffer.truncate(n);
                    return Ok(buffer);
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::Interrupted
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    return Ok(Vec::new());
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Write bytes to the serial port. Logs I/O errors.
    pub fn send_data(&self, data: &[u8]) {
        if let Err(e) = self.serial_port.write_all(data) {
            log::error!("Serial write error: {e}");
        }
    }

    /// Clone the underlying serial port for concurrent read/write.
    pub fn clone_serial_port(&self) -> std::io::Result<Self> {
        Ok(Self {
            serial_port: self.serial_port.try_clone()?,
            sent: self.sent.clone(),
        })
    }

    /// Wrap an already-open serial port (used by tests with `SerialPort::pair`).
    pub fn from_port(serial_port: SerialPort) -> Self {
        Self {
            serial_port,
            sent: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

/// Spawn a receiver thread that continuously reads from the serial port,
/// parses incoming DJI frames, writes to the shared `SharedData`, and
/// optionally notifies the ZMQ PUB channel on each parsed frame.
pub fn serial_start_receiver(
    mut serial: Serial,
    serial_data: Arc<Mutex<SharedData>>,
    tx_senders: Vec<mpsc::Sender<usize>>,
    stop: Arc<AtomicBool>,
    worker_health: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    let mut serial_parser = if tx_senders.is_empty() {
        SerialParser::new(serial_data.clone())
    } else {
        SerialParser::new_with_tx(serial_data.clone(), tx_senders)
    };
    let mut data: Vec<u8> = Vec::new();
    thread::spawn(move || {
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            match serial.receive_data() {
                Ok(add_data) => {
                    if !add_data.is_empty() {
                        data.extend_from_slice(&add_data);
                        serial_parser.parser(&mut data);
                    }
                }
                Err(error) => {
                    log::error!("Serial read error: {error}");
                    continue;
                }
            }
        }
        worker_health.store(false, Ordering::Relaxed);
    })
}

/// Build and send a single 0x0121 radar autonomous decision frame to the referee.
fn send_decision_frame(serial: &Serial, data: &SharedData, radar_id: DeviceId) {
    let decision_data = data.radar_autonomous_decision.to_bytes().unwrap_or_default();
    let decision = RobotInteractionData {
        subcontext_cmd_id: RADAR_AUTONOMOUS_DECISION_DATA_CMD_ID,
        sender_id: radar_id,
        receiver_id: DeviceId::RefereeServer,
        subcontext_data: decision_data,
    };
    let decision_bytes = decision.to_bytes();
    let decision_frame = serial_package(ROBOT_INTERACTION_CMD_ID, decision_bytes);
    if let Ok(frame_bytes) = decision_frame.to_bytes() {
        serial.send_data(&frame_bytes);
        log::info!(
            "Serial TX 0x0121 radar decision sent: radar_cmd={} password_cmd={} ({} bytes)",
            data.radar_autonomous_decision.radar_cmd,
            data.radar_autonomous_decision.password_cmd,
            frame_bytes.len()
        );
    }
}

/// Spawn a transmitter thread that listens for idx notifications via channel,
/// constructs the corresponding DJI frame with `serial_package`, and sends it.
pub fn serial_start_transmitter(
    serial: Serial,
    serial_data: Arc<Mutex<SharedData>>,
    tx_rx: mpsc::Receiver<usize>,
    stop: Arc<AtomicBool>,
    worker_health: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut last_minimap_sent: Option<std::time::Instant> = None;
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let idx = match tx_rx.recv() {
                Ok(idx) => idx,
                Err(_) => break,
            };
            let data = serial_data.lock().unwrap_or_else(|e| {
                log::error!("SharedData mutex poisoned in serial TX");
                e.into_inner()
            });
            match idx {
                IDX_MINIMAP_RECEIVE_RADAR => {
                    send_minimap(&serial, &data, &mut last_minimap_sent);
                    continue;
                }
                IDX_ROBOT_INTERACTION_DECISION => {
                    // 0x020E sync 触发：只发 0x0121 决策帧（含 key，不依赖 SDR 数据）。
                    let radar_id = if data.radar_side == "blue" {
                        DeviceId::BlueRadar
                    } else {
                        DeviceId::RedRadar
                    };
                    send_decision_frame(&serial, &data, radar_id);
                    continue;
                }
                IDX_ROBOT_INTERACTION => {
                    let mut sub_data = data.robot_interaction.subcontext_data.clone();
                    sub_data.resize(112, 0);
                    let radar_id = if data.radar_side == "blue" {
                        DeviceId::BlueRadar
                    } else {
                        DeviceId::RedRadar
                    };
                    let targets: &[DeviceId] = if data.radar_side == "blue" {
                        &[
                            DeviceId::BlueInfantry3,
                            DeviceId::BlueInfantry4,
                            DeviceId::BlueAerial,
                        ]
                    } else {
                        &[
                            DeviceId::RedInfantry3,
                            DeviceId::RedInfantry4,
                            DeviceId::RedAerial,
                        ]
                    };
                    drop(data);
                    // SDR 数据广播到三个己方单位（步兵3/步兵4/空中，不含英雄与哨兵）；0x0121 决策帧由
                    // IDX_ROBOT_INTERACTION_DECISION（0x020E sync）单独触发。
                    // 帧间 sleep 50ms：配合 ZMQ 侧 1Hz 通知限频，0x0301 帧率 ≤3Hz < 30Hz 上限，
                    // 单次广播约 300ms（写 ~50ms/帧 + 3×50ms sleep），每秒仅占 TX 约 300ms，
                    // 为 0x0305（5Hz）留出充足发送空档。
                    let mut broadcast_frames = 0;
                    for &target in targets {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let interaction = RobotInteractionData {
                            subcontext_cmd_id: RADAR_INTERACTION_SUBCONTEXT_CMD_ID,
                            sender_id: radar_id,
                            receiver_id: target,
                            subcontext_data: sub_data.clone(),
                        };
                        let data_bytes = interaction.to_bytes();
                        let frame = serial_package(ROBOT_INTERACTION_CMD_ID, data_bytes);
                        if let Ok(frame_bytes) = frame.to_bytes() {
                            serial.send_data(&frame_bytes);
                            broadcast_frames += 1;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                    log::info!(
                        "Serial TX 0x0200 SDR broadcast sent: {} frames to {} targets",
                        broadcast_frames,
                        targets.len()
                    );
                    continue;
                }
                // 其它 idx（GAME_STATE / RADAR_MARK_PROCESS / DECISION_SYNC 等）与
                // 串口 TX 无关，静默忽略。
                _ => {
                    drop(data);
                    continue;
                }
            }
        }
        worker_health.store(false, Ordering::Relaxed);
    })
}

/// 发送 0x0305 minimap 帧并输出详细日志（发送间隔 Δ + 六组 opponent 坐标 + 帧长），
/// 用于现场核对发送频率与内容。
fn send_minimap(
    serial: &Serial,
    data: &SharedData,
    last_minimap_sent: &mut Option<std::time::Instant>,
) {
    if let Ok(data_bytes) = data.minimap_receive.to_bytes() {
        let frame = serial_package(MINIMAP_RECEIVE_RADAR_CMD_ID, data_bytes);
        if let Ok(frame_bytes) = frame.to_bytes() {
            serial.send_data(&frame_bytes);
            let delta_ms = last_minimap_sent
                .map(|t| t.elapsed().as_millis() as u64)
                .unwrap_or(0);
            *last_minimap_sent = Some(std::time::Instant::now());
            let m = &data.minimap_receive;
            log::info!(
                "Serial TX 0x0305 minimap sent ({} bytes) Δ={}ms hero=({},{}) eng=({},{}) inf3=({},{}) inf4=({},{}) aerial=({},{}) sentry=({},{})",
                frame_bytes.len(),
                delta_ms,
                m.opponent_hero_x, m.opponent_hero_y,
                m.opponent_engineer_x, m.opponent_engineer_y,
                m.opponent_infantry_3_x, m.opponent_infantry_3_y,
                m.opponent_infantry_4_x, m.opponent_infantry_4_y,
                m.opponent_aerial_x, m.opponent_aerial_y,
                m.opponent_sentry_x, m.opponent_sentry_y,
            );
        }
    }
}
