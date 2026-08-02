use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use deku::prelude::*;

use radar_egui::robot_interaction_id::DeviceId;
use radar_egui::serial::serial::Serial;
use radar_egui::serial::serial_package::serial_package;
use radar_egui::serial::serialconfig::SerialConfig;
use radar_egui::shared_data::{
    RobotInteractionData, SharedData, RADAR_INTERACTION_SUBCONTEXT_CMD_ID, ROBOT_INTERACTION_CMD_ID,
};

fn build_frame(radar_side: &str, target: DeviceId, sdr_data: &[u8]) -> Vec<u8> {
    let radar_id = if radar_side == "blue" {
        DeviceId::BlueRadar
    } else {
        DeviceId::RedRadar
    };
    let mut sub = vec![0x03];
    sub.extend_from_slice(sdr_data);
    sub.resize(112, 0);
    let interaction = RobotInteractionData {
        subcontext_cmd_id: RADAR_INTERACTION_SUBCONTEXT_CMD_ID,
        sender_id: radar_id,
        receiver_id: target,
        subcontext_data: sub,
    };
    let data_bytes = interaction.to_bytes();
    let frame = serial_package(ROBOT_INTERACTION_CMD_ID, data_bytes);
    frame.to_bytes().unwrap()
}

fn print_frame(label: &str, frame: &[u8]) {
    let data_len = u16::from_le_bytes([frame[1], frame[2]]);
    let cmd_id = u16::from_le_bytes([frame[5], frame[6]]);
    let sub_cmd = u16::from_le_bytes([frame[7], frame[8]]);
    let sender = u16::from_le_bytes([frame[9], frame[10]]);
    let receiver = u16::from_le_bytes([frame[11], frame[12]]);
    let sub_len = data_len as usize - 6;
    let sub_start = 13;
    let crc16 = u16::from_le_bytes([frame[frame.len() - 2], frame[frame.len() - 1]]);

    println!("{}", label);
    println!("  total bytes:  {}", frame.len());
    println!(
        "  raw hex:      {}",
        frame
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!("  SOF:          0x{:02X}  (expect A5)", frame[0]);
    println!("  data_len:     {}  (expect 118)", data_len);
    println!("  seq:          {}", frame[3]);
    println!("  crc8:         0x{:02X}", frame[4]);
    println!("  cmd_id:       0x{:04X}  (expect 0301)", cmd_id);
    println!("  sub_cmd_id:   0x{:04X}  (expect 0200)", sub_cmd);
    println!("  sender_id:    {}  (RedRadar=9 / BlueRadar=109)", sender);
    println!("  receiver_id:  {}", receiver);
    println!("  sub_data:     {} bytes (expect 112)", sub_len);
    println!("    [0]  msg_type:  {:02X}  (expect 03)", frame[sub_start]);
    println!(
        "    [1..8]:     {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} ...",
        frame[sub_start + 1],
        frame[sub_start + 2],
        frame[sub_start + 3],
        frame[sub_start + 4],
        frame[sub_start + 5],
        frame[sub_start + 6],
        frame[sub_start + 7],
        frame[sub_start + 8]
    );
    println!(
        "    [71..78]:   {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} ...",
        frame[sub_start + 71],
        frame[sub_start + 72],
        frame[sub_start + 73],
        frame[sub_start + 74],
        frame[sub_start + 75],
        frame[sub_start + 76],
        frame[sub_start + 77],
        frame[sub_start + 78]
    );
    println!(
        "    last 4:     {:02X} {:02X} {:02X} {:02X}",
        frame[sub_start + 108],
        frame[sub_start + 109],
        frame[sub_start + 110],
        frame[sub_start + 111]
    );
    println!("  crc16:        0x{:04X}", crc16);
    println!();
}

#[test]
fn test_detailed_frame_dump() {
    let sdr_data = vec![0xAA; 71];

    println!("\n========================================");
    println!("  5-FRAME ROBOT INTERACTION DUMP");
    println!("  subcontext: 1(msg_type) + 71(SDR) + 40(pad) = 112");
    println!("  subcmd_id:  0x0200");
    println!("========================================\n");

    let red_targets: &[(DeviceId, &str, u16)] = &[
        (DeviceId::RedHero, "RedHero", 1),
        (DeviceId::RedInfantry3, "RedInfantry3", 3),
        (DeviceId::RedInfantry4, "RedInfantry4", 4),
        (DeviceId::RedSentry, "RedSentry", 7),
        (DeviceId::RedAerial, "RedAerial", 6),
    ];

    for (i, (target, name, expected_id)) in red_targets.iter().enumerate() {
        let frame = build_frame("red", *target, &sdr_data);
        let receiver = u16::from_le_bytes([frame[11], frame[12]]);
        assert_eq!(receiver, *expected_id, "receiver_id mismatch for {}", name);
        print_frame(
            &format!("--- Frame #{} → {} (id={}) ---", i, name, expected_id),
            &frame,
        );
    }
}

#[test]
#[ignore]
fn test_tx_continuous() {
    let config = SerialConfig {
        port_name: "/dev/ttyACM0".into(),
        baud_rate: 115_200,
    };
    let serial = Serial::new(config).expect("open /dev/ttyACM0");

    let shared = Arc::new(Mutex::new({
        let mut sd = SharedData::default();
        let mut sub = vec![0x03];
        sub.extend_from_slice(&[0xAA; 71]);
        sub.resize(112, 0);
        sd.robot_interaction = RobotInteractionData {
            subcontext_cmd_id: RADAR_INTERACTION_SUBCONTEXT_CMD_ID,
            sender_id: DeviceId::Unknown,
            receiver_id: DeviceId::Unknown,
            subcontext_data: sub,
        };
        sd.radar_side = "red".to_string();
        sd
    }));

    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    let sh = shared.clone();

    let handle = thread::spawn(move || loop {
        if s.load(Ordering::Relaxed) {
            break;
        }

        let data = sh.lock().unwrap_or_else(|e| e.into_inner());
        let mut sub_data = data.robot_interaction.subcontext_data.clone();
        sub_data.resize(112, 0);
        let radar_id = if data.radar_side == "blue" {
            DeviceId::BlueRadar
        } else {
            DeviceId::RedRadar
        };
        let targets: [DeviceId; 5] = if data.radar_side == "blue" {
            [
                DeviceId::BlueHero,
                DeviceId::BlueInfantry3,
                DeviceId::BlueInfantry4,
                DeviceId::BlueSentry,
                DeviceId::BlueAerial,
            ]
        } else {
            [
                DeviceId::RedHero,
                DeviceId::RedInfantry3,
                DeviceId::RedInfantry4,
                DeviceId::RedSentry,
                DeviceId::RedAerial,
            ]
        };
        drop(data);

        for &target in &targets {
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
                println!(
                    "Sent: cmd=0x{:04X} sender={} receiver={}",
                    ROBOT_INTERACTION_CMD_ID,
                    Into::<u16>::into(radar_id),
                    Into::<u16>::into(target),
                );
            }
            thread::sleep(Duration::from_millis(100));
        }
    });

    println!("Sending 5 frames/round at 10Hz...");
    thread::sleep(Duration::from_secs(300));

    stop.store(true, Ordering::Relaxed);
    handle.join().expect("TX thread panicked");
    println!("Done");
}
