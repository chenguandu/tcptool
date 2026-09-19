/// JT808 message builder
/// Convenient functions to build complete JT808 frames ready to send
/// (supports JT/T 808-2013 and JT/T 808-2019)

use crate::protocol::codec;
use crate::protocol::message::Jt808Message;
use crate::protocol::types::{BodyProps, MsgHeader, ProtocolVersion};

/// Build a complete JT808 frame given protocol version, message ID,
/// terminal phone, serial number, and body bytes
pub fn build_frame(
    version: ProtocolVersion,
    msg_id: u16,
    terminal_phone: &str,
    serial_no: u16,
    body: &[u8],
) -> Vec<u8> {
    let header = MsgHeader {
        msg_id,
        body_props: BodyProps {
            body_len: body.len() as u16,
            encryption: 0,
            has_subpackage: false,
        },
        terminal_id: terminal_phone.to_string(),
        serial_no,
        subpackage: None,
        version: version.version_number(),
    };
    let mut hb = header.encode();
    hb.extend_from_slice(body);
    codec::encode_message(&hb)
}

/// Build a JT808 heartbeat frame (0x0002)
pub fn build_heartbeat(version: ProtocolVersion, terminal_phone: &str, serial_no: u16) -> Vec<u8> {
    build_frame(version, 0x0002, terminal_phone, serial_no, &[])
}

/// Build a JT808 terminal register frame (0x0100)
#[allow(clippy::too_many_arguments)]
pub fn build_register(
    version: ProtocolVersion,
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
    build_frame(version, 0x0100, terminal_phone, serial_no, &reg.encode(version))
}

/// Build a JT808 terminal auth frame (0x0102)
/// 2013: 鉴权码 STRING；2019: 鉴权码长度 + 鉴权码 + IMEI(15) + 软件版本号(20)
pub fn build_auth(
    version: ProtocolVersion,
    terminal_phone: &str,
    serial_no: u16,
    auth_code: &str,
    imei: &str,
    software_version: &str,
) -> Vec<u8> {
    use crate::protocol::message::TerminalAuth;
    let auth = TerminalAuth {
        auth_code: auth_code.to_string(),
        imei: imei.to_string(),
        software_version: software_version.to_string(),
    };
    build_frame(version, 0x0102, terminal_phone, serial_no, &auth.encode(version))
}

/// Build a JT808 location report frame (0x0200)
#[allow(clippy::too_many_arguments)]
pub fn build_location_report(
    version: ProtocolVersion,
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
    build_frame(version, 0x0200, terminal_phone, serial_no, &report.encode(version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::MsgHeader;

    #[test]
    fn test_build_heartbeat_2013() {
        let frame = build_heartbeat(ProtocolVersion::V2013, "013900000001", 1);
        assert!(frame.len() > 6);
        assert_eq!(frame[0], 0x7E);
        assert_eq!(*frame.last().unwrap(), 0x7E);

        let (decoded, _) = codec::decode_message(&frame).unwrap();
        let (header, header_len) = MsgHeader::decode(&decoded).unwrap();
        assert_eq!(header_len, 12);
        assert_eq!(header.protocol_version(), ProtocolVersion::V2013);
        assert_eq!(header.msg_id, 0x0002);
    }

    #[test]
    fn test_build_heartbeat_2019() {
        let frame = build_heartbeat(ProtocolVersion::V2019, "61316263600001", 1);
        assert_eq!(frame[0], 0x7E);
        assert_eq!(*frame.last().unwrap(), 0x7E);

        let (decoded, _) = codec::decode_message(&frame).unwrap();
        let (header, header_len) = MsgHeader::decode(&decoded).unwrap();
        assert_eq!(header_len, 17);
        assert_eq!(header.protocol_version(), ProtocolVersion::V2019);
        assert_eq!(header.version, Some(1));
        assert_eq!(header.msg_id, 0x0002);
        assert_eq!(header.terminal_id, "61316263600001");
        // 协议版本号紧跟消息体属性
        assert_eq!(decoded[4], 0x01);
        assert_eq!(
            &decoded[5..15],
            &[0x00, 0x00, 0x00, 0x61, 0x31, 0x62, 0x63, 0x60, 0x00, 0x01]
        );
    }

    #[test]
    fn test_build_register() {
        let frame = build_register(
            ProtocolVersion::V2013,
            "013900000001", 1, 31, 1, "MFG01", "MODEL01", "TID0001", 1, "京A88888",
        );
        assert_eq!(frame[0], 0x7E);
        assert_eq!(*frame.last().unwrap(), 0x7E);
    }

    #[test]
    fn test_build_register_2019() {
        let frame = build_register(
            ProtocolVersion::V2019,
            "013900000001", 1, 31, 1, "MFG01", "MODEL01", "013900000001", 1, "京A88888",
        );
        let (decoded, _) = codec::decode_message(&frame).unwrap();
        let (header, header_len) = MsgHeader::decode(&decoded).unwrap();
        assert_eq!(header.protocol_version(), ProtocolVersion::V2019);
        // 2019 register body: 4 + 11 + 30 + 30 + 1 + plate(GBK "京A88888" = 8)
        let body = &decoded[header_len..];
        assert_eq!(body.len(), 4 + 11 + 30 + 30 + 1 + 2 + 6);

        // Parse the same way the receive path does
        let parsed = crate::protocol::message::parse_message(
            header.protocol_version(),
            header.msg_id,
            body,
        );
        match parsed {
            crate::protocol::message::ParsedMessage::TerminalRegister(r) => {
                assert_eq!(r.terminal_id, "013900000001");
                assert_eq!(r.plate_number, "京A88888");
            }
            other => panic!("expected TerminalRegister, got {}", other.name()),
        }
    }

    #[test]
    fn test_build_auth() {
        let frame = build_auth(ProtocolVersion::V2013, "013900000001", 1, "AUTH1234", "", "");
        assert_eq!(frame[0], 0x7E);
        assert_eq!(*frame.last().unwrap(), 0x7E);
    }

    #[test]
    fn test_build_auth_2019() {
        let frame = build_auth(
            ProtocolVersion::V2019,
            "61316263600001",
            2,
            "AUTH1234",
            "861234567890123",
            "V1.0.0",
        );
        let (decoded, _) = codec::decode_message(&frame).unwrap();
        let (header, header_len) = MsgHeader::decode(&decoded).unwrap();
        assert_eq!(header.protocol_version(), ProtocolVersion::V2019);
        assert_eq!(header.serial_no, 2);
        // 2019 auth body: 1 + 8 + 15 + 20
        let body = &decoded[header_len..];
        assert_eq!(body.len(), 44);
        let parsed = crate::protocol::message::parse_message(
            header.protocol_version(),
            header.msg_id,
            body,
        );
        match parsed {
            crate::protocol::message::ParsedMessage::TerminalAuth(a) => {
                assert_eq!(a.auth_code, "AUTH1234");
                assert_eq!(a.imei, "861234567890123");
                assert_eq!(a.software_version, "V1.0.0");
            }
            other => panic!("expected TerminalAuth, got {}", other.name()),
        }
    }

    #[test]
    fn test_decode_heartbeat() {
        let frame = build_heartbeat(ProtocolVersion::V2013, "013900000001", 1);
        let (decoded, _) = codec::decode_message(&frame).unwrap();
        assert!(decoded.len() >= 12);
        let msg_id = u16::from_be_bytes([decoded[0], decoded[1]]);
        assert_eq!(msg_id, 0x0002);
    }
}
