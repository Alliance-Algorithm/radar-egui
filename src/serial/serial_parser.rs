use super::serial_crc;
use crate::shared_data::SharedData;
use crate::shared_data::{
    DartLaunchData, GameResultData, GameStateData, RadarAutonomousDecisionSyncData,
    RadarMarkProcessData, SerialFrameHeader, SiteEventData, CMD_ID_LENGTH, CRC16_LENGTH,
    DART_LAUNCH_CMD_ID, FRAME_HEADER_LENGTH, FRAME_HEADER_SOF, GAME_RESULT_CMD_ID,
    GAME_STATE_CMD_ID, IDX_GAME_STATE, IDX_RADAR_AUTONOMOUS_DECISION_SYNC, IDX_RADAR_MARK_PROCESS,
    IDX_ROBOT_INTERACTION_DECISION, RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID, RADAR_MARK_PROCESS_CMD_ID,
    SITE_EVENT_CMD_ID,
};
use deku::prelude::*;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

pub struct SerialParser {
    frame_header: SerialFrameHeader,
    protocol_data: Arc<Mutex<SharedData>>,
    tx: Vec<mpsc::Sender<usize>>,
}

impl SerialParser {
    pub fn new(protocol_data_input: Arc<Mutex<SharedData>>) -> Self {
        SerialParser {
            frame_header: SerialFrameHeader::default(),
            protocol_data: protocol_data_input,
            tx: Vec::new(),
        }
    }

    pub fn new_with_tx(
        protocol_data_input: Arc<Mutex<SharedData>>,
        tx: Vec<mpsc::Sender<usize>>,
    ) -> Self {
        SerialParser {
            frame_header: SerialFrameHeader::default(),
            protocol_data: protocol_data_input,
            tx,
        }
    }
    /// Scan `read_buffer` for complete frames and write parsed data into shared state.
    /// Returns whether at least one frame was successfully parsed.
    pub fn parser<'a>(&mut self, read_buffer: &'a mut Vec<u8>) -> (bool, &'a mut Vec<u8>) {
        let mut parsed_any = false;
        let mut index = 0;
        while index < read_buffer.len() {
            if read_buffer[index] != FRAME_HEADER_SOF {
                index += 1;
                continue;
            }
            let header_end = index + FRAME_HEADER_LENGTH;
            if header_end > read_buffer.len() {
                break;
            }
            if !serial_crc::verify_crc8(&read_buffer[index..header_end]) {
                log::info!(
                    "Serial RX: crc8 mismatch at offset {} ({} bytes), skipping",
                    index,
                    header_end - index
                );
                index += 1;
                continue;
            }
            self.frame_header.sof = read_buffer[index];
            self.frame_header.data_len =
                u16::from_le_bytes([read_buffer[index + 1], read_buffer[index + 2]]);
            self.frame_header.seq = read_buffer[index + 3];
            self.frame_header.crc8 = read_buffer[index + 4];

            let data_len = self.frame_header.data_len as usize;
            let package_start = index;
            let package_end = index + FRAME_HEADER_LENGTH + CMD_ID_LENGTH + data_len + CRC16_LENGTH;
            if package_end > read_buffer.len() {
                break;
            }
            if !serial_crc::verify_crc16(&read_buffer[package_start..package_end]) {
                log::info!(
                    "Serial RX: crc16 mismatch, cmd_id=0x{:04X} data_len={} (dropping frame)",
                    u16::from_le_bytes([read_buffer[index + 5], read_buffer[index + 6]]),
                    data_len
                );
                index += FRAME_HEADER_LENGTH
                    + CMD_ID_LENGTH
                    + self.frame_header.data_len as usize
                    + CRC16_LENGTH;
                continue;
            }

            let cmd_id = u16::from_le_bytes([read_buffer[index + 5], read_buffer[index + 6]]);
            let data_start = index + FRAME_HEADER_LENGTH + CMD_ID_LENGTH;
            let data = &read_buffer[data_start..data_start + data_len];

            match cmd_id {
                GAME_STATE_CMD_ID => {
                    if let Ok((_, v)) = GameStateData::from_bytes((data, 0)) {
                        log::info!("GameState: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap_or_else(|e| {
                            log::error!("SharedData mutex poisoned in serial parser");
                            e.into_inner()
                        });
                        lock.game_state = v;
                        for t in &self.tx {
                            t.send(IDX_GAME_STATE).ok();
                        }
                        parsed_any = true;
                    } else {
                        log::warn!("Serial RX: failed to decode GameState (0x0001)");
                    }
                }
                GAME_RESULT_CMD_ID => {
                    if let Ok((_, v)) = GameResultData::from_bytes((data, 0)) {
                        log::info!("GameResult: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap_or_else(|e| {
                            log::error!("SharedData mutex poisoned in serial parser");
                            e.into_inner()
                        });
                        lock.game_result = v;
                        parsed_any = true;
                    } else {
                        log::warn!("Serial RX: failed to decode GameResult (0x0002)");
                    }
                }
                SITE_EVENT_CMD_ID => {
                    if let Ok((_, v)) = SiteEventData::from_bytes((data, 0)) {
                        log::info!("SiteEvent: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap_or_else(|e| {
                            log::error!("SharedData mutex poisoned in serial parser");
                            e.into_inner()
                        });
                        lock.site_event = v;
                        parsed_any = true;
                    } else {
                        log::warn!("Serial RX: failed to decode SiteEvent (0x0101)");
                    }
                }
                DART_LAUNCH_CMD_ID => {
                    if let Ok((_, v)) = DartLaunchData::from_bytes((data, 0)) {
                        log::info!("DartLaunch: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap_or_else(|e| {
                            log::error!("SharedData mutex poisoned in serial parser");
                            e.into_inner()
                        });
                        lock.dart_launch = v;
                        parsed_any = true;
                    } else {
                        log::warn!("Serial RX: failed to decode DartLaunch (0x0105)");
                    }
                }
                RADAR_MARK_PROCESS_CMD_ID => {
                    if let Ok((_, v)) = RadarMarkProcessData::from_bytes((data, 0)) {
                        log::info!("RadarMarkProcess: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap_or_else(|e| {
                            log::error!("SharedData mutex poisoned in serial parser");
                            e.into_inner()
                        });
                        lock.radar_mark_process = v;
                        for t in &self.tx {
                            t.send(IDX_RADAR_MARK_PROCESS).ok();
                        }
                        parsed_any = true;
                    } else {
                        log::warn!("Serial RX: failed to decode RadarMarkProcess (0x020C)");
                    }
                }
                RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID => {
                    if let Ok((_, v)) = RadarAutonomousDecisionSyncData::from_bytes((data, 0)) {
                        log::info!("RadarAutonomousDecisionSync: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap_or_else(|e| {
                            log::error!("SharedData mutex poisoned in serial parser");
                            e.into_inner()
                        });
                        lock.radar_autonomous_decision_sync = v;
                        for t in &self.tx {
                            t.send(IDX_RADAR_AUTONOMOUS_DECISION_SYNC).ok();
                        }
                        // 双倍易伤自主决策：0x020E 到达即评估并触发 0x0121（不依赖 SDR 链路）。
                        // active==1 时累加 radar_cmd（消耗机会）；active==0 不累加但 0x0121 照发（key 照常传输）。
                        let progress = lock.game_state.game_progress;
                        let chance = lock.radar_autonomous_decision_sync.double_weakness_chance;
                        let active = lock.radar_autonomous_decision_sync.double_weakness_active;
                        if progress == 5 {
                            // 结算：本地机会数清零
                            lock.radar_autonomous_decision_sync.double_weakness_chance = 0;
                            log::info!(
                                "Serial RX decision eval: game_progress=5 (settlement), double_weakness_chance reset to 0"
                            );
                        } else if progress == 4 {
                            if active == 1 {
                                // 仅 active==1（正在被触发）时允许累加：chance==0 只发 0；
                                // chance>=1 单调 +1（消耗一个机会），radar_cmd 上限 2。
                                if chance == 0 {
                                    lock.radar_autonomous_decision.radar_cmd = 0;
                                } else if lock.radar_autonomous_decision.radar_cmd < 2 {
                                    lock.radar_autonomous_decision.radar_cmd = lock
                                        .radar_autonomous_decision
                                        .radar_cmd
                                        .saturating_add(1);
                                }
                            }
                            log::info!(
                                "Serial RX decision eval: game_progress=4 active={} chance={} radar_cmd={} (0x0121 sent)",
                                active,
                                chance,
                                lock.radar_autonomous_decision.radar_cmd,
                            );
                            for t in &self.tx {
                                t.send(IDX_ROBOT_INTERACTION_DECISION).ok();
                            }
                        } else {
                            log::info!(
                                "Serial RX decision eval: game_progress={} active={} chance={} radar_cmd={} (no trigger)",
                                progress,
                                active,
                                chance,
                                lock.radar_autonomous_decision.radar_cmd,
                            );
                        }
                        parsed_any = true;
                    } else {
                        log::warn!("Serial RX: failed to decode RadarAutonomousDecisionSync (0x020E)");
                    }
                }
                _ => {
                    log::warn!("Serial RX: unknown cmd_id 0x{:04X}", cmd_id);
                }
            }
            index = package_end;
        }
        read_buffer.drain(0..index);
        (parsed_any, read_buffer)
    }
}
