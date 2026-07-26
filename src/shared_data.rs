use deku::prelude::*;
use serde::{Deserialize, Serialize};

use crate::robot_interaction_id::DeviceId;

// ─── DJI protocol constants ───

pub const FRAME_HEADER_SOF: u8 = 0xA5;
pub const FRAME_HEADER_LENGTH: usize = 5;
pub const CMD_ID_LENGTH: usize = 2;
pub const CRC8_LENGTH: usize = 1;
pub const CRC16_LENGTH: usize = 2;
pub const GAME_STATE_CMD_ID: u16 = 0x0001;
pub const GAME_RESULT_CMD_ID: u16 = 0x0002;
pub const SITE_EVENT_CMD_ID: u16 = 0x0101;
pub const DART_LAUNCH_CMD_ID: u16 = 0x0105;
pub const RADAR_MARK_PROCESS_CMD_ID: u16 = 0x020C;
pub const RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID: u16 = 0x020E;
pub const ROBOT_INTERACTION_CMD_ID: u16 = 0x0301;
pub const RADAR_AUTONOMOUS_DECISION_DATA_CMD_ID: u16 = 0x0121;
pub const RADAR_LOCAL_COMPUTATION_CMD_ID: u16 = 0x0122;
pub const MINIMAP_RECEIVE_RADAR_CMD_ID: u16 = 0x0305;
pub const SDR_ENEMY_ROBOT_POSITION_CMD_ID: u16 = 0x0A01;
pub const SDR_ENEMY_ROBOT_BLOOD_CMD_ID: u16 = 0x0A02;
pub const SDR_ENEMY_ROBOT_REMAINING_AMMO_CMD_ID: u16 = 0x0A03;
pub const SDR_ENEMY_ROBOT_OVERALL_STATE_CMD_ID: u16 = 0x0A04;
pub const SDR_ENEMY_ROBOT_GAIN_CMD_ID: u16 = 0x0A05;
pub const SDR_JAMMING_KEY_CMD_ID: u16 = 0x0A06;

pub const GAME_STATE_DATA_LEN: usize = 11;
pub const GAME_RESULT_DATA_LEN: usize = 1;
pub const SITE_EVENT_DATA_LEN: usize = 4;
pub const DART_LAUNCH_DATA_LEN: usize = 3;
pub const RADAR_MARK_PROCESS_DATA_LEN: usize = 2;
pub const RADAR_AUTONOMOUS_DECISION_SYNC_DATA_LEN: usize = 1;
pub const ROBOT_INTERACTION_DATA_LEN: usize = 118;
pub const MINIMAP_RECEIVE_RADAR_DATA_LEN: usize = 48;
pub const SDR_ENEMY_ROBOT_POSITION_DATA_LEN: usize = 24;
pub const SDR_ENEMY_ROBOT_BLOOD_DATA_LEN: usize = 12;
pub const SDR_ENEMY_ROBOT_REMAINING_AMMO_DATA_LEN: usize = 10;
pub const SDR_ENEMY_ROBOT_OVERALL_STATE_DATA_LEN: usize = 8;
pub const SDR_ENEMY_ROBOT_GAIN_DATA_LEN: usize = 41;
pub const SDR_JAMMING_KEY_DATA_LEN: usize = 6;

// ─── DJI protocol frame types (deku) ───

#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little")]
pub struct SerialFrameHeader {
    pub sof: u8,
    pub data_len: u16,
    pub seq: u8,
    pub crc8: u8,
}

#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
pub struct SerialFrame {
    pub frame_header: SerialFrameHeader,
    pub cmd_id: u16,
    #[deku(count = "frame_header.data_len as usize")]
    pub data: Vec<u8>,
    #[deku(endian = "little")]
    pub crc16: u16,
}

// cmd_id = 0x0001
#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little", bit_order = "lsb")]
pub struct GameStateData {
    #[deku(bits = "4")]
    pub game_type: u8,
    #[deku(bits = "4")]
    pub game_progress: u8,
    pub stage_remain_time: u16,
    pub sync_timestamp: u64,
}

// cmd_id = 0x0002
#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little", bit_order = "lsb")]
pub struct GameResultData {
    pub winner: u8,
}

// cmd_id = 0x0101
#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little", bit_order = "lsb")]
pub struct SiteEventData {
    #[deku(bits = "3")]
    pub supply_zone_status: u8,
    #[deku(bits = "2")]
    pub energy_small_status: u8,
    #[deku(bits = "2")]
    pub energy_large_status: u8,
    #[deku(bits = "2")]
    pub central_highland_status: u8,
    #[deku(bits = "2")]
    pub trapezoid_highland_status: u8,
    #[deku(bits = "9")]
    pub dart_hit_time: u16,
    #[deku(bits = "3")]
    pub dart_hit_target: u8,
    #[deku(bits = "2")]
    pub center_gain_status: u8,
    #[deku(bits = "2")]
    pub fortress_gain_status: u8,
    #[deku(bits = "2")]
    pub outpost_gain_status: u8,
    #[deku(bits = "1", pad_bits_after = "2")]
    pub base_gain_status: u8,
}

// cmd_id = 0x0105
#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little", bit_order = "lsb")]
pub struct DartLaunchData {
    pub dart_remaining_time: u8,
    #[deku(bits = "3")]
    pub dart_hit_target: u8,
    #[deku(bits = "3")]
    pub dart_hit_count: u8,
    #[deku(bits = "3", pad_bits_after = "7")]
    pub dart_selected_target: u8,
}

// cmd_id = 0x020C
#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little", bit_order = "lsb")]
pub struct RadarMarkProcessData {
    #[deku(bits = "1")]
    pub opponent_hero_vulnerable: u8,
    #[deku(bits = "1")]
    pub opponent_engineer_vulnerable: u8,
    #[deku(bits = "1")]
    pub opponent_infantry_3_vulnerable: u8,
    #[deku(bits = "1")]
    pub opponent_infantry_4_vulnerable: u8,
    #[deku(bits = "1")]
    pub opponent_aerial_marked: u8,
    #[deku(bits = "1")]
    pub opponent_sentry_vulnerable: u8,
    #[deku(bits = "1")]
    pub ally_hero_marked: u8,
    #[deku(bits = "1")]
    pub ally_engineer_marked: u8,
    #[deku(bits = "1")]
    pub ally_infantry_3_marked: u8,
    #[deku(bits = "1")]
    pub ally_infantry_4_marked: u8,
    #[deku(bits = "1")]
    pub ally_aerial_marked: u8,
    #[deku(bits = "1")]
    pub ally_sentry_marked: u8,
    #[deku(bits = "1")]
    pub opponent_aerial_targeted: u8,
    #[deku(bits = "1")]
    pub opponent_aerial_countered: u8,
    #[deku(bits = "1")]
    pub ally_aerial_targeted: u8,
    #[deku(bits = "1")]
    pub ally_aerial_countered: u8,
}

// cmd_id = 0x020E
#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little", bit_order = "lsb")]
pub struct RadarAutonomousDecisionSyncData {
    #[deku(bits = "2")]
    pub double_weakness_chance: u8,
    #[deku(bits = "1")]
    pub double_weakness_active: u8,
    #[deku(bits = "2")]
    pub encryption_rank: u8,
    #[deku(bits = "1", pad_bits_after = "2")]
    pub key_modifiable: u8,
}

// cmd_id = 0x0301
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotInteractionData {
    pub subcontext_cmd_id: u16,
    pub sender_id: DeviceId,
    pub receiver_id: DeviceId,
    pub subcontext_data: Vec<u8>,
}

impl Default for RobotInteractionData {
    fn default() -> Self {
        Self {
            subcontext_cmd_id: 0,
            sender_id: DeviceId::Unknown,
            receiver_id: DeviceId::Unknown,
            subcontext_data: Vec::new(),
        }
    }
}

impl RobotInteractionData {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = self.subcontext_cmd_id.to_le_bytes().to_vec();
        let sid: u16 = self.sender_id.into();
        bytes.extend_from_slice(&sid.to_le_bytes());
        let rid: u16 = self.receiver_id.into();
        bytes.extend_from_slice(&rid.to_le_bytes());
        bytes.extend_from_slice(&self.subcontext_data);
        bytes
    }
}

// sub-content cmd_id = 0x0121
#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little")]
pub struct RadarAutonomousDecisionData {
    pub radar_cmd: u8,
    pub password_cmd: u8,
    pub password: [u8; 6],
}

// cmd_id = 0x0305
#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little")]
pub struct MinimapReceiveRadarData {
    pub opponent_hero_x: u16,
    pub opponent_hero_y: u16,
    pub opponent_engineer_x: u16,
    pub opponent_engineer_y: u16,
    pub opponent_infantry_3_x: u16,
    pub opponent_infantry_3_y: u16,
    pub opponent_infantry_4_x: u16,
    pub opponent_infantry_4_y: u16,
    pub opponent_aerial_x: u16,
    pub opponent_aerial_y: u16,
    pub opponent_sentry_x: u16,
    pub opponent_sentry_y: u16,
    pub ally_hero_x: u16,
    pub ally_hero_y: u16,
    pub ally_engineer_x: u16,
    pub ally_engineer_y: u16,
    pub ally_infantry_3_x: u16,
    pub ally_infantry_3_y: u16,
    pub ally_infantry_4_x: u16,
    pub ally_infantry_4_y: u16,
    pub ally_aerial_x: u16,
    pub ally_aerial_y: u16,
    pub ally_sentry_x: u16,
    pub ally_sentry_y: u16,
}

// ─── SDR wireless link (0x0A01–0x0A06) ───

#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little")]
pub struct SdrEnemyRobotPositionData {
    pub hero_x: i16,
    pub hero_y: i16,
    pub engineer_x: i16,
    pub engineer_y: i16,
    pub infantry_3_x: i16,
    pub infantry_3_y: i16,
    pub infantry_4_x: i16,
    pub infantry_4_y: i16,
    pub aerial_x: i16,
    pub aerial_y: i16,
    pub sentry_x: i16,
    pub sentry_y: i16,
}

#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little")]
pub struct SdrEnemyRobotBloodData {
    pub hero_blood: u16,
    pub engineer_blood: u16,
    pub infantry_3_blood: u16,
    pub infantry_4_blood: u16,
    pub reserved: u16,
    pub sentry_blood: u16,
}

#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little")]
pub struct SdrEnemyRobotRemainingAmmoData {
    pub hero_ammo: u16,
    pub infantry_3_ammo: u16,
    pub infantry_4_ammo: u16,
    pub aerial_ammo: u16,
    pub sentry_ammo: u16,
}

#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little", bit_order = "lsb")]
pub struct SdrEnemyRobotOverallStateData {
    pub remaining_gold: u16,
    pub total_gold: u16,
    #[deku(bits = "1")]
    pub supply_zone_status: u8,
    #[deku(bits = "2")]
    pub central_highland_status: u8,
    #[deku(bits = "1")]
    pub trapezoid_highland_status: u8,
    #[deku(bits = "2")]
    pub fortress_gain_status: u8,
    #[deku(bits = "2")]
    pub outpost_gain_status: u8,
    #[deku(bits = "1")]
    pub base_gain_status: u8,
    #[deku(bits = "1")]
    pub tunnel_1_status: u8,
    #[deku(bits = "1")]
    pub tunnel_2_status: u8,
    #[deku(bits = "1")]
    pub tunnel_3_status: u8,
    #[deku(bits = "1")]
    pub tunnel_4_status: u8,
    #[deku(bits = "1")]
    pub highland_upper_status: u8,
    #[deku(bits = "1")]
    pub ramp_rear_status: u8,
    #[deku(bits = "1", pad_bits_after = "16")]
    pub road_upper_status: u8,
}

#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little")]
pub struct SdrEnemyRobotGainData {
    pub hero_hp_recovery: u8,
    pub hero_cooling_acceleration: u16,
    pub hero_defence: u8,
    pub hero_negative_defence: u8,
    pub hero_attack: u16,
    pub engineer_hp_recovery: u8,
    pub engineer_cooling_acceleration: u16,
    pub engineer_defence: u8,
    pub engineer_negative_defence: u8,
    pub engineer_attack: u16,
    pub infantry_3_hp_recovery: u8,
    pub infantry_3_cooling_acceleration: u16,
    pub infantry_3_defence: u8,
    pub infantry_3_negative_defence: u8,
    pub infantry_3_attack: u16,
    pub infantry_4_hp_recovery: u8,
    pub infantry_4_cooling_acceleration: u16,
    pub infantry_4_defence: u8,
    pub infantry_4_negative_defence: u8,
    pub infantry_4_attack: u16,
    pub sentry_hp_recovery: u8,
    pub sentry_cooling_acceleration: u16,
    pub sentry_defence: u8,
    pub sentry_negative_defence: u8,
    pub sentry_attack: u16,
    pub sentry_posture: u8,
    pub hero_state: u8,
    pub engineer_state: u8,
    pub infantry_3_state: u8,
    pub infantry_4_state: u8,
    pub sentry_state: u8,
}

#[derive(Debug, Clone, Default, DekuRead, DekuWrite, Serialize, Deserialize)]
#[deku(endian = "little")]
pub struct SdrJammingKeyData {
    pub key: [u8; 6],
}
// ─── Radar local computation data (0x0301 sub-cmd 0x0122) ───
// Payload layout: 10 (ammo) + 8 (economy) + 41 (gain+states) + 1 (drone) = 60 bytes
// Sent via radar → 0x0301 robot interaction → 0x0310 custom client

#[derive(Debug, Clone, Default)]
pub struct RadarLocalComputationData {
    /// [0..10]  enemy robot allowed ammo (5 robots × u16 LE)
    pub ammo: SdrEnemyRobotRemainingAmmoData,
    /// [10..18) enemy economy + site occupation status
    pub economy: SdrEnemyRobotOverallStateData,
    /// [18..59) enemy robot gain buffs + postures + alive states
    pub gain: SdrEnemyRobotGainData,
    /// [59]     enemy drone counter-progress (percentage, 0-100)
    pub drone_counter_progress: u8,
}

impl RadarLocalComputationData {
    /// Parse from 60-byte subcontext_data slice.
    pub fn from_slice(data: &[u8]) -> Option<Self> {
        if data.len() < 60 {
            return None;
        }
        let (_, ammo) = SdrEnemyRobotRemainingAmmoData::from_bytes((&data[0..10], 0)).ok()?;
        let (_, economy) = SdrEnemyRobotOverallStateData::from_bytes((&data[10..18], 0)).ok()?;
        let (_, gain) = SdrEnemyRobotGainData::from_bytes((&data[18..59], 0)).ok()?;
        Some(Self {
            ammo,
            economy,
            gain,
            drone_counter_progress: data[59],
        })
    }

    /// Serialize to 60-byte Vec for transmission.
    pub fn to_bytes(&self) -> Result<Vec<u8>, DekuError> {
        let mut bytes = self.ammo.to_bytes()?;
        bytes.extend(self.economy.to_bytes()?);
        bytes.extend(self.gain.to_bytes()?);
        bytes.push(self.drone_counter_progress);
        Ok(bytes)
    }
}

// ─── Channel notification indices ───

pub const IDX_GAME_STATE: usize = 0;
pub const IDX_GAME_RESULT: usize = 1;
pub const IDX_SITE_EVENT: usize = 2;
pub const IDX_DART_LAUNCH: usize = 3;
pub const IDX_RADAR_MARK_PROCESS: usize = 4;
pub const IDX_RADAR_AUTONOMOUS_DECISION_SYNC: usize = 5;
pub const IDX_ROBOT_INTERACTION: usize = 6;

// ─── Shared runtime state ───

#[derive(Debug, Clone, Default)]
pub struct Position {
    pub x: i16,
    pub y: i16,
}

#[derive(Debug, Clone, Default)]
pub struct SharedData {
    pub enemy_hero: Position,
    pub enemy_engineer: Position,
    pub enemy_infantry_3: Position,
    pub enemy_infantry_4: Position,
    pub enemy_aerial: Position,
    pub enemy_sentry: Position,
    pub ally_hero: Position,
    pub ally_engineer: Position,
    pub ally_infantry_3: Position,
    pub ally_infantry_4: Position,
    pub ally_aerial: Position,
    pub ally_sentry: Position,

    pub sdr_blood: SdrEnemyRobotBloodData,
    pub sdr_ammo: SdrEnemyRobotRemainingAmmoData,
    pub sdr_state: SdrEnemyRobotOverallStateData,
    pub sdr_gain: SdrEnemyRobotGainData,
    pub sdr_jamming_key: SdrJammingKeyData,

    pub game_state: GameStateData,
    pub game_result: GameResultData,
    pub site_event: SiteEventData,
    pub dart_launch: DartLaunchData,
    pub radar_mark_process: RadarMarkProcessData,
    pub radar_autonomous_decision_sync: RadarAutonomousDecisionSyncData,
    pub robot_interaction: RobotInteractionData,

    pub radar_autonomous_decision: RadarAutonomousDecisionData,
    pub minimap_receive: MinimapReceiveRadarData,
    pub radar_local_computation: RadarLocalComputationData,
}
