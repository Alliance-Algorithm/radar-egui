use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use radar_egui::robot_interaction_id::DeviceId;
use radar_egui::serial::serial::{serial_start_transmitter, Serial};
use radar_egui::serial::serialconfig::SerialConfig;
use radar_egui::shared_data::{
    MinimapReceiveRadarData, SharedData, IDX_MINIMAP_RECEIVE_RADAR, MINIMAP_RECEIVE_RADAR_CMD_ID,
    RADAR_AUTONOMOUS_DECISION_DATA_CMD_ID, ROBOT_INTERACTION_CMD_ID,
};

fn stop_worker(tx: mpsc::Sender<usize>, handle: thread::JoinHandle<()>, stop: Arc<AtomicBool>) {
    stop.store(true, Ordering::Relaxed);
    drop(tx);
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        handle.join().expect("serial worker panicked");
        done_tx.send(()).expect("send worker completion");
    });
    done_rx
        .recv_timeout(Duration::from_millis(500))
        .expect("serial worker did not stop promptly");
}

fn read_bytes(output: &mut serial2::SerialPort, target_len: usize, timeout: Duration) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 256];
    let deadline = std::time::Instant::now() + timeout;
    while bytes.len() < target_len && std::time::Instant::now() < deadline {
        match output.read(&mut buf) {
            Ok(n) => bytes.extend_from_slice(&buf[..n]),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => break,
        }
    }
    bytes
}

#[test]
#[cfg(unix)]
fn transmitter_sends_minimap_on_notification() {
    let (input, mut output) = serial2::SerialPort::pair().expect("open test serial pair");
    output
        .set_read_timeout(Duration::from_millis(50))
        .expect("set test read timeout");
    let serial = Serial::from_port(input);
    let shared = Arc::new(Mutex::new(SharedData::default()));
    {
        let mut guard = shared.lock().unwrap();
        guard.minimap_receive = MinimapReceiveRadarData {
            opponent_hero_x: 10,
            opponent_hero_y: 20,
            ..Default::default()
        };
    }
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let handle = serial_start_transmitter(
        serial,
        shared,
        rx,
        stop.clone(),
        Arc::new(AtomicBool::new(true)),
    );

    tx.send(IDX_MINIMAP_RECEIVE_RADAR)
        .expect("send minimap notification");

    let frame = read_bytes(&mut output, 55, Duration::from_millis(500));

    assert_eq!(frame[0], 0xA5, "SOF");
    let cmd_id = u16::from_le_bytes([frame[5], frame[6]]);
    assert_eq!(cmd_id, MINIMAP_RECEIVE_RADAR_CMD_ID, "cmd_id 0x0305");
    assert_eq!(
        u16::from_le_bytes([frame[7], frame[8]]),
        10,
        "opponent_hero_x"
    );
    assert_eq!(
        u16::from_le_bytes([frame[9], frame[10]]),
        20,
        "opponent_hero_y"
    );

    stop_worker(tx, handle, stop);
}

#[test]
#[cfg(unix)]
fn broadcast_sends_five_interaction_frames() {
    let (input, mut output) = serial2::SerialPort::pair().expect("open test serial pair");
    output
        .set_read_timeout(Duration::from_millis(50))
        .expect("set test read timeout");
    let serial = Serial::from_port(input);
    let shared = Arc::new(Mutex::new(SharedData::default()));
    {
        let mut guard = shared.lock().unwrap();
        guard.radar_autonomous_decision.radar_cmd = 3;
        guard.radar_autonomous_decision.password_cmd = 2;
    }
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let handle = serial_start_transmitter(
        serial,
        shared,
        rx,
        stop.clone(),
        Arc::new(AtomicBool::new(true)),
    );

    tx.send(radar_egui::shared_data::IDX_ROBOT_INTERACTION)
        .expect("send robot interaction notification");

    // The decision frame (0x0121 subcmd, 14-byte data, receiver =
    // RefereeServer) is sent first, then the 5 broadcast frames (0x0200
    // subcmd, 118-byte data).
    let decision_len = 5 + 2 + 14 + 2;
    let bytes = read_bytes(
        &mut output,
        5 * 127 + decision_len,
        Duration::from_millis(1000),
    );

    assert_eq!(
        bytes.len(),
        5 * 127 + decision_len,
        "one decision + five broadcast frames"
    );
    let decision = &bytes[0..decision_len];
    assert_eq!(decision[0], 0xA5, "decision SOF");
    let cmd_id = u16::from_le_bytes([decision[5], decision[6]]);
    assert_eq!(cmd_id, ROBOT_INTERACTION_CMD_ID, "decision cmd_id 0x0301");
    let subcmd = u16::from_le_bytes([decision[7], decision[8]]);
    assert_eq!(
        subcmd, RADAR_AUTONOMOUS_DECISION_DATA_CMD_ID,
        "decision subcmd 0x0121",
    );
    let sender = u16::from_le_bytes([decision[9], decision[10]]);
    assert_eq!(sender, 9, "sender = RedRadar");
    let receiver = u16::from_le_bytes([decision[11], decision[12]]);
    assert_eq!(receiver, 0x8080, "receiver = RefereeServer");
    assert_eq!(decision[13], 3, "radar_cmd");
    assert_eq!(decision[14], 2, "password_cmd");
    for i in 0..5 {
        let frame = &bytes[decision_len + i * 127..decision_len + (i + 1) * 127];
        assert_eq!(frame[0], 0xA5, "frame {} SOF", i);
        let cmd_id = u16::from_le_bytes([frame[5], frame[6]]);
        assert_eq!(
            cmd_id, ROBOT_INTERACTION_CMD_ID,
            "frame {} cmd_id 0x0301",
            i
        );
        let subcmd = u16::from_le_bytes([frame[7], frame[8]]);
        assert_eq!(subcmd, 0x0200, "frame {} subcmd 0x0200", i);
    }

    stop_worker(tx, handle, stop);
}
