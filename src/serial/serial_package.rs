use super::serial_crc;
use crate::shared_data::{SerialFrame, SerialFrameHeader};
use deku::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering};
static PACKET_SEQ: AtomicU8 = AtomicU8::new(0);
/// Build a complete DJI serial frame with auto-incrementing sequence number,
/// CRC8 header check, and CRC16 frame check.
pub fn serial_package(cmd_id: u16, data: Vec<u8>) -> SerialFrame {
    let seq = PACKET_SEQ.fetch_add(1, Ordering::SeqCst);
    let mut frame_header: SerialFrameHeader = SerialFrameHeader {
        sof: 0xA5,
        data_len: data.len() as u16,
        seq: seq,
        crc8: 0,
    };
    frame_header.crc8 = {
        let mut header_bytes = frame_header.to_bytes().unwrap();
        serial_crc::append_crc8(&mut header_bytes).unwrap_or_default()
    };
    let mut package = SerialFrame {
        frame_header,
        cmd_id,
        data,
        crc16: 0,
    };
    package.crc16 = {
        let mut package_bytes = package.to_bytes().unwrap();
        serial_crc::append_crc16(&mut package_bytes).unwrap_or_default()
    };
    package
}
