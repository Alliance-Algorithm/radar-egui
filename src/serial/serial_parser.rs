use super::serial_crc;
use crate::shared_data::SharedData;
use crate::shared_data::{
    DartLaunchData, GameResultData, GameStateData, RadarAutonomousDecisionSyncData,
    RadarMarkProcessData, SerialFrameHeader, SiteEventData, CMD_ID_LENGTH, CRC16_LENGTH,
    DART_LAUNCH_CMD_ID, FRAME_HEADER_LENGTH, FRAME_HEADER_SOF, GAME_RESULT_CMD_ID,
    GAME_STATE_CMD_ID, IDX_GAME_STATE, IDX_RADAR_MARK_PROCESS,
    RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID, RADAR_MARK_PROCESS_CMD_ID, SITE_EVENT_CMD_ID,
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
                        parsed_any = true;
                    }
                }
                _ => {}
            }
            index = package_end;
        }
        read_buffer.drain(0..index);
        (parsed_any, read_buffer)
    }
}
