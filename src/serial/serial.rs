use super::serial_package::serial_package;
use super::serial_parser::SerialParser;
use super::serialconfig::SerialConfig;
use crate::shared_data::SharedData;
use crate::shared_data::{
    DART_LAUNCH_CMD_ID, GAME_RESULT_CMD_ID, GAME_STATE_CMD_ID,
    RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID, RADAR_MARK_PROCESS_CMD_ID, ROBOT_INTERACTION_CMD_ID,
    SITE_EVENT_CMD_ID,
};
use deku::prelude::*;
use serial2::{SerialPort, Settings};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Serial port handle for raw byte I/O via `serial2`.
pub struct Serial {
    serial_port: SerialPort,
}

impl Serial {
    /// Open a serial port with the given config.
    pub fn new(config: SerialConfig) -> std::io::Result<Self> {
        let port = SerialPort::open(config.port_name, |mut s: Settings| {
            s.set_raw();
            s.set_baud_rate(config.baud_rate)?;
            s.set_char_size(serial2::CharSize::Bits8);
            s.set_stop_bits(serial2::StopBits::One);
            Ok(s)
        })?;

        Ok(Self { serial_port: port })
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
                        || e.kind() == std::io::ErrorKind::Interrupted =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Write bytes to the serial port. Silently ignores errors.
    pub fn send_data(&self, data: &[u8]) {
        let _ = self.serial_port.write_all(data);
    }
    /// Clone the underlying serial port for concurrent read/write.
    pub fn clone_serial_port(&self) -> std::io::Result<Self> {
        Ok(Self {
            serial_port: self.serial_port.try_clone()?,
        })
    }
}

/// Spawn a receiver thread that continuously reads from the serial port,
/// parses incoming DJI frames, writes to the shared `SharedData`, and
/// optionally notifies the ZMQ PUB channel on each parsed frame.
pub fn serial_start_receiver(
    mut serial: Serial,
    serial_data: Arc<Mutex<SharedData>>,
    pub_tx: Option<mpsc::Sender<usize>>,
) -> thread::JoinHandle<()> {
    let mut serial_parser = match pub_tx {
        Some(tx) => SerialParser::new_with_tx(serial_data.clone(), tx),
        None => SerialParser::new(serial_data.clone()),
    };
    let mut data: Vec<u8> = Vec::new();
    thread::spawn(move || loop {
        match serial.receive_data() {
            Ok(add_data) => {
                data.extend_from_slice(&add_data);
                serial_parser.parser(&mut data);
            }
            Err(_) => continue,
        }
    })
}

/// Spawn a transmitter thread that polls shared state every 10 ms,
/// constructs DJI frames with `serial_package`, and sends them over the serial port.
pub fn serial_start_transmitter(
    serial: Serial,
    serial_data: Arc<Mutex<SharedData>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        let mut data = serial_data.lock().unwrap();
        for idx in 0..7 {
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
                    let b = data.robot_interaction.to_bytes();
                    (ROBOT_INTERACTION_CMD_ID, Ok(b))
                }
                _ => continue,
            };
            if let Ok(data_bytes) = raw {
                let frame = serial_package(cmd_id, data_bytes);
                if let Ok(frame_bytes) = frame.to_bytes() {
                    serial.send_data(&frame_bytes);
                }
            }
        }
        drop(data);
        thread::sleep(Duration::from_millis(10));
    })
}
