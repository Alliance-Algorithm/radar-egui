use super::serial_package::serial_package;
use super::serial_parser::SerialParser;
use super::serialconfig::SerialConfig;
use crate::robot_interaction_id::DeviceId;
use crate::shared_data::{RobotInteractionData, SharedData};
use crate::shared_data::{
    DART_LAUNCH_CMD_ID, GAME_RESULT_CMD_ID, GAME_STATE_CMD_ID,
    RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID, RADAR_INTERACTION_SUBCONTEXT_CMD_ID,
    RADAR_MARK_PROCESS_CMD_ID, ROBOT_INTERACTION_CMD_ID, SITE_EVENT_CMD_ID,
};
use deku::prelude::*;
use serial2::{SerialPort, Settings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const SERIAL_READ_TIMEOUT: Duration = Duration::from_millis(50);

/// Serial port handle for raw byte I/O via `serial2`.
pub struct Serial {
    serial_port: SerialPort,
    #[cfg(test)]
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

        port.set_read_timeout(SERIAL_READ_TIMEOUT)?;

        Ok(Self {
            serial_port: port,
            #[cfg(test)]
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
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
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
            #[cfg(test)]
            sent: self.sent.clone(),
        })
    }

    #[cfg(test)]
    pub fn from_port(mut serial_port: SerialPort) -> Self {
        serial_port
            .set_read_timeout(SERIAL_READ_TIMEOUT)
            .expect("configure test serial read timeout");
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
) -> thread::JoinHandle<()> {
    let mut serial_parser = if tx_senders.is_empty() {
        SerialParser::new(serial_data.clone())
    } else {
        SerialParser::new_with_tx(serial_data.clone(), tx_senders)
    };
    let mut data: Vec<u8> = Vec::new();
    thread::spawn(move || loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        match serial.receive_data() {
            Ok(add_data) => {
                data.extend_from_slice(&add_data);
                serial_parser.parser(&mut data);
            }
            Err(_) => continue,
        }
    })
}

/// Spawn a transmitter thread that listens for idx notifications via channel,
/// constructs the corresponding DJI frame with `serial_package`, and sends it.
pub fn serial_start_transmitter(
    serial: Serial,
    serial_data: Arc<Mutex<SharedData>>,
    tx_rx: mpsc::Receiver<usize>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let idx = match tx_rx.recv_timeout(SERIAL_READ_TIMEOUT) {
            Ok(idx) => idx,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let data = serial_data.lock().unwrap_or_else(|e| {
            log::error!("SharedData mutex poisoned in serial TX");
            e.into_inner()
        });
        let (cmd_id, raw) = match idx {
            0 => (GAME_STATE_CMD_ID, data.game_state.to_bytes()),
            1 => (GAME_RESULT_CMD_ID, data.game_result.to_bytes()),
            2 => (SITE_EVENT_CMD_ID, data.site_event.to_bytes()),
            3 => (DART_LAUNCH_CMD_ID, data.dart_launch.to_bytes()),
            4 => (
                RADAR_MARK_PROCESS_CMD_ID,
                data.radar_mark_process.to_bytes(),
            ),
            5 => (
                RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID,
                data.radar_autonomous_decision_sync.to_bytes(),
            ),
            6 => {
                let mut sub_data = data.robot_interaction.subcontext_data.clone();
                sub_data.resize(112, 0);
                let radar_id = if data.radar_side == "blue" {
                    DeviceId::BlueRadar
                } else {
                    DeviceId::RedRadar
                };
                let targets: &[DeviceId] = if data.radar_side == "blue" {
                    &[
                        DeviceId::BlueHero,
                        DeviceId::BlueInfantry3,
                        DeviceId::BlueInfantry4,
                        DeviceId::BlueSentry,
                        DeviceId::BlueAerial,
                    ]
                } else {
                    &[
                        DeviceId::RedHero,
                        DeviceId::RedInfantry3,
                        DeviceId::RedInfantry4,
                        DeviceId::RedSentry,
                        DeviceId::RedAerial,
                    ]
                };
                drop(data);
                for &target in targets {
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
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                continue;
            }
            _ => {
                log::warn!("Serial TX unknown idx: {}", idx);
                drop(data);
                continue;
            }
        };
        drop(data);
        if let Ok(data_bytes) = raw {
            let frame = serial_package(cmd_id, data_bytes);
            if let Ok(frame_bytes) = frame.to_bytes() {
                serial.send_data(&frame_bytes);
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_worker_stops(handle: thread::JoinHandle<()>, stop: Arc<AtomicBool>) {
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            handle.join().expect("serial worker panicked");
            done_tx.send(()).expect("send worker completion");
        });

        stop.store(true, Ordering::Relaxed);
        done_rx
            .recv_timeout(Duration::from_millis(500))
            .expect("serial worker did not stop promptly");
    }

    #[test]
    #[cfg(unix)]
    fn idle_receiver_stops_after_stop_is_set() {
        let serial = Serial::new(SerialConfig {
            port_name: "/dev/ptmx".into(),
            baud_rate: 115_200,
        })
        .expect("open test serial device");
        let shared = Arc::new(Mutex::new(SharedData::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let handle = serial_start_receiver(serial, shared, Vec::new(), stop.clone());

        thread::sleep(Duration::from_millis(20));
        assert_worker_stops(handle, stop);
    }

    #[test]
    fn idle_transmitter_stops_after_stop_is_set() {
        let serial = Serial::new(SerialConfig {
            port_name: "/dev/ptmx".into(),
            baud_rate: 115_200,
        })
        .expect("open test serial device");
        let shared = Arc::new(Mutex::new(SharedData::default()));
        let (_tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let handle = serial_start_transmitter(serial, shared, rx, stop.clone());

        thread::sleep(Duration::from_millis(20));
        assert_worker_stops(handle, stop);
    }
}
