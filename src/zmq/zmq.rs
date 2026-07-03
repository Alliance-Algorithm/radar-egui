use std::io::Result;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use zmq2;

use crate::serial::data_format::{
    SerialData, IDX_GAME_STATE, IDX_RADAR_AUTONOMOUS_DECISION_SYNC, IDX_RADAR_MARK_PROCESS,
};
use crate::zmq::data_format::{
    ReceiveLaser, ReceiveLidarLocation, ReceiveSdr, TransmitGameState, TransmitRadarMarkProcess,
    TransmitRadarSync, ZmqData, IDX_ZMQ_LASER, IDX_ZMQ_LIDAR, IDX_ZMQ_SDR, ZMQ_PUB_GAME_STATE,
    ZMQ_PUB_RADAR_MARK, ZMQ_PUB_RADAR_SYNC,
};
pub fn zmq_init(
    thread_num: i32,
    pub_str: &str,
    sub_str: &[String],
) -> zmq2::Result<(zmq2::Socket, zmq2::Socket, &'static str)> {
    let context = zmq2::Context::new();
    context.set_io_threads(thread_num)?;
    let pub_socket = context.socket(zmq2::PUB)?;
    let sub_socket = context.socket(zmq2::SUB)?;
    sub_socket.set_connect_timeout(100)?;
    pub_socket.bind(pub_str)?;
    for index in sub_str.iter() {
        sub_socket.connect(index)?;
    }
    Ok((
        pub_socket,
        sub_socket,
        "Has been initialized pub and sub socket successfully",
    ))
}
pub fn zmq_send(pub_socket: &zmq2::Socket, msg: &str) -> zmq2::Result<()> {
    pub_socket.send(msg, 0)?;
    Ok(())
}
pub fn zmq_recv(sub_socket: &zmq2::Socket) -> zmq2::Result<Vec<u8>> {
    sub_socket.recv_bytes(0)
}
pub fn start_zmq_pub(
    pub_socket: zmq2::Socket,
    zmq_data: Arc<Mutex<ZmqData>>,
    serial_data: Arc<Mutex<SerialData>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        zmq_serial_update(&zmq_data, &serial_data);

        let mut zmq_lock = zmq_data.lock().unwrap();
        if let Some(ref data) = zmq_lock.game_state.take() {
            if let Ok(msg) = serde_json::to_string(data) {
                zmq_send(&pub_socket, &msg).ok();
            }
        }
        if let Some(ref data) = zmq_lock.radar_mark.take() {
            if let Ok(msg) = serde_json::to_string(data) {
                zmq_send(&pub_socket, &msg).ok();
            }
        }
        if let Some(ref data) = zmq_lock.radar_sync.take() {
            if let Ok(msg) = serde_json::to_string(data) {
                zmq_send(&pub_socket, &msg).ok();
            }
        }
        drop(zmq_lock);
        thread::sleep(Duration::from_millis(10));
    })
}
pub fn start_zmq_sub(
    sub_socket: zmq2::Socket,
    zmq_data: Arc<Mutex<ZmqData>>,
    serial_data: Arc<Mutex<SerialData>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        let bytes = match zmq_recv(&sub_socket) {
            Ok(b) => b,
            Err(_) => {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        // SDR
        if let Ok(sdr) = serde_json::from_slice::<ReceiveSdr>(&bytes) {
            if let Ok(mut z) = zmq_data.lock() {
                z.sdr = Some(sdr);
                z.zmq_produce[IDX_ZMQ_SDR] = 1;
            }
            continue;
        }
        // Laser
        if let Ok(laser) = serde_json::from_slice::<ReceiveLaser>(&bytes) {
            if let Ok(mut z) = zmq_data.lock() {
                z.laser = Some(laser);
                z.zmq_produce[IDX_ZMQ_LASER] = 1;
            }
            continue;
        }
        // Lidar
        if let Ok(lidar) = serde_json::from_slice::<ReceiveLidarLocation>(&bytes) {
            if let Ok(mut z) = zmq_data.lock() {
                z.lidar = Some(lidar);
                z.zmq_produce[IDX_ZMQ_LIDAR] = 1;
            }
            continue;
        }
    })
}
/// Poll `serial_produced[idx]` flags and copy updated fields into `ZmqData` PUB slots.
pub fn zmq_serial_update(zmq_data: &Arc<Mutex<ZmqData>>, serial_data: &Arc<Mutex<SerialData>>) {
    let mut zmq_lock = zmq_data.lock().unwrap();
    let mut serial_lock = serial_data.lock().unwrap();

    if serial_lock.serial_produced[IDX_GAME_STATE] != 0 {
        let src = &serial_lock.game_state_data;
        zmq_lock.game_state = Some(TransmitGameState {
            cmd_id: ZMQ_PUB_GAME_STATE,
            game_type: src.game_type,
            game_progress: src.game_progress,
            stage_remain_time: src.stage_remain_time,
            sync_timestamp: src.sync_timestamp,
        });
        serial_lock.serial_produced[IDX_GAME_STATE] = 0;
    }

    if serial_lock.serial_produced[IDX_RADAR_MARK_PROCESS] != 0 {
        let src = &serial_lock.radar_mark_process_data;
        zmq_lock.radar_mark = Some(TransmitRadarMarkProcess {
            cmd_id: ZMQ_PUB_RADAR_MARK,
            opponent_hero_vulnerable: src.opponent_hero_vulnerable,
            opponent_engineer_vulnerable: src.opponent_engineer_vulnerable,
            opponent_infantry_3_vulnerable: src.opponent_infantry_3_vulnerable,
            opponent_infantry_4_vulnerable: src.opponent_infantry_4_vulnerable,
            opponent_aerial_marked: src.opponent_aerial_marked,
            opponent_sentry_vulnerable: src.opponent_sentry_vulnerable,
            ally_hero_marked: src.ally_hero_marked,
            ally_engineer_marked: src.ally_engineer_marked,
            ally_infantry_3_marked: src.ally_infantry_3_marked,
            ally_infantry_4_marked: src.ally_infantry_4_marked,
            ally_aerial_marked: src.ally_aerial_marked,
            ally_sentry_marked: src.ally_sentry_marked,
            opponent_aerial_targeted: src.opponent_aerial_targeted,
            opponent_aerial_countered: src.opponent_aerial_countered,
            ally_aerial_targeted: src.ally_aerial_targeted,
            ally_aerial_countered: src.ally_aerial_countered,
        });
        serial_lock.serial_produced[IDX_RADAR_MARK_PROCESS] = 0;
    }

    if serial_lock.serial_produced[IDX_RADAR_AUTONOMOUS_DECISION_SYNC] != 0 {
        let src = &serial_lock.radar_autonomous_decision_sync_data;
        zmq_lock.radar_sync = Some(TransmitRadarSync {
            cmd_id: ZMQ_PUB_RADAR_SYNC,
            double_weakness_chance: src.double_weakness_chance,
            double_weakness_active: src.double_weakness_active,
            encryption_rank: src.encryption_rank,
            key_modifiable: src.key_modifiable,
        });
        serial_lock.serial_produced[IDX_RADAR_AUTONOMOUS_DECISION_SYNC] = 0;
    }
}
pub fn zmq_sdr_lidar_fusion(zmq_data: &Arc<Mutex<ZmqData>>) {
    let zmq_lock = zmq_data.lock().unwrap();
    if zmq_lock.zmq_produce[IDX_ZMQ_LIDAR] != 0 && zmq_lock.zmq_produce[IDX_ZMQ_SDR] != 0 {
        let sdr_position = zmq_lock.sdr.clone().unwrap().position;
        let lidar_position = zmq_lock.lidar.clone().unwrap();
    }
}
