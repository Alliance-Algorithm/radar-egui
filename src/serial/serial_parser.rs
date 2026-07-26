use crate::robot_interaction_id::DeviceId;
use super::serial_crc;
use crate::shared_data::{
    self as data_format, CMD_ID_LENGTH, CRC16_LENGTH, DART_LAUNCH_CMD_ID, FRAME_HEADER_LENGTH,
    FRAME_HEADER_SOF, GAME_RESULT_CMD_ID, GAME_STATE_CMD_ID, IDX_DART_LAUNCH, IDX_GAME_RESULT,
    IDX_GAME_STATE, IDX_RADAR_AUTONOMOUS_DECISION_SYNC, IDX_RADAR_MARK_PROCESS,
    IDX_ROBOT_INTERACTION, IDX_SITE_EVENT, RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID,
    RADAR_MARK_PROCESS_CMD_ID, ROBOT_INTERACTION_CMD_ID, SITE_EVENT_CMD_ID,
};
use crate::shared_data::SharedData;
use deku::prelude::*;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

pub struct SerialParser {
    frame_header: data_format::SerialFrameHeader,
    protocol_data: Arc<Mutex<SharedData>>,
    tx: Option<mpsc::Sender<usize>>,
}

impl SerialParser {
    pub fn new(protocol_data_input: Arc<Mutex<SharedData>>) -> Self {
        SerialParser {
            frame_header: data_format::SerialFrameHeader::default(),
            protocol_data: protocol_data_input,
            tx: None,
        }
    }

    pub fn new_with_tx(
        protocol_data_input: Arc<Mutex<SharedData>>,
        tx: mpsc::Sender<usize>,
    ) -> Self {
        SerialParser {
            frame_header: data_format::SerialFrameHeader::default(),
            protocol_data: protocol_data_input,
            tx: Some(tx),
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
            self.frame_header.frame_header_sof = read_buffer[index];
            self.frame_header.frame_header_data_len =
                u16::from_le_bytes([read_buffer[index + 1], read_buffer[index + 2]]);
            self.frame_header.frame_header_seq = read_buffer[index + 3];
            self.frame_header.frame_header_crc8 = read_buffer[index + 4];

            let data_len = self.frame_header.frame_header_data_len as usize;
            let package_start = index;
            let package_end = index + FRAME_HEADER_LENGTH + CMD_ID_LENGTH + data_len + CRC16_LENGTH;
            if package_end > read_buffer.len() {
                break;
            }
            if !serial_crc::verify_crc16(&read_buffer[package_start..package_end]) {
                index += FRAME_HEADER_LENGTH
                    + CMD_ID_LENGTH
                    + self.frame_header.frame_header_data_len as usize
                    + CRC16_LENGTH;
                continue;
            }

            let cmd_id = u16::from_le_bytes([read_buffer[index + 5], read_buffer[index + 6]]);
            let data_start = index + FRAME_HEADER_LENGTH + CMD_ID_LENGTH;
            let data = &read_buffer[data_start..data_start + data_len];

            match cmd_id {
                GAME_STATE_CMD_ID => {
                    if let Ok((_, v)) = data_format::GameStateData::from_bytes((data, 0)) {
                        log::info!("GameState: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap();
                        lock.game_state = v;
                        if let Some(ref tx) = self.tx { tx.send(IDX_GAME_STATE).ok(); }
                        parsed_any = true;
                    }
                }
                GAME_RESULT_CMD_ID => {
                    if let Ok((_, v)) = data_format::GameResultData::from_bytes((data, 0)) {
                        log::info!("GameResult: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap();
                        lock.game_result = v;
                        if let Some(ref tx) = self.tx { tx.send(IDX_GAME_RESULT).ok(); }
                        parsed_any = true;
                    }
                }
                SITE_EVENT_CMD_ID => {
                    if let Ok((_, v)) = data_format::SiteEventData::from_bytes((data, 0)) {
                        log::info!("SiteEvent: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap();
                        lock.site_event = v;
                        if let Some(ref tx) = self.tx { tx.send(IDX_SITE_EVENT).ok(); }
                        parsed_any = true;
                    }
                }
                DART_LAUNCH_CMD_ID => {
                    if let Ok((_, v)) = data_format::DartLaunchData::from_bytes((data, 0)) {
                        log::info!("DartLaunch: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap();
                        lock.dart_launch = v;
                        if let Some(ref tx) = self.tx { tx.send(IDX_DART_LAUNCH).ok(); }
                        parsed_any = true;
                    }
                }
                RADAR_MARK_PROCESS_CMD_ID => {
                    if let Ok((_, v)) = data_format::RadarMarkProcessData::from_bytes((data, 0)) {
                        log::info!("RadarMarkProcess: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap();
                        lock.radar_mark_process = v;
                        if let Some(ref tx) = self.tx { tx.send(IDX_RADAR_MARK_PROCESS).ok(); }
                        parsed_any = true;
                    }
                }
                RADAR_AUTONOMOUS_DECISION_SYNC_CMD_ID => {
                    if let Ok((_, v)) =
                        data_format::RadarAutonomousDecisionSyncData::from_bytes((data, 0))
                    {
                        log::info!("RadarAutonomousDecisionSync: {:?}", v);
                        let mut lock = self.protocol_data.lock().unwrap();
                        lock.radar_decision_sync = v;
                        if let Some(ref tx) = self.tx { tx.send(IDX_RADAR_AUTONOMOUS_DECISION_SYNC).ok(); }
                        parsed_any = true;
                    }
                }
                ROBOT_INTERACTION_CMD_ID => {
                    if data.len() >= 6 {
                        let sub_cmd = u16::from_le_bytes([data[0], data[1]]);
                        let sender = DeviceId::from(u16::from_le_bytes([data[2], data[3]]));
                        let receiver = DeviceId::from(u16::from_le_bytes([data[4], data[5]]));
                        log::info!(
                            "RobotInteraction: sub_cmd=0x{:04X} sender={:?} receiver={:?} sub_data_len={}",
                            sub_cmd, sender, receiver, data.len() - 6
                        );
                        let mut lock = self.protocol_data.lock().unwrap();
                        lock.robot_interaction = data_format::RobotInteractionData {
                            subcontext_cmd_id: sub_cmd,
                            sender_id: sender,
                            receiver_id: receiver,
                            subcontext_data: data[6..].to_vec(),
                        };
                        if let Some(ref tx) = self.tx { tx.send(IDX_ROBOT_INTERACTION).ok(); }
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
