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
    SdrJammingKeyData, SharedData, GAME_STATE_CMD_ID, IDX_GAME_STATE, IDX_MINIMAP_RECEIVE_RADAR,
    IDX_RADAR_AUTONOMOUS_DECISION_SYNC, IDX_RADAR_MARK_PROCESS, IDX_ROBOT_INTERACTION,
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
    // linger 0：进程退出时立即释放端口，避免孤儿 socket 占用（与 radar_bridge 一致）
    pub_socket.set_linger(0)?;
    pub_socket.bind(bind_addr)?;
    Ok(pub_socket)
}

pub fn zmq_init_sub(thread_num: i32, connect_addrs: &[String]) -> zmq2::Result<zmq2::Socket> {
    let context = zmq2::Context::new();
    context.set_io_threads(thread_num)?;
    let sub_socket = context.socket(zmq2::SUB)?;
    sub_socket.set_linger(0)?;
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
    pub_sockets: Vec<zmq2::Socket>,
    shared: Arc<Mutex<SharedData>>,
    rx: mpsc::Receiver<usize>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let Ok(idx) = rx.recv() else { break };
        let payload = {
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
                    Some(msg.to_string())
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
                    Some(msg.to_string())
                }
                IDX_RADAR_AUTONOMOUS_DECISION_SYNC => {
                    let msg = serde_json::json!({
                        "cmd_id": RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID,
                        "double_weakness_chance": lock.radar_autonomous_decision_sync.double_weakness_chance,
                        "double_weakness_active": lock.radar_autonomous_decision_sync.double_weakness_active,
                        "encryption_rank": lock.radar_autonomous_decision_sync.encryption_rank,
                        "key_modifiable": lock.radar_autonomous_decision_sync.key_modifiable,
                    });
                    Some(msg.to_string())
                }
                _ => None,
            }
        };
        if let Some(payload) = payload {
            let mut sent_ok = true;
            for socket in &pub_sockets {
                if let Err(error) = zmq_send(socket, &payload) {
                    log::error!("ZMQ PUB send failed: {error}");
                    sent_ok = false;
                }
            }
            if sent_ok {
                log::info!(
                    "ZMQ PUB sent to {} socket(s): {}",
                    pub_sockets.len(),
                    payload
                );
            }
        }
    })
}

pub fn zmq_start_sub(
    sub_socket: zmq2::Socket,
    shared: Arc<Mutex<SharedData>>,
    stop: Arc<AtomicBool>,
    tx_slot: Arc<Mutex<Option<mpsc::Sender<usize>>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut last_minimap_send: Option<std::time::Instant> = None;
        loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let Ok(bytes) = sub_socket.recv_bytes(0) else {
            continue;
        };
        // SDR
        if let Ok(msg) = serde_json::from_slice::<SdrMsg>(&bytes) {
            log::info!(
                "ZMQ SUB SDR: hero=({},{}) engineer=({},{}) inf3=({},{}) inf4=({},{}) sentry=({},{}) aerial=({},{})",
                msg.position.hero_x,
                msg.position.hero_y,
                msg.position.engineer_x,
                msg.position.engineer_y,
                msg.position.infantry_3_x,
                msg.position.infantry_3_y,
                msg.position.infantry_4_x,
                msg.position.infantry_4_y,
                msg.position.sentry_x,
                msg.position.sentry_y,
                msg.position.aerial_x,
                msg.position.aerial_y,
            );
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
                // 0x0305 易伤累计坐标：以 SDR 信息波解析的敌方坐标为数据源（覆盖定位坐标）。
                // SDR 坐标为厘米（协议 0x0A01），负值/异常值截断为 0 防 u16 回绕。
                guard.minimap_receive.opponent_hero_x = msg.position.hero_x.max(0) as u16;
                guard.minimap_receive.opponent_hero_y = msg.position.hero_y.max(0) as u16;
                guard.minimap_receive.opponent_engineer_x = msg.position.engineer_x.max(0) as u16;
                guard.minimap_receive.opponent_engineer_y = msg.position.engineer_y.max(0) as u16;
                guard.minimap_receive.opponent_infantry_3_x = msg.position.infantry_3_x.max(0) as u16;
                guard.minimap_receive.opponent_infantry_3_y = msg.position.infantry_3_y.max(0) as u16;
                guard.minimap_receive.opponent_infantry_4_x = msg.position.infantry_4_x.max(0) as u16;
                guard.minimap_receive.opponent_infantry_4_y = msg.position.infantry_4_y.max(0) as u16;
                guard.minimap_receive.opponent_aerial_x = msg.position.aerial_x.max(0) as u16;
                guard.minimap_receive.opponent_aerial_y = msg.position.aerial_y.max(0) as u16;
                guard.minimap_receive.opponent_sentry_x = msg.position.sentry_x.max(0) as u16;
                guard.minimap_receive.opponent_sentry_y = msg.position.sentry_y.max(0) as u16;
                guard.sdr_blood = msg.blood;
                guard.sdr_ammo = msg.ammo;
                guard.sdr_state = msg.state;
                guard.sdr_gain = msg.gain;
                // key 由 SDR 自行更新：破解的干扰密钥注入 0x0121 决策帧（password 字段）
                let jamming_key = msg.key.key;
                guard.sdr_jamming_key = msg.key;
                guard.radar_autonomous_decision.password = jamming_key;
                guard.robot_interaction = RobotInteractionData {
                    subcontext_cmd_id: RADAR_INTERACTION_SUBCONTEXT_CMD_ID,
                    sender_id: DeviceId::Unknown,
                    receiver_id: DeviceId::Unknown,
                    subcontext_data: sub_data,
                };
            }
            notify_tx(&tx_slot, IDX_ROBOT_INTERACTION);
            // 0x0305 频率上限 5Hz：SDR 数据 10Hz，限频通知小地图发送
            let now = std::time::Instant::now();
            if last_minimap_send
                .map_or(true, |t| now.duration_since(t) >= std::time::Duration::from_millis(200))
            {
                last_minimap_send = Some(now);
                notify_tx(&tx_slot, IDX_MINIMAP_RECEIVE_RADAR);
            }
            continue;
        }
        // Laser
        if let Ok(msg) = serde_json::from_slice::<LaserMsg>(&bytes) {
            log::info!(
                "ZMQ SUB Laser: detected={} center=({:.1}, {:.1}) brightness={:.3} contour_points={}",
                msg.detected,
                msg.center[0],
                msg.center[1],
                msg.brightness,
                msg.contour.len(),
            );
            if let Ok(mut guard) = shared.lock() {
                // laser data: if needed, add to SharedData
            }
            continue;
        }
        // Lidar
        if let Ok(msg) = serde_json::from_slice::<LidarMsg>(&bytes) {
            log::info!(
                "ZMQ SUB Lidar: opp hero=({},{}) eng=({},{}) inf3=({},{}) inf4=({},{}) aerial=({},{}) sentry=({},{}) ally hero=({},{}) eng=({},{}) inf3=({},{}) inf4=({},{}) aerial=({},{}) sentry=({},{})",
                msg.opponent_hero_x,
                msg.opponent_hero_y,
                msg.opponent_engineer_x,
                msg.opponent_engineer_y,
                msg.opponent_infantry_3_x,
                msg.opponent_infantry_3_y,
                msg.opponent_infantry_4_x,
                msg.opponent_infantry_4_y,
                msg.opponent_aerial_x,
                msg.opponent_aerial_y,
                msg.opponent_sentry_x,
                msg.opponent_sentry_y,
                msg.ally_hero_x,
                msg.ally_hero_y,
                msg.ally_engineer_x,
                msg.ally_engineer_y,
                msg.ally_infantry_3_x,
                msg.ally_infantry_3_y,
                msg.ally_infantry_4_x,
                msg.ally_infantry_4_y,
                msg.ally_aerial_x,
                msg.ally_aerial_y,
                msg.ally_sentry_x,
                msg.ally_sentry_y,
            );
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
                guard.minimap_receive.opponent_hero_x = msg.opponent_hero_x;
                guard.minimap_receive.opponent_hero_y = msg.opponent_hero_y;
                guard.minimap_receive.opponent_engineer_x = msg.opponent_engineer_x;
                guard.minimap_receive.opponent_engineer_y = msg.opponent_engineer_y;
                guard.minimap_receive.opponent_infantry_3_x = msg.opponent_infantry_3_x;
                guard.minimap_receive.opponent_infantry_3_y = msg.opponent_infantry_3_y;
                guard.minimap_receive.opponent_infantry_4_x = msg.opponent_infantry_4_x;
                guard.minimap_receive.opponent_infantry_4_y = msg.opponent_infantry_4_y;
                guard.minimap_receive.opponent_aerial_x = msg.opponent_aerial_x;
                guard.minimap_receive.opponent_aerial_y = msg.opponent_aerial_y;
                guard.minimap_receive.opponent_sentry_x = msg.opponent_sentry_x;
                guard.minimap_receive.opponent_sentry_y = msg.opponent_sentry_y;
                guard.minimap_receive.ally_hero_x = msg.ally_hero_x;
                guard.minimap_receive.ally_hero_y = msg.ally_hero_y;
                guard.minimap_receive.ally_engineer_x = msg.ally_engineer_x;
                guard.minimap_receive.ally_engineer_y = msg.ally_engineer_y;
                guard.minimap_receive.ally_infantry_3_x = msg.ally_infantry_3_x;
                guard.minimap_receive.ally_infantry_3_y = msg.ally_infantry_3_y;
                guard.minimap_receive.ally_infantry_4_x = msg.ally_infantry_4_x;
                guard.minimap_receive.ally_infantry_4_y = msg.ally_infantry_4_y;
                guard.minimap_receive.ally_aerial_x = msg.ally_aerial_x;
                guard.minimap_receive.ally_aerial_y = msg.ally_aerial_y;
                guard.minimap_receive.ally_sentry_x = msg.ally_sentry_x;
                guard.minimap_receive.ally_sentry_y = msg.ally_sentry_y;
            }
            notify_tx(&tx_slot, IDX_MINIMAP_RECEIVE_RADAR);
            continue;
        }
        log::warn!(
            "ZMQ SUB: message not parsed ({} bytes)",
            bytes.len()
        );
        }
    })
}

fn notify_tx(tx_slot: &Mutex<Option<mpsc::Sender<usize>>>, idx: usize) {
    if let Ok(slot) = tx_slot.lock() {
        if let Some(tx) = slot.as_ref() {
            tx.send(idx).ok();
        }
    }
}
