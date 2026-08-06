use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::json;

use radar_egui::shared_data::{
    DartLaunchData, GameResultData, GameStateData, RadarMarkProcessData, SdrEnemyRobotBloodData,
    SdrEnemyRobotGainData, SdrEnemyRobotOverallStateData, SdrEnemyRobotPositionData,
    SdrEnemyRobotRemainingAmmoData, SdrJammingKeyData, SharedData, GAME_STATE_CMD_ID,
    IDX_GAME_STATE, IDX_MINIMAP_RECEIVE_RADAR, IDX_RADAR_MARK_PROCESS, IDX_ROBOT_INTERACTION, RADAR_MARK_PROCESS_CMD_ID,
};
use radar_egui::zmq::zmq::{zmq_send, zmq_start_pub, zmq_start_sub};

const INPROC_ADDR: &str = "inproc://zmq-test";

fn make_pair() -> (zmq2::Socket, zmq2::Socket) {
    make_pair_at("inproc://zmq-test")
}

fn make_pair_at(addr: &str) -> (zmq2::Socket, zmq2::Socket) {
    let ctx = zmq2::Context::new();
    ctx.set_io_threads(1).unwrap();

    let pub_sock = ctx.socket(zmq2::PUB).unwrap();
    pub_sock.bind(addr).unwrap();

    let sub_sock = ctx.socket(zmq2::SUB).unwrap();
    sub_sock.set_subscribe(b"").unwrap();
    sub_sock.set_rcvtimeo(200).unwrap(); // 200ms timeout so the thread can check stop
    sub_sock.connect(addr).unwrap();

    (pub_sock, sub_sock)
}

/// Build a complete SDR JSON as the external SDR bridge would send.
fn make_sdr_json(
    blood: &SdrEnemyRobotBloodData,
    ammo: &SdrEnemyRobotRemainingAmmoData,
    state: &SdrEnemyRobotOverallStateData,
    gain: &SdrEnemyRobotGainData,
) -> String {
    json!({
        "cmd_id": 0x2002,
        "position": {
            "hero_x": 100, "hero_y": 200,
            "engineer_x": 300, "engineer_y": 400,
            "infantry_3_x": 500, "infantry_3_y": 600,
            "infantry_4_x": 700, "infantry_4_y": 800,
            "aerial_x": 900, "aerial_y": 1000,
            "sentry_x": 1100, "sentry_y": 1200,
        },
        "blood": blood,
        "ammo": ammo,
        "state": state,
        "gain": gain,
        "key": { "key": [1, 2, 3, 4, 5, 6] },
    })
    .to_string()
}

fn make_lidar_json() -> String {
    json!({
        "cmd_id": 0x2001,
        "opponent_hero_x": 10, "opponent_hero_y": 20,
        "opponent_engineer_x": 30, "opponent_engineer_y": 40,
        "opponent_infantry_3_x": 50, "opponent_infantry_3_y": 60,
        "opponent_infantry_4_x": 70, "opponent_infantry_4_y": 80,
        "opponent_aerial_x": 90, "opponent_aerial_y": 100,
        "opponent_sentry_x": 110, "opponent_sentry_y": 120,
        "ally_hero_x": 1000, "ally_hero_y": 2000,
        "ally_engineer_x": 3000, "ally_engineer_y": 4000,
        "ally_infantry_3_x": 5000, "ally_infantry_3_y": 6000,
        "ally_infantry_4_x": 7000, "ally_infantry_4_y": 8000,
        "ally_aerial_x": 9000, "ally_aerial_y": 10000,
        "ally_sentry_x": 11000, "ally_sentry_y": 12000,
    })
    .to_string()
}

// ─── SDR SUB tests ───

#[test]
fn test_zmq_sub_sdr_populates_fields() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    let stop = Arc::new(AtomicBool::new(false));

    let (pub_sock, sub_sock) = make_pair();
    let _handle = zmq_start_sub(
        sub_sock,
        shared.clone(),
        stop.clone(),
        Arc::new(Mutex::new(None)),
    );

    thread::sleep(Duration::from_millis(50)); // let sub handshake

    let test_blood = SdrEnemyRobotBloodData {
        hero_blood: 1000,
        engineer_blood: 2000,
        infantry_3_blood: 3000,
        infantry_4_blood: 4000,
        reserved: 0,
        sentry_blood: 5000,
    };
    let test_ammo = SdrEnemyRobotRemainingAmmoData {
        hero_ammo: 10,
        infantry_3_ammo: 20,
        infantry_4_ammo: 30,
        aerial_ammo: 40,
        sentry_ammo: 50,
    };
    let test_state = SdrEnemyRobotOverallStateData {
        remaining_gold: 100,
        total_gold: 500,
        supply_zone_status: 1,
        central_highland_status: 2,
        trapezoid_highland_status: 0,
        fortress_gain_status: 1,
        outpost_gain_status: 0,
        base_gain_status: 1,
        tunnel_1_status: 1,
        tunnel_2_status: 0,
        tunnel_3_status: 0,
        tunnel_4_status: 0,
        highland_upper_status: 0,
        ramp_rear_status: 0,
        road_upper_status: 0,
    };
    let test_gain = SdrEnemyRobotGainData {
        hero_hp_recovery: 1,
        hero_cooling_acceleration: 2,
        hero_defence: 3,
        hero_negative_defence: 4,
        hero_attack: 5,
        engineer_hp_recovery: 6,
        engineer_cooling_acceleration: 7,
        engineer_defence: 8,
        engineer_negative_defence: 9,
        engineer_attack: 10,
        infantry_3_hp_recovery: 11,
        infantry_3_cooling_acceleration: 12,
        infantry_3_defence: 13,
        infantry_3_negative_defence: 14,
        infantry_3_attack: 15,
        infantry_4_hp_recovery: 16,
        infantry_4_cooling_acceleration: 17,
        infantry_4_defence: 18,
        infantry_4_negative_defence: 19,
        infantry_4_attack: 20,
        sentry_hp_recovery: 21,
        sentry_cooling_acceleration: 22,
        sentry_defence: 23,
        sentry_negative_defence: 24,
        sentry_attack: 25,
        sentry_posture: 0,
        hero_state: 1,
        engineer_state: 1,
        infantry_3_state: 1,
        infantry_4_state: 1,
        sentry_state: 0,
    };

    let json = make_sdr_json(&test_blood, &test_ammo, &test_state, &test_gain);
    zmq_send(&pub_sock, &json).unwrap();
    thread::sleep(Duration::from_millis(100));

    let guard = shared.lock().unwrap();
    assert_eq!(guard.sdr_blood.hero_blood, 1000, "hero_blood");
    assert_eq!(guard.sdr_blood.sentry_blood, 5000, "sentry_blood");
    assert_eq!(guard.sdr_ammo.hero_ammo, 10, "hero_ammo");
    assert_eq!(guard.sdr_ammo.sentry_ammo, 50, "sentry_ammo");
    assert_eq!(guard.sdr_state.remaining_gold, 100, "remaining_gold");
    assert_eq!(guard.sdr_gain.hero_hp_recovery, 1, "hero_hp_recovery");
    assert_eq!(guard.sdr_gain.sentry_posture, 0, "sentry_posture");
    assert_eq!(guard.sdr_jamming_key.key, [1, 2, 3, 4, 5, 6], "key");
    assert_eq!(guard.enemy_hero.x, 100, "enemy_hero.x");
    assert_eq!(guard.enemy_hero.y, 200, "enemy_hero.y");
    drop(guard);

    stop.store(true, Ordering::Relaxed);
    drop(pub_sock);
    // Give the sub thread time to notice stop (200ms rcvtimeo)
    thread::sleep(Duration::from_millis(300));
}

#[test]
fn test_zmq_sub_sdr_populates_robot_interaction() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    let stop = Arc::new(AtomicBool::new(false));

    let (pub_sock, sub_sock) = make_pair();
    let _handle = zmq_start_sub(
        sub_sock,
        shared.clone(),
        stop.clone(),
        Arc::new(Mutex::new(None)),
    );

    thread::sleep(Duration::from_millis(50));

    let json = make_sdr_json(
        &SdrEnemyRobotBloodData {
            hero_blood: 100,
            ..Default::default()
        },
        &SdrEnemyRobotRemainingAmmoData {
            hero_ammo: 10,
            ..Default::default()
        },
        &SdrEnemyRobotOverallStateData::default(),
        &SdrEnemyRobotGainData::default(),
    );
    zmq_send(&pub_sock, &json).unwrap();
    thread::sleep(Duration::from_millis(100));

    let guard = shared.lock().unwrap();
    assert_eq!(
        guard.robot_interaction.subcontext_cmd_id, 0x0200,
        "subcontext_cmd_id"
    );
    let sub = &guard.robot_interaction.subcontext_data;
    assert!(!sub.is_empty(), "subcontext_data not empty");
    assert_eq!(sub[0], 0x03, "msg_type at [0]");
    drop(guard);

    stop.store(true, Ordering::Relaxed);
    drop(pub_sock);
    thread::sleep(Duration::from_millis(300));
}

#[test]
fn test_zmq_sub_sdr_does_not_touch_autonomous_decision() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    {
        let mut guard = shared.lock().unwrap();
        guard.game_state.game_progress = 4;
        guard.radar_autonomous_decision_sync.double_weakness_chance = 1;
        guard.radar_autonomous_decision_sync.double_weakness_active = 0;
        guard.radar_autonomous_decision.radar_cmd = 0;
    }
    let stop = Arc::new(AtomicBool::new(false));

    let (pub_sock, sub_sock) = make_pair();
    let _handle = zmq_start_sub(
        sub_sock,
        shared.clone(),
        stop.clone(),
        Arc::new(Mutex::new(None)),
    );

    thread::sleep(Duration::from_millis(50));

    let json = make_sdr_json(
        &SdrEnemyRobotBloodData {
            hero_blood: 100,
            ..Default::default()
        },
        &SdrEnemyRobotRemainingAmmoData::default(),
        &SdrEnemyRobotOverallStateData::default(),
        &SdrEnemyRobotGainData::default(),
    );

    // 双倍易伤评估已从 SDR 路径移到串口 0x020E 解析（serial_parser）：
    // SDR 消息到达不应再改动 sync 值或 radar_cmd。
    zmq_send(&pub_sock, &json).unwrap();
    thread::sleep(Duration::from_millis(100));
    {
        let guard = shared.lock().unwrap();
        assert_eq!(
            guard.radar_autonomous_decision_sync.double_weakness_chance, 1,
            "chance untouched by SDR"
        );
        assert_eq!(
            guard.radar_autonomous_decision.radar_cmd, 0,
            "radar_cmd untouched by SDR"
        );
        assert_eq!(
            guard.radar_autonomous_decision.password,
            [1, 2, 3, 4, 5, 6],
            "SDR jamming key injected into 0x0121 decision password"
        );
    }

    stop.store(true, Ordering::Relaxed);
    drop(pub_sock);
    thread::sleep(Duration::from_millis(300));
}

// ─── PUB tests ───

#[test]
fn test_zmq_pub_game_state_format() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    let stop = Arc::new(AtomicBool::new(false));

    // Set test data
    {
        let mut guard = shared.lock().unwrap();
        guard.game_state = GameStateData {
            game_type: 1,
            game_progress: 3,
            stage_remain_time: 420,
            sync_timestamp: 12345678,
        };
        guard.radar_mark_process = RadarMarkProcessData {
            opponent_hero_vulnerable: 1,
            opponent_sentry_vulnerable: 1,
            ally_aerial_targeted: 1,
            ..Default::default()
        };
    }

    let (pub_sock, sub_sock) = make_pair();
    let (tx, rx) = std::sync::mpsc::channel();
    let _handle = zmq_start_pub(vec![pub_sock], shared.clone(), rx, stop.clone());

    thread::sleep(Duration::from_millis(50));

    // Send GameState notification
    tx.send(IDX_GAME_STATE).unwrap();
    thread::sleep(Duration::from_millis(100));

    // Read from sub socket
    let mut buf = zmq2::Message::new();
    sub_sock.recv(&mut buf, 0).unwrap();
    let text = buf.as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).unwrap();

    assert_eq!(parsed["cmd_id"], GAME_STATE_CMD_ID);
    assert_eq!(parsed["game_type"], 1);
    assert_eq!(parsed["game_progress"], 3);
    assert_eq!(parsed["stage_remain_time"], 420);
    assert_eq!(parsed["sync_timestamp"], 12345678);

    stop.store(true, Ordering::Relaxed);
}

#[test]
fn test_zmq_pub_radar_mark_process_format() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    let stop = Arc::new(AtomicBool::new(false));

    {
        let mut guard = shared.lock().unwrap();
        guard.radar_mark_process = RadarMarkProcessData {
            opponent_hero_vulnerable: 1,
            opponent_engineer_vulnerable: 0,
            opponent_infantry_3_vulnerable: 1,
            opponent_infantry_4_vulnerable: 0,
            opponent_aerial_marked: 1,
            opponent_sentry_vulnerable: 1,
            ally_hero_marked: 0,
            ally_engineer_marked: 1,
            ally_infantry_3_marked: 0,
            ally_infantry_4_marked: 1,
            ally_aerial_marked: 0,
            ally_sentry_marked: 1,
            opponent_aerial_targeted: 1,
            opponent_aerial_countered: 0,
            ally_aerial_targeted: 0,
            ally_aerial_countered: 1,
        };
    }

    let (pub_sock, sub_sock) = make_pair();
    let (tx, rx) = std::sync::mpsc::channel();
    let _handle = zmq_start_pub(vec![pub_sock], shared.clone(), rx, stop.clone());

    thread::sleep(Duration::from_millis(50));

    tx.send(IDX_RADAR_MARK_PROCESS).unwrap();
    thread::sleep(Duration::from_millis(100));

    let mut buf = zmq2::Message::new();
    sub_sock.recv(&mut buf, 0).unwrap();
    let text = buf.as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).unwrap();

    assert_eq!(parsed["cmd_id"], RADAR_MARK_PROCESS_CMD_ID);
    assert_eq!(parsed["opponent_hero_vulnerable"], 1);
    assert_eq!(parsed["opponent_sentry_vulnerable"], 1);
    assert_eq!(parsed["ally_aerial_targeted"], 0);
    assert_eq!(parsed["ally_aerial_countered"], 1);
}

#[test]
fn test_zmq_sub_sdr_updates_position() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    let stop = Arc::new(AtomicBool::new(false));

    let (pub_sock, sub_sock) = make_pair();
    let _handle = zmq_start_sub(
        sub_sock,
        shared.clone(),
        stop.clone(),
        Arc::new(Mutex::new(None)),
    );

    thread::sleep(Duration::from_millis(50));

    let json = json!({
        "cmd_id": 0x2002,
        "position": {
            "hero_x": -100, "hero_y": -200,
            "engineer_x": 0, "engineer_y": 0,
            "infantry_3_x": 1, "infantry_3_y": 2,
            "infantry_4_x": 3, "infantry_4_y": 4,
            "aerial_x": 5, "aerial_y": 6,
            "sentry_x": -7, "sentry_y": -8,
        },
        "blood": {
            "hero_blood": 0, "engineer_blood": 0,
            "infantry_3_blood": 0, "infantry_4_blood": 0,
            "reserved": 0, "sentry_blood": 0
        },
        "ammo": {
            "hero_ammo": 0, "infantry_3_ammo": 0,
            "infantry_4_ammo": 0, "aerial_ammo": 0, "sentry_ammo": 0
        },
        "state": {
            "remaining_gold": 0, "total_gold": 0,
            "supply_zone_status": 0, "central_highland_status": 0,
            "trapezoid_highland_status": 0, "fortress_gain_status": 0,
            "outpost_gain_status": 0, "base_gain_status": 0,
            "tunnel_1_status": 0, "tunnel_2_status": 0,
            "tunnel_3_status": 0, "tunnel_4_status": 0,
            "highland_upper_status": 0, "ramp_rear_status": 0,
            "road_upper_status": 0
        },
        "gain": {
            "hero_hp_recovery": 0, "hero_cooling_acceleration": 0,
            "hero_defence": 0, "hero_negative_defence": 0, "hero_attack": 0,
            "engineer_hp_recovery": 0, "engineer_cooling_acceleration": 0,
            "engineer_defence": 0, "engineer_negative_defence": 0, "engineer_attack": 0,
            "infantry_3_hp_recovery": 0, "infantry_3_cooling_acceleration": 0,
            "infantry_3_defence": 0, "infantry_3_negative_defence": 0, "infantry_3_attack": 0,
            "infantry_4_hp_recovery": 0, "infantry_4_cooling_acceleration": 0,
            "infantry_4_defence": 0, "infantry_4_negative_defence": 0, "infantry_4_attack": 0,
            "sentry_hp_recovery": 0, "sentry_cooling_acceleration": 0,
            "sentry_defence": 0, "sentry_negative_defence": 0, "sentry_attack": 0,
            "sentry_posture": 0,
            "hero_state": 0, "engineer_state": 0,
            "infantry_3_state": 0, "infantry_4_state": 0, "sentry_state": 0
        },
        "key": { "key": [0,0,0,0,0,0] },
    })
    .to_string();
    zmq_send(&pub_sock, &json).unwrap();
    thread::sleep(Duration::from_millis(100));

    let guard = shared.lock().unwrap();
    assert_eq!(guard.enemy_hero.x, -100);
    assert_eq!(guard.enemy_hero.y, -200);
    assert_eq!(guard.enemy_aerial.x, 5);
    assert_eq!(guard.enemy_sentry.y, -8);
    drop(guard);

    stop.store(true, Ordering::Relaxed);
    drop(pub_sock);
    thread::sleep(Duration::from_millis(300));
}

#[test]
fn test_zmq_pub_fans_out_to_all_sockets() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    let stop = Arc::new(AtomicBool::new(false));

    {
        let mut guard = shared.lock().unwrap();
        guard.game_state = GameStateData {
            game_type: 2,
            game_progress: 4,
            stage_remain_time: 300,
            sync_timestamp: 999,
        };
    }

    let (pub_a, sub_a) = make_pair_at("inproc://zmq-fanout-a");
    let (pub_b, sub_b) = make_pair_at("inproc://zmq-fanout-b");
    let (tx, rx) = std::sync::mpsc::channel();
    let _handle = zmq_start_pub(vec![pub_a, pub_b], shared.clone(), rx, stop.clone());

    thread::sleep(Duration::from_millis(50));

    tx.send(IDX_GAME_STATE).unwrap();
    thread::sleep(Duration::from_millis(100));

    let mut buf_a = zmq2::Message::new();
    let mut buf_b = zmq2::Message::new();
    sub_a.recv(&mut buf_a, 0).unwrap();
    sub_b.recv(&mut buf_b, 0).unwrap();
    let parsed_a: serde_json::Value = serde_json::from_str(buf_a.as_str().unwrap()).unwrap();
    let parsed_b: serde_json::Value = serde_json::from_str(buf_b.as_str().unwrap()).unwrap();

    assert_eq!(parsed_a["cmd_id"], GAME_STATE_CMD_ID);
    assert_eq!(parsed_b["cmd_id"], GAME_STATE_CMD_ID);
    assert_eq!(parsed_a, parsed_b);
    assert_eq!(parsed_a["game_type"], 2);
    assert_eq!(parsed_a["game_progress"], 4);
    assert_eq!(parsed_a["stage_remain_time"], 300);
    assert_eq!(parsed_a["sync_timestamp"], 999);

    stop.store(true, Ordering::Relaxed);
}

// ─── minimap (0x0305) data source tests ───

#[test]
fn test_zmq_sub_sdr_populates_minimap_opponent() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_slot = Arc::new(Mutex::new(Some(tx)));

    let (pub_sock, sub_sock) = make_pair();
    let _handle = zmq_start_sub(sub_sock, shared.clone(), stop.clone(), tx_slot);

    thread::sleep(Duration::from_millis(50));

    // 含负值（SDR 未检测到=-1）与正常坐标
    let json = json!({
        "cmd_id": 0x2002,
        "position": {
            "hero_x": 636, "hero_y": 578,
            "engineer_x": -1, "engineer_y": -1,
            "infantry_3_x": 1057, "infantry_3_y": 1112,
            "infantry_4_x": 211, "infantry_4_y": 250,
            "aerial_x": 800, "aerial_y": 900,
            "sentry_x": 1200, "sentry_y": 1300,
        },
        "blood": SdrEnemyRobotBloodData::default(),
        "ammo": SdrEnemyRobotRemainingAmmoData::default(),
        "state": SdrEnemyRobotOverallStateData::default(),
        "gain": SdrEnemyRobotGainData::default(),
        "key": { "key": [1, 2, 3, 4, 5, 6] },
    }).to_string();
    zmq_send(&pub_sock, &json).unwrap();
    thread::sleep(Duration::from_millis(100));

    let guard = shared.lock().unwrap();
    assert_eq!(guard.minimap_receive.opponent_hero_x, 636, "opp hero x = SDR 坐标");
    assert_eq!(guard.minimap_receive.opponent_hero_y, 578);
    assert_eq!(guard.minimap_receive.opponent_engineer_x, 0, "负值截断为 0");
    assert_eq!(guard.minimap_receive.opponent_engineer_y, 0);
    assert_eq!(guard.minimap_receive.opponent_infantry_3_x, 1057);
    assert_eq!(guard.minimap_receive.opponent_sentry_y, 1300);
    assert_eq!(guard.minimap_receive.ally_hero_x, 0, "ally 槽位保持 0");
    assert_eq!(guard.minimap_receive.ally_sentry_y, 0, "ally 槽位保持 0");
    drop(guard);

    // SDR 帧触发通知：先 IDX_ROBOT_INTERACTION（0x0200 广播），再限频后 IDX_MINIMAP_RECEIVE_RADAR（0x0305）
    let first = rx.recv_timeout(Duration::from_millis(500));
    assert!(matches!(first, Ok(IDX_ROBOT_INTERACTION)), "应先通知 0x0200 广播");
    let notif = rx.recv_timeout(Duration::from_millis(500));
    assert!(matches!(notif, Ok(IDX_MINIMAP_RECEIVE_RADAR)), "应通知 minimap 发送");

    stop.store(true, Ordering::Relaxed);
    drop(pub_sock);
    thread::sleep(Duration::from_millis(300));
}

#[test]
fn test_zmq_sub_lidar_ignored() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_slot = Arc::new(Mutex::new(Some(tx)));

    let (pub_sock, sub_sock) = make_pair();
    let _handle = zmq_start_sub(sub_sock, shared.clone(), stop.clone(), tx_slot);

    thread::sleep(Duration::from_millis(50));

    // 先发一帧 SDR 填充 minimap opponent，再发 Lidar（定位）帧
    let sdr_json = json!({
        "cmd_id": 0x2002,
        "position": {
            "hero_x": 636, "hero_y": 578,
            "engineer_x": 300, "engineer_y": 400,
            "infantry_3_x": 500, "infantry_3_y": 600,
            "infantry_4_x": 700, "infantry_4_y": 800,
            "aerial_x": 900, "aerial_y": 1000,
            "sentry_x": 1100, "sentry_y": 1200,
        },
        "blood": SdrEnemyRobotBloodData::default(),
        "ammo": SdrEnemyRobotRemainingAmmoData::default(),
        "state": SdrEnemyRobotOverallStateData::default(),
        "gain": SdrEnemyRobotGainData::default(),
        "key": { "key": [1, 2, 3, 4, 5, 6] },
    }).to_string();
    zmq_send(&pub_sock, &sdr_json).unwrap();
    thread::sleep(Duration::from_millis(100));
    while rx.try_recv().is_ok() {}

    zmq_send(&pub_sock, &make_lidar_json()).unwrap();
    thread::sleep(Duration::from_millis(100));

    let guard = shared.lock().unwrap();
    assert_eq!(guard.minimap_receive.opponent_hero_x, 636, "定位不得覆盖 SDR 坐标");
    assert_eq!(guard.minimap_receive.opponent_hero_y, 578);
    assert_eq!(guard.minimap_receive.ally_hero_x, 0, "定位不写 ally 槽位");
    assert_eq!(guard.enemy_hero.x, 636, "定位分支已移除，不再覆盖 enemy_*");
    drop(guard);

    // Lidar 分支已移除：不应产生任何通知（0x0305 仅由 SDR 驱动）
    let notif = rx.recv_timeout(Duration::from_millis(300));
    assert!(
        matches!(notif, Err(std::sync::mpsc::RecvTimeoutError::Timeout)),
        "Lidar 帧不应通知 minimap 发送"
    );

    stop.store(true, Ordering::Relaxed);
    drop(pub_sock);
    thread::sleep(Duration::from_millis(300));
}

#[test]
fn test_zmq_sub_sdr_minimap_notify_rate_limited_5hz() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_slot = Arc::new(Mutex::new(Some(tx)));

    let (pub_sock, sub_sock) = make_pair();
    let _handle = zmq_start_sub(sub_sock, shared.clone(), stop.clone(), tx_slot);

    thread::sleep(Duration::from_millis(50));

    // 连发 10 帧 SDR（模拟 10Hz 信息波）
    let sdr_json = json!({
        "cmd_id": 0x2002,
        "position": {
            "hero_x": 636, "hero_y": 578,
            "engineer_x": 300, "engineer_y": 400,
            "infantry_3_x": 500, "infantry_3_y": 600,
            "infantry_4_x": 700, "infantry_4_y": 800,
            "aerial_x": 900, "aerial_y": 1000,
            "sentry_x": 1100, "sentry_y": 1200,
        },
        "blood": SdrEnemyRobotBloodData::default(),
        "ammo": SdrEnemyRobotRemainingAmmoData::default(),
        "state": SdrEnemyRobotOverallStateData::default(),
        "gain": SdrEnemyRobotGainData::default(),
        "key": { "key": [1, 2, 3, 4, 5, 6] },
    }).to_string();
    for _ in 0..10 {
        zmq_send(&pub_sock, &sdr_json).unwrap();
    }
    thread::sleep(Duration::from_millis(700));

    // 200ms 限频：700ms 窗口内 minimap 通知应 ≤4 次
    let mut minimap_notifs = 0;
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(IDX_MINIMAP_RECEIVE_RADAR) => minimap_notifs += 1,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    assert!(
        (1..=4).contains(&minimap_notifs),
        "5Hz 限频：700ms 内 minimap 通知应 1~4 次，实际 {minimap_notifs}"
    );

    stop.store(true, Ordering::Relaxed);
    drop(pub_sock);
    thread::sleep(Duration::from_millis(300));
}

#[test]
fn test_zmq_sub_sdr_stale_timestamp_skips_minimap_notify() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_slot = Arc::new(Mutex::new(Some(tx)));

    let (pub_sock, sub_sock) = make_pair();
    let _handle = zmq_start_sub(sub_sock, shared.clone(), stop.clone(), tx_slot);

    thread::sleep(Duration::from_millis(50));

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    let base = json!({
        "cmd_id": 0x2002,
        "position": {
            "hero_x": 636, "hero_y": 578,
            "engineer_x": 300, "engineer_y": 400,
            "infantry_3_x": 500, "infantry_3_y": 600,
            "infantry_4_x": 700, "infantry_4_y": 800,
            "aerial_x": 900, "aerial_y": 1000,
            "sentry_x": 1100, "sentry_y": 1200,
        },
        "blood": SdrEnemyRobotBloodData::default(),
        "ammo": SdrEnemyRobotRemainingAmmoData::default(),
        "state": SdrEnemyRobotOverallStateData::default(),
        "gain": SdrEnemyRobotGainData::default(),
        "key": { "key": [1, 2, 3, 4, 5, 6] },
    });

    // 过期数据（timestamp = 10s 前）：坐标仍写入 SharedData，但不通知 0x0305
    let mut stale = base.clone();
    stale["timestamp"] = json!(now - 10.0);
    zmq_send(&pub_sock, &stale.to_string()).unwrap();
    thread::sleep(Duration::from_millis(100));

    let guard = shared.lock().unwrap();
    assert_eq!(guard.minimap_receive.opponent_hero_x, 636, "过期坐标仍写入（数据源本身最新写入）");
    drop(guard);
    while rx.try_recv().is_ok() {}

    let notif = rx.recv_timeout(Duration::from_millis(300));
    assert!(
        matches!(notif, Err(std::sync::mpsc::RecvTimeoutError::Timeout)),
        "过期 SDR 数据不应触发 0x0305 发送"
    );

    // 新鲜数据（timestamp = now）：应触发 minimap 通知
    let mut fresh = base;
    fresh["timestamp"] = json!(now);
    zmq_send(&pub_sock, &fresh.to_string()).unwrap();
    thread::sleep(Duration::from_millis(100));

    let mut saw_minimap = false;
    loop {
        match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(IDX_MINIMAP_RECEIVE_RADAR) => {
                saw_minimap = true;
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    assert!(saw_minimap, "新鲜 SDR 数据应触发 0x0305 发送");

    stop.store(true, Ordering::Relaxed);
    drop(pub_sock);
    thread::sleep(Duration::from_millis(300));
}

#[test]
fn test_zmq_sub_sdr_broadcast_notify_rate_limited_1hz() {
    let shared = Arc::new(Mutex::new(SharedData::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_slot = Arc::new(Mutex::new(Some(tx)));

    let (pub_sock, sub_sock) = make_pair();
    let _handle = zmq_start_sub(sub_sock, shared.clone(), stop.clone(), tx_slot);

    thread::sleep(Duration::from_millis(50));

    let sdr_json = json!({
        "cmd_id": 0x2002,
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64(),
        "position": {
            "hero_x": 636, "hero_y": 578,
            "engineer_x": 300, "engineer_y": 400,
            "infantry_3_x": 500, "infantry_3_y": 600,
            "infantry_4_x": 700, "infantry_4_y": 800,
            "aerial_x": 900, "aerial_y": 1000,
            "sentry_x": 1100, "sentry_y": 1200,
        },
        "blood": SdrEnemyRobotBloodData::default(),
        "ammo": SdrEnemyRobotRemainingAmmoData::default(),
        "state": SdrEnemyRobotOverallStateData::default(),
        "gain": SdrEnemyRobotGainData::default(),
        "key": { "key": [1, 2, 3, 4, 5, 6] },
    }).to_string();
    // 连续发 10 帧（模拟 SDR 10Hz 满速）
    for _ in 0..10 {
        zmq_send(&pub_sock, &sdr_json).unwrap();
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(700));

    let mut broadcast = 0;
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(IDX_ROBOT_INTERACTION) => broadcast += 1,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    assert!(
        (1..=2).contains(&broadcast),
        "1Hz 限频：700ms 内 0x0200 广播通知应 1 次（最多 2），实际 {broadcast}"
    );

    stop.store(true, Ordering::Relaxed);
    drop(pub_sock);
    thread::sleep(Duration::from_millis(300));
}
