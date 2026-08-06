use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use radar_egui::robot_interaction_id::DeviceId;
use radar_egui::serial::serial::{serial_start_transmitter, Serial};
use radar_egui::serial::serial_crc;
use radar_egui::serial::serial_parser::SerialParser;
use radar_egui::serial::serialconfig::SerialConfig;
use radar_egui::shared_data::{
    CMD_ID_LENGTH, CRC16_LENGTH, FRAME_HEADER_LENGTH, FRAME_HEADER_SOF,
    MinimapReceiveRadarData, RadarAutonomousDecisionSyncData, SharedData,
    IDX_MINIMAP_RECEIVE_RADAR, IDX_RADAR_AUTONOMOUS_DECISION_SYNC,
    IDX_ROBOT_INTERACTION_DECISION, MINIMAP_RECEIVE_RADAR_CMD_ID,
    RADAR_AUTONOMOUS_DECISION_DATA_CMD_ID, RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID,
    ROBOT_INTERACTION_CMD_ID,
};
use deku::prelude::*;

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
fn broadcast_sends_three_interaction_frames() {
    let (input, mut output) = serial2::SerialPort::pair().expect("open test serial pair");
    output
        .set_read_timeout(Duration::from_millis(50))
        .expect("set test read timeout");
    let serial = Serial::from_port(input);
    let shared = Arc::new(Mutex::new(SharedData::default()));
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

    // SDR 驱动的 0x0200 广播仅 3 帧（步兵3/步兵4/空中，不含英雄与哨兵；
    // 0x0121 决策帧由 0x020E sync 单独触发）。
    let bytes = read_bytes(&mut output, 3 * 127, Duration::from_millis(1000));

    assert_eq!(bytes.len(), 3 * 127, "three broadcast frames only");
    for i in 0..3 {
        let frame = &bytes[i * 127..(i + 1) * 127];
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

#[test]
#[cfg(unix)]
fn decision_notification_sends_single_0121_frame() {
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

    tx.send(radar_egui::shared_data::IDX_ROBOT_INTERACTION_DECISION)
        .expect("send decision notification");

    // 0x0121 决策帧：14-byte data，receiver = RefereeServer。
    let decision_len = 5 + 2 + 14 + 2;
    let decision = read_bytes(&mut output, decision_len, Duration::from_millis(1000));

    assert_eq!(decision.len(), decision_len, "single decision frame");
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

    stop_worker(tx, handle, stop);
}

// ─── 0x020E 双倍易伤评估（不同 chance/active 数据）───

fn build_020e_frame(chance: u8, active: u8) -> Vec<u8> {
    let data = RadarAutonomousDecisionSyncData {
        double_weakness_chance: chance,
        double_weakness_active: active,
        ..Default::default()
    }
    .to_bytes()
    .unwrap();
    let total = FRAME_HEADER_LENGTH + CMD_ID_LENGTH + data.len() + CRC16_LENGTH;
    let mut frame = vec![0u8; total];
    frame[0] = FRAME_HEADER_SOF;
    frame[1] = data.len() as u8;
    frame[3] = 0; // seq
    frame[5] = (RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID & 0xff) as u8;
    frame[6] = (RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID >> 8) as u8;
    frame[FRAME_HEADER_LENGTH + CMD_ID_LENGTH..FRAME_HEADER_LENGTH + CMD_ID_LENGTH + data.len()]
        .copy_from_slice(&data);
    serial_crc::append_crc8(&mut frame[..FRAME_HEADER_LENGTH]).unwrap();
    serial_crc::append_crc16(&mut frame).unwrap();
    frame
}

fn drain_notifications(rx: &mpsc::Receiver<usize>) -> Vec<usize> {
    let mut out = Vec::new();
    while let Ok(idx) = rx.try_recv() {
        out.push(idx);
    }
    out
}

fn parse_020e(parser: &mut SerialParser, rx: &mpsc::Receiver<usize>, chance: u8, active: u8) -> Vec<usize> {
    let mut buffer = build_020e_frame(chance, active);
    parser.parser(&mut buffer);
    drain_notifications(rx)
}

#[test]
fn parser_020e_decision_eval_varies_with_chance_and_active() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    {
        let mut guard = shared.lock().unwrap();
        guard.game_state.game_progress = 4;
    }
    let (tx, rx) = mpsc::channel();
    let mut parser = SerialParser::new_with_tx(shared.clone(), vec![tx]);
    let decision_idx = IDX_ROBOT_INTERACTION_DECISION;

    // 1) 首次请求：chance=1 active=0（尚未生效）→ radar_cmd 0 -> 1，触发 0x0121
    let notifs = parse_020e(&mut parser, &rx, 1, 0);
    assert!(notifs.contains(&decision_idx), "active=0 chance>0 应触发首次双倍易伤");
    assert_eq!(shared.lock().unwrap().radar_autonomous_decision.radar_cmd, 1);

    // 2) 再次请求：chance=1 active=0 → radar_cmd 1 -> 2（单调 +1）
    parse_020e(&mut parser, &rx, 1, 0);
    assert_eq!(shared.lock().unwrap().radar_autonomous_decision.radar_cmd, 2);

    // 3) 生效期/排队：chance=1 active=1 → radar_cmd 已到上限 2，保持不超
    let notifs = parse_020e(&mut parser, &rx, 1, 1);
    assert!(notifs.contains(&decision_idx));
    assert_eq!(shared.lock().unwrap().radar_autonomous_decision.radar_cmd, 2);

    // 4) chance=0 active=1：无机会不累加（radar_cmd 保持 2，不破坏单调递增），仍触发（key 照常传输）
    let notifs = parse_020e(&mut parser, &rx, 0, 1);
    assert!(notifs.contains(&decision_idx), "chance=0 仍发 0x0121（key 传输）");
    assert_eq!(shared.lock().unwrap().radar_autonomous_decision.radar_cmd, 2);

    // 5) 新局：progress=5（结算）→ chance 清零，radar_cmd 归 0（下一局开局为 0），不发决策帧
    {
        let mut guard = shared.lock().unwrap();
        guard.game_state.game_progress = 5;
    }
    let notifs = parse_020e(&mut parser, &rx, 2, 1);
    assert!(!notifs.contains(&decision_idx), "progress=5 不发 0x0121");
    assert_eq!(
        shared.lock().unwrap().radar_autonomous_decision_sync.double_weakness_chance,
        0,
        "progress=5 清零 chance"
    );
    assert_eq!(
        shared.lock().unwrap().radar_autonomous_decision.radar_cmd,
        0,
        "progress=5 重置 radar_cmd（下一局开局为 0）"
    );

    // 6) 新局首次请求：progress=4 且上一局已重置 → 再次 0 -> 1
    {
        let mut guard = shared.lock().unwrap();
        guard.game_state.game_progress = 4;
    }
    let notifs = parse_020e(&mut parser, &rx, 1, 0);
    assert!(notifs.contains(&decision_idx));
    assert_eq!(shared.lock().unwrap().radar_autonomous_decision.radar_cmd, 1);

    // 7) progress=1（其它阶段）：不发决策帧，值不变
    {
        let mut guard = shared.lock().unwrap();
        guard.game_state.game_progress = 1;
    }
    let notifs = parse_020e(&mut parser, &rx, 1, 1);
    assert!(!notifs.contains(&decision_idx), "非比赛阶段不发 0x0121");
    assert_eq!(shared.lock().unwrap().radar_autonomous_decision.radar_cmd, 1, "非比赛阶段值不变");

    // 8) 每次 0x020E 解析都通知 ZMQ PUB（IDX_RADAR_AUTONOMOUS_DECISION_SYNC）
    let notifs = parse_020e(&mut parser, &rx, 1, 1);
    assert!(notifs.contains(&IDX_RADAR_AUTONOMOUS_DECISION_SYNC));
}
