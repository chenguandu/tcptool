/// JT808 message builder
/// Convenient functions to build complete JT808 frames ready to send

use crate::protocol::codec;
use crate::protocol::message::Jt808Message;
use crate::protocol::types::{BodyProps, MsgHeader};

/// Build a complete JT808 frame given message ID, terminal phone, serial number, and body bytes
pub fn build_frame(msg_id: u16, terminal_phone: &str, serial_no: u16, body: &[u8]) -> Vec<u8> {
    let header = MsgHeader {
        msg_id,
        body_props: BodyProps {
            body_len: body.len() as u16,
            encryption: 0,
            has_subpackage: false,
            is_response: false,
        },
        terminal_id: terminal_phone.to_string(),
        serial_no,
        subpackage: None,
    };
    let mut hb = header.encode();
    hb.extend_from_slice(body);
    codec::encode_message(&hb)
}

/// Build a JT808 heartbeat frame (0x0002)
pub fn build_heartbeat(terminal_phone: &str, serial_no: u16) -> Vec<u8> {
    build_frame(0x0002, terminal_phone, serial_no, &[])
}

/// Build a JT808 terminal register frame (0x0100)
pub fn build_register(
    terminal_phone: &str,
    serial_no: u16,
    province_id: u16,
    city_id: u16,
    manufacturer_id: &str,
    terminal_model: &str,
    terminal_id: &str,
    color: u8,
    plate_number: &str,
) -> Vec<u8> {
    use crate::protocol::message::TerminalRegister;
    let reg = TerminalRegister {
        province_id,
        city_id,
        manufacturer_id: manufacturer_id.to_string(),
        terminal_model: terminal_model.to_string(),
        terminal_id: terminal_id.to_string(),
        color,
        plate_number: plate_number.to_string(),
    };
    build_frame(0x0100, terminal_phone, serial_no, &reg.encode())
}

/// Build a JT808 terminal auth frame (0x0102)
pub fn build_auth(terminal_phone: &str, serial_no: u16, auth_code: &str) -> Vec<u8> {
    use crate::protocol::message::TerminalAuth;
    let auth = TerminalAuth {
        auth_code: auth_code.to_string(),
    };
    build_frame(0x0102, terminal_phone, serial_no, &auth.encode())
}

/// Build a JT808 location report frame (0x0200)
pub fn build_location_report(
    terminal_phone: &str,
    serial_no: u16,
    latitude: u32,
    longitude: u32,
    altitude: u16,
    speed: u16,
    direction: u16,
) -> Vec<u8> {
    use crate::protocol::message::LocationReport;
    let now = chrono::Local::now();
    let timestamp = now.format("%y%m%d%H%M%S").to_string();
    let report = LocationReport {
        alarm_flags: 0,
        status: 0,
        latitude,
        longitude,
        altitude,
        speed,
        direction,
        timestamp,
        extra_items: Vec::new(),
    };
    build_frame(0x0200, terminal_phone, serial_no, &report.encode())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_heartbeat() {
        let frame = build_heartbeat("013900000001", 1);
        assert!(frame.len() > 6);
        assert_eq!(frame[0], 0x7E);
        assert_eq!(*frame.last().unwrap(), 0x7E);
    }

    #[test]
    fn test_build_register() {
        let frame = build_register("013900000001", 1, 31, 1, "MFG01", "MODEL01", "TID0001", 1, "京A88888");
        assert_eq!(frame[0], 0x7E);
        assert_eq!(*frame.last().unwrap(), 0x7E);
    }

    #[test]
    fn test_build_auth() {
        let frame = build_auth("013900000001", 1, "AUTH1234");
        assert_eq!(frame[0], 0x7E);
        assert_eq!(*frame.last().unwrap(), 0x7E);
    }

    #[test]
    fn test_decode_heartbeat() {
        let frame = build_heartbeat("013900000001", 1);
        let (decoded, _) = codec::decode_message(&frame).unwrap();
        assert!(decoded.len() >= 12);
        let msg_id = u16::from_be_bytes([decoded[0], decoded[1]]);
        assert_eq!(msg_id, 0x0002);
    }
}
