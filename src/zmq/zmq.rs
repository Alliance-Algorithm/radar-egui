use deku::prelude::*;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use zmq2;

use crate::robot_interaction_id::DeviceId;
use crate::shared_data::{
    RobotInteractionData, SdrEnemyRobotBloodData, SdrEnemyRobotGainData,
    SdrEnemyRobotOverallStateData, SdrEnemyRobotPositionData, SdrEnemyRobotRemainingAmmoData,
    SdrJammingKeyData, SharedData, GAME_STATE_CMD_ID, IDX_GAME_STATE,
    IDX_RADAR_AUTONOMOUS_DECISION_SYNC, IDX_RADAR_MARK_PROCESS,
    RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID, RADAR_INTERACTION_SUBCONTEXT_CMD_ID,
    RADAR_MARK_PROCESS_CMD_ID,
};

// ── Private ZMQ message types (JSON deserialization only) ──

#[derive(Deserialize)]
struct SdrMsg {
    cmd_id: u16,
    position: SdrEnemyRobotPositionData,
    blood: SdrEnemyRobotBloodData,
    ammo: SdrEnemyRobotRemainingAmmoData,
    state: SdrEnemyRobotOverallStateData,
    gain: SdrEnemyRobotGainData,
    key: SdrJammingKeyData,
}

#[derive(Deserialize)]
struct LaserMsg {
    cmd_id: u16,
    detected: bool,
    center: [f32; 2],
    brightness: f32,
    contour: Vec<[f32; 2]>,
}

#[derive(Deserialize)]
struct LidarMsg {
    cmd_id: u16,
    opponent_hero_x: u16,
    opponent_hero_y: u16,
    opponent_engineer_x: u16,
    opponent_engineer_y: u16,
    opponent_infantry_3_x: u16,
    opponent_infantry_3_y: u16,
    opponent_infantry_4_x: u16,
    opponent_infantry_4_y: u16,
    opponent_aerial_x: u16,
    opponent_aerial_y: u16,
    opponent_sentry_x: u16,
    opponent_sentry_y: u16,
    ally_hero_x: u16,
    ally_hero_y: u16,
    ally_engineer_x: u16,
    ally_engineer_y: u16,
    ally_infantry_3_x: u16,
    ally_infantry_3_y: u16,
    ally_infantry_4_x: u16,
    ally_infantry_4_y: u16,
    ally_aerial_x: u16,
    ally_aerial_y: u16,
    ally_sentry_x: u16,
    ally_sentry_y: u16,
}

// ── Public API ──

pub fn zmq_init_pub(thread_num: i32, bind_addr: &str) -> zmq2::Result<zmq2::Socket> {
    let context = zmq2::Context::new();
    context.set_io_threads(thread_num)?;
    let pub_socket = context.socket(zmq2::PUB)?;
    pub_socket.bind(bind_addr)?;
    Ok(pub_socket)
}

pub fn zmq_init_sub(thread_num: i32, connect_addrs: &[String]) -> zmq2::Result<zmq2::Socket> {
    let context = zmq2::Context::new();
    context.set_io_threads(thread_num)?;
    let sub_socket = context.socket(zmq2::SUB)?;
    for addr in connect_addrs.iter() {
        sub_socket.connect(addr)?;
    }
    sub_socket.set_subscribe(b"")?;
    Ok(sub_socket)
}

pub fn zmq_send(pub_socket: &zmq2::Socket, msg: &str) -> zmq2::Result<()> {
    pub_socket.send(msg, 0)?;
    Ok(())
}

pub fn zmq_start_pub(
    pub_socket: zmq2::Socket,
    shared: Arc<Mutex<SharedData>>,
    rx: mpsc::Receiver<usize>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let Ok(idx) = rx.recv() else { break };
        let lock = shared.lock().unwrap_or_else(|e| e.into_inner());
        match idx {
            IDX_GAME_STATE => {
                let msg = serde_json::json!({
                    "cmd_id": GAME_STATE_CMD_ID,
                    "game_type": lock.game_state.game_type,
                    "game_progress": lock.game_state.game_progress,
                    "stage_remain_time": lock.game_state.stage_remain_time,
                    "sync_timestamp": lock.game_state.sync_timestamp,
                });
                log::info!("ZMQ PUB GameState: {}", msg);
                zmq_send(&pub_socket, &msg.to_string()).ok();
            }
            IDX_RADAR_MARK_PROCESS => {
                let msg = serde_json::json!({
                    "cmd_id": RADAR_MARK_PROCESS_CMD_ID,
                    "opponent_hero_vulnerable": lock.radar_mark_process.opponent_hero_vulnerable,
                    "opponent_engineer_vulnerable": lock.radar_mark_process.opponent_engineer_vulnerable,
                    "opponent_infantry_3_vulnerable": lock.radar_mark_process.opponent_infantry_3_vulnerable,
                    "opponent_infantry_4_vulnerable": lock.radar_mark_process.opponent_infantry_4_vulnerable,
                    "opponent_aerial_marked": lock.radar_mark_process.opponent_aerial_marked,
                    "opponent_sentry_vulnerable": lock.radar_mark_process.opponent_sentry_vulnerable,
                    "ally_hero_marked": lock.radar_mark_process.ally_hero_marked,
                    "ally_engineer_marked": lock.radar_mark_process.ally_engineer_marked,
                    "ally_infantry_3_marked": lock.radar_mark_process.ally_infantry_3_marked,
                    "ally_infantry_4_marked": lock.radar_mark_process.ally_infantry_4_marked,
                    "ally_aerial_marked": lock.radar_mark_process.ally_aerial_marked,
                    "ally_sentry_marked": lock.radar_mark_process.ally_sentry_marked,
                    "opponent_aerial_targeted": lock.radar_mark_process.opponent_aerial_targeted,
                    "opponent_aerial_countered": lock.radar_mark_process.opponent_aerial_countered,
                    "ally_aerial_targeted": lock.radar_mark_process.ally_aerial_targeted,
                    "ally_aerial_countered": lock.radar_mark_process.ally_aerial_countered,
                });
                log::info!("ZMQ PUB RadarMarkProcess: {}", msg);
                zmq_send(&pub_socket, &msg.to_string()).ok();
            }
            IDX_RADAR_AUTONOMOUS_DECISION_SYNC => {
                let msg = serde_json::json!({
                    "cmd_id": RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID,
                    "double_weakness_chance": lock.radar_autonomous_decision_sync.double_weakness_chance,
                    "double_weakness_active": lock.radar_autonomous_decision_sync.double_weakness_active,
                    "encryption_rank": lock.radar_autonomous_decision_sync.encryption_rank,
                    "key_modifiable": lock.radar_autonomous_decision_sync.key_modifiable,
                });
                log::info!("ZMQ PUB RadarAutonomousDecisionSync: {}", msg);
                zmq_send(&pub_socket, &msg.to_string()).ok();
            }
            _ => {
                log::warn!("ZMQ PUB unknown idx: {}", idx);
            }
        }
        drop(lock);
    })
}

pub fn zmq_start_sub(
    sub_socket: zmq2::Socket,
    shared: Arc<Mutex<SharedData>>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let Ok(bytes) = sub_socket.recv_bytes(0) else {
            continue;
        };
        // SDR
        if let Ok(msg) = serde_json::from_slice::<SdrMsg>(&bytes) {
            let mut sub_data = vec![0x03]; // message_type as first byte
            sub_data.extend_from_slice(&msg.blood.to_bytes().unwrap_or_default());
            sub_data.extend_from_slice(&msg.ammo.to_bytes().unwrap_or_default());
            sub_data.extend_from_slice(&msg.state.to_bytes().unwrap_or_default());
            sub_data.extend_from_slice(&msg.gain.to_bytes().unwrap_or_default());
            if let Ok(mut guard) = shared.lock() {
                guard.enemy_hero.x = msg.position.hero_x;
                guard.enemy_hero.y = msg.position.hero_y;
                guard.enemy_engineer.x = msg.position.engineer_x;
                guard.enemy_engineer.y = msg.position.engineer_y;
                guard.enemy_infantry_3.x = msg.position.infantry_3_x;
                guard.enemy_infantry_3.y = msg.position.infantry_3_y;
                guard.enemy_infantry_4.x = msg.position.infantry_4_x;
                guard.enemy_infantry_4.y = msg.position.infantry_4_y;
                guard.enemy_aerial.x = msg.position.aerial_x;
                guard.enemy_aerial.y = msg.position.aerial_y;
                guard.enemy_sentry.x = msg.position.sentry_x;
                guard.enemy_sentry.y = msg.position.sentry_y;
                guard.sdr_blood = msg.blood;
                guard.sdr_ammo = msg.ammo;
                guard.sdr_state = msg.state;
                guard.sdr_gain = msg.gain;
                guard.sdr_jamming_key = msg.key;
                guard.robot_interaction = RobotInteractionData {
                    subcontext_cmd_id: RADAR_INTERACTION_SUBCONTEXT_CMD_ID,
                    sender_id: DeviceId::Unknown,
                    receiver_id: DeviceId::Unknown,
                    subcontext_data: sub_data,
                };
            }
            continue;
        }
        // Laser
        if let Ok(msg) = serde_json::from_slice::<LaserMsg>(&bytes) {
            if let Ok(mut guard) = shared.lock() {
                // laser data: if needed, add to SharedData
            }
            continue;
        }
        // Lidar
        if let Ok(msg) = serde_json::from_slice::<LidarMsg>(&bytes) {
            if let Ok(mut guard) = shared.lock() {
                guard.enemy_hero.x = msg.opponent_hero_x as i16;
                guard.enemy_hero.y = msg.opponent_hero_y as i16;
                guard.enemy_engineer.x = msg.opponent_engineer_x as i16;
                guard.enemy_engineer.y = msg.opponent_engineer_y as i16;
                guard.enemy_infantry_3.x = msg.opponent_infantry_3_x as i16;
                guard.enemy_infantry_3.y = msg.opponent_infantry_3_y as i16;
                guard.enemy_infantry_4.x = msg.opponent_infantry_4_x as i16;
                guard.enemy_infantry_4.y = msg.opponent_infantry_4_y as i16;
                guard.enemy_aerial.x = msg.opponent_aerial_x as i16;
                guard.enemy_aerial.y = msg.opponent_aerial_y as i16;
                guard.enemy_sentry.x = msg.opponent_sentry_x as i16;
                guard.enemy_sentry.y = msg.opponent_sentry_y as i16;
                guard.ally_hero.x = msg.ally_hero_x as i16;
                guard.ally_hero.y = msg.ally_hero_y as i16;
                guard.ally_engineer.x = msg.ally_engineer_x as i16;
                guard.ally_engineer.y = msg.ally_engineer_y as i16;
                guard.ally_infantry_3.x = msg.ally_infantry_3_x as i16;
                guard.ally_infantry_3.y = msg.ally_infantry_3_y as i16;
                guard.ally_infantry_4.x = msg.ally_infantry_4_x as i16;
                guard.ally_infantry_4.y = msg.ally_infantry_4_y as i16;
                guard.ally_aerial.x = msg.ally_aerial_x as i16;
                guard.ally_aerial.y = msg.ally_aerial_y as i16;
                guard.ally_sentry.x = msg.ally_sentry_x as i16;
                guard.ally_sentry.y = msg.ally_sentry_y as i16;
            }
            continue;
        }
    })
}
