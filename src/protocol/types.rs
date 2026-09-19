/// JT808 message types and constants
/// Based on JT/T 808-2013 / JT/T 808-2019 and the platform protocol
/// "终端与平台通信协议(兼容部标JT808)_V2.0_2019"

use serde::{Deserialize, Serialize};

/// JT/T 808 protocol version / 传输模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ProtocolVersion {
    /// JT/T 808-2013
    #[default]
    V2013,
    /// JT/T 808-2019
    V2019,
    /// 通用 TCP：原样收发，不组帧、不解析
    GenericTcp,
}

impl ProtocolVersion {
    pub fn all() -> &'static [ProtocolVersion] {
        &[Self::V2013, Self::V2019, Self::GenericTcp]
    }

    /// Display name, e.g. "JT808-2013"
    pub fn name(&self) -> &'static str {
        match self {
            Self::V2013 => "JT808-2013",
            Self::V2019 => "JT808-2019",
            Self::GenericTcp => "通用TCP",
        }
    }

    /// Short name, e.g. "2013"
    pub fn short_name(&self) -> &'static str {
        match self {
            Self::V2013 => "2013",
            Self::V2019 => "2019",
            Self::GenericTcp => "TCP",
        }
    }

    pub fn from_str_loose(s: &str) -> Self {
        let lower = s.to_ascii_lowercase();
        if lower.contains("tcp") || lower.contains("general") || s.contains("通用") {
            Self::GenericTcp
        } else if s.contains("2019") {
            Self::V2019
        } else {
            Self::V2013
        }
    }

    /// 是否为 JT808 协议（2013/2019）
    pub fn is_jt808(&self) -> bool {
        matches!(self, Self::V2013 | Self::V2019)
    }

    /// 是否为通用 TCP 原样透传模式
    pub fn is_generic(&self) -> bool {
        matches!(self, Self::GenericTcp)
    }

    /// Length in bytes of the terminal SN field in the message header.
    /// 2013: 12 hex chars -> 6 bytes; 2019: 20 hex chars -> 10 bytes.
    pub fn phone_len(&self) -> usize {
        match self {
            Self::V2019 => 10,
            _ => 6,
        }
    }

    /// Whether the message header carries a version number byte (0x01 for 2019)
    pub fn has_version_field(&self) -> bool {
        matches!(self, Self::V2019)
    }

    /// Value of the version number byte (only meaningful for 2019+)
    pub fn version_number(&self) -> Option<u8> {
        match self {
            Self::V2019 => Some(1),
            _ => None,
        }
    }
}

/// Message IDs
pub mod msg_id {
    // 终端通用应答
    pub const TERMINAL_GENERAL_RESP: u16 = 0x0001;
    // 终端心跳
    pub const TERMINAL_HEARTBEAT: u16 = 0x0002;
    // 终端注册
    pub const TERMINAL_REGISTER: u16 = 0x0100;
    // 终端注销
    pub const TERMINAL_UNREGISTER: u16 = 0x0003;
    // 终端鉴权
    pub const TERMINAL_AUTH: u16 = 0x0102;
    // 查询终端属性应答
    pub const TERMINAL_ATTR_RESP: u16 = 0x0107;
    // 位置信息汇报
    pub const TERMINAL_LOCATION_REPORT: u16 = 0x0200;

    // 平台通用应答
    pub const PLATFORM_GENERAL_RESP: u16 = 0x8001;
    // 平台注册应答
    pub const PLATFORM_REGISTER_RESP: u16 = 0x8100;
    // 平台参数设置
    pub const PLATFORM_PARAM_SET: u16 = 0x8103;
    // 平台参数查询
    pub const PLATFORM_PARAM_QUERY: u16 = 0x8104;
    // 查询终端属性
    pub const PLATFORM_QUERY_ATTR: u16 = 0x8107;
    // 临时位置跟踪
    pub const PLATFORM_TEMP_TRACK: u16 = 0x8202;
    // 文本信息下发
    pub const PLATFORM_TEXT_MSG: u16 = 0x8300;
    // 车辆控制
    pub const PLATFORM_VEHICLE_CONTROL: u16 = 0x8500;
}

/// Protocol constants
pub mod constant {
    /// Message delimiter byte
    pub const DELIMITER: u8 = 0x7E;
    /// Escape byte
    pub const ESCAPE: u8 = 0x7D;
    /// Escape mappings
    pub const ESCAPE_MAP: [(u8, u8); 2] = [(0x7E, 0x02), (0x7D, 0x01)];
    /// Message body properties: version flag (2019+) is bit 14
    pub const PROPS_VERSION_FLAG: u16 = 1 << 14;
    /// Message body properties: subpackage flag is bit 13
    pub const PROPS_SUBPACKAGE_FLAG: u16 = 1 << 13;
}

/// Message header.
///
/// 2013 layout (12 bytes + optional 4):
///   msg_id(2) + props(2) + SN(6, 12 hex chars) + serial(2) + [subpackage(4)]
/// 2019 layout (17 bytes + optional 4):
///   msg_id(2) + props(2) + version(1) + SN(10, 20 hex chars) + serial(2) + [subpackage(4)]
#[derive(Debug, Clone)]
pub struct MsgHeader {
    /// Message ID (2 bytes)
    pub msg_id: u16,
    /// Message body properties (2 bytes)
    pub body_props: BodyProps,
    /// Terminal SN (carried in the "terminal phone" field).
    /// 2013: 12 hex chars; 2019: 20 hex chars.
    pub terminal_id: String,
    /// Message serial number (2 bytes)
    pub serial_no: u16,
    /// Optional subpackage info
    pub subpackage: Option<SubpackageInfo>,
    /// Protocol version number (1 byte, right after body properties, 2019 only).
    /// None = JT808-2013 header (bit14 = 0).
    pub version: Option<u8>,
}

/// Message body properties
#[derive(Debug, Clone, Default)]
pub struct BodyProps {
    /// Message body length (10 bits)
    pub body_len: u16,
    /// Whether data encryption is used (3 bits, usually 0=none)
    pub encryption: u8,
    /// Whether subpackage
    pub has_subpackage: bool,
}

/// Subpackage information (optional, 4 bytes)
#[derive(Debug, Clone)]
pub struct SubpackageInfo {
    /// Total number of packages
    pub total: u16,
    /// Package sequence number
    pub seq: u16,
}

impl MsgHeader {
    /// Protocol version inferred from the header (version flag + version byte)
    pub fn protocol_version(&self) -> ProtocolVersion {
        if self.version.is_some() {
            ProtocolVersion::V2019
        } else {
            ProtocolVersion::V2013
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut props = self.body_props.encode();
        if self.version.is_some() {
            props |= constant::PROPS_VERSION_FLAG;
        }
        let mut buf = Vec::with_capacity(21);
        buf.extend_from_slice(&self.msg_id.to_be_bytes());
        buf.extend_from_slice(&props.to_be_bytes());
        // 2019: 消息体属性后紧跟 1 字节协议版本号（固定为 1）
        if let Some(v) = self.version {
            buf.push(v);
        }
        // 终端 SN：2013 = 6 字节(12 hex chars)，2019 = 10 字节(20 hex chars)
        let sn_len = if self.version.is_some() { 10 } else { 6 };
        buf.extend_from_slice(&string_to_bcd(&self.terminal_id, sn_len));
        buf.extend_from_slice(&self.serial_no.to_be_bytes());
        if let Some(ref sp) = self.subpackage {
            buf.extend_from_slice(&sp.total.to_be_bytes());
            buf.extend_from_slice(&sp.seq.to_be_bytes());
        }
        buf
    }

    /// Decode a message header from raw (unescaped, checksum-free) message bytes.
    /// Returns the header and its total encoded length.
    pub fn decode(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 4 {
            return Err("消息头过短".into());
        }
        let msg_id = u16::from_be_bytes([data[0], data[1]]);
        let props_raw = u16::from_be_bytes([data[2], data[3]]);
        let has_subpackage = props_raw & constant::PROPS_SUBPACKAGE_FLAG != 0;
        let has_version = props_raw & constant::PROPS_VERSION_FLAG != 0;

        let mut pos = 4usize;
        // 2019: 协议版本号紧跟消息体属性
        let version = if has_version {
            if data.len() < pos + 1 {
                return Err("消息头版本号缺失".into());
            }
            let v = data[pos];
            pos += 1;
            Some(v)
        } else {
            None
        };

        let sn_len = if has_version { 10 } else { 6 };
        if data.len() < pos + sn_len + 2 {
            return Err("消息头终端SN不完整".into());
        }
        let mut terminal_id = bcd_to_string(&data[pos..pos + sn_len]);
        if has_version {
            // 2019 SN 为 20 个十六进制字符，不足时前置补 0（平台协议 4.4.3），
            // 解码时去掉前导 0 还原终端 SN
            let trimmed = terminal_id.trim_start_matches('0');
            if !trimmed.is_empty() {
                terminal_id = trimmed.to_string();
            }
        }
        pos += sn_len;

        let serial_no = u16::from_be_bytes([data[pos], data[pos + 1]]);
        pos += 2;

        let subpackage = if has_subpackage {
            if data.len() < pos + 4 {
                return Err("消息头分包信息不完整".into());
            }
            let total = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let seq = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
            pos += 4;
            Some(SubpackageInfo { total, seq })
        } else {
            None
        };

        Ok((
            Self {
                msg_id,
                body_props: BodyProps {
                    body_len: props_raw & 0x03FF,
                    encryption: ((props_raw >> 10) & 0x07) as u8,
                    has_subpackage,
                },
                terminal_id,
                serial_no,
                subpackage,
                version,
            },
            pos,
        ))
    }
}

impl BodyProps {
    pub fn encode(&self) -> u16 {
        let mut val = self.body_len & 0x03FF;
        val |= (self.encryption as u16 & 0x07) << 10;
        if self.has_subpackage {
            val |= constant::PROPS_SUBPACKAGE_FLAG;
        }
        val
    }
}

/// Pack a hex-digit string into a fixed-length byte field.
///
/// Used for BCD timestamps (YYMMDDHHMMSS) and the terminal SN header field
/// (2013: 12 hex chars / 6 bytes, 2019: 20 hex chars / 10 bytes).
/// Short strings are left-padded with '0' (per platform doc: 不足时前置补 0).
pub fn string_to_bcd(s: &str, len: usize) -> Vec<u8> {
    let target = len * 2;
    let mut chars: Vec<u8> = s
        .bytes()
        .filter(|b| b.is_ascii_hexdigit())
        .map(|b| b.to_ascii_uppercase())
        .collect();
    // Keep the right-most digits when too long (numeric right-alignment)
    if chars.len() > target {
        chars.drain(0..chars.len() - target);
    }
    while chars.len() < target {
        chars.insert(0, b'0');
    }

    let mut buf = vec![0u8; len];
    for (i, &c) in chars.iter().enumerate() {
        let v = (c as char).to_digit(16).unwrap_or(0) as u8;
        if i % 2 == 0 {
            buf[i / 2] = v << 4;
        } else {
            buf[i / 2] |= v;
        }
    }
    buf
}

/// Unpack a byte field into an uppercase hex-digit string
/// (e.g. BCD timestamp or the terminal SN header field).
pub fn bcd_to_string(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for &b in data {
        let high = (b >> 4) & 0x0F;
        let low = b & 0x0F;
        s.push(std::char::from_digit(high as u32, 16).unwrap_or('0').to_ascii_uppercase());
        s.push(std::char::from_digit(low as u32, 16).unwrap_or('0').to_ascii_uppercase());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_generic_tcp() {
        assert_eq!(
            ProtocolVersion::from_str_loose("TCP"),
            ProtocolVersion::GenericTcp
        );
        assert_eq!(
            ProtocolVersion::from_str_loose("通用TCP"),
            ProtocolVersion::GenericTcp
        );
        assert_eq!(
            ProtocolVersion::from_str_loose("2019"),
            ProtocolVersion::V2019
        );
        assert_eq!(
            ProtocolVersion::from_str_loose("2013"),
            ProtocolVersion::V2013
        );
        assert!(ProtocolVersion::GenericTcp.is_generic());
        assert!(!ProtocolVersion::V2019.is_generic());
        assert!(!ProtocolVersion::GenericTcp.is_jt808());
        assert!(ProtocolVersion::V2013.is_jt808());
        assert_eq!(ProtocolVersion::GenericTcp.short_name(), "TCP");
    }

    #[test]
    fn test_bcd_roundtrip() {
        let bcd = string_to_bcd("013900000001", 6);
        assert_eq!(bcd, vec![0x01, 0x39, 0x00, 0x00, 0x00, 0x01]);
        assert_eq!(bcd_to_string(&bcd), "013900000001");
    }

    #[test]
    fn test_sn_2019_left_zero_padded() {
        // Platform doc 4.4.3: 20 hex chars, left-padded with 0, SN = 61316263600001
        let bytes = string_to_bcd("61316263600001", 10);
        assert_eq!(
            bytes,
            vec![0x00, 0x00, 0x00, 0x61, 0x31, 0x62, 0x63, 0x60, 0x00, 0x01]
        );
        assert_eq!(bcd_to_string(&bytes), "00000061316263600001");
    }

    #[test]
    fn test_sn_hex_chars() {
        // A-F hex characters are packed as bytes, not decimal digits
        let bytes = string_to_bcd("AF1234567890", 6);
        assert_eq!(bytes, vec![0xAF, 0x12, 0x34, 0x56, 0x78, 0x90]);
        assert_eq!(bcd_to_string(&bytes), "AF1234567890");
    }

    #[test]
    fn test_header_2013_roundtrip() {
        let header = MsgHeader {
            msg_id: 0x0002,
            body_props: BodyProps::default(),
            terminal_id: "013900000001".into(),
            serial_no: 7,
            subpackage: None,
            version: None,
        };
        let bytes = header.encode();
        assert_eq!(bytes.len(), 12);
        let (decoded, header_len) = MsgHeader::decode(&bytes).unwrap();
        assert_eq!(header_len, 12);
        assert_eq!(decoded.msg_id, 0x0002);
        assert_eq!(decoded.terminal_id, "013900000001");
        assert_eq!(decoded.serial_no, 7);
        assert_eq!(decoded.protocol_version(), ProtocolVersion::V2013);
    }

    #[test]
    fn test_header_2019_roundtrip() {
        let header = MsgHeader {
            msg_id: 0x0200,
            body_props: BodyProps {
                body_len: 28,
                encryption: 0,
                has_subpackage: false,
            },
            terminal_id: "61316263600001".into(),
            serial_no: 9,
            subpackage: None,
            version: Some(1),
        };
        let bytes = header.encode();
        assert_eq!(bytes.len(), 17);
        // 消息体属性 bit14 must be set
        assert_eq!(u16::from_be_bytes([bytes[2], bytes[3]]), 28 | (1 << 14));
        // 协议版本号 right after body properties
        assert_eq!(bytes[4], 0x01);
        // SN left zero padded to 10 bytes
        assert_eq!(
            &bytes[5..15],
            &[0x00, 0x00, 0x00, 0x61, 0x31, 0x62, 0x63, 0x60, 0x00, 0x01]
        );
        let (decoded, header_len) = MsgHeader::decode(&bytes).unwrap();
        assert_eq!(header_len, 17);
        assert_eq!(decoded.terminal_id, "61316263600001");
        assert_eq!(decoded.serial_no, 9);
        assert_eq!(decoded.version, Some(1));
        assert_eq!(decoded.protocol_version(), ProtocolVersion::V2019);
    }

    #[test]
    fn test_decode_platform_2019_response() {
        // Real 0x8001 platform general response (checksum removed):
        // 8001 4005 01 00000061316263600001 0003 0027090000
        // props=0x4005 (bit14 set, body len 5), version=1,
        // SN=61316263600001, serial=3, body: respSerial=39, respMsgId=0x0900, result=0
        let bytes = [
            0x80, 0x01, 0x40, 0x05, 0x01, 0x00, 0x00, 0x00, 0x61, 0x31, 0x62, 0x63, 0x60, 0x00,
            0x01, 0x00, 0x03, 0x00, 0x27, 0x09, 0x00, 0x00,
        ];
        let (header, header_len) = MsgHeader::decode(&bytes).unwrap();
        assert_eq!(header_len, 17);
        assert_eq!(header.msg_id, 0x8001);
        assert_eq!(header.protocol_version(), ProtocolVersion::V2019);
        assert_eq!(header.version, Some(1));
        assert_eq!(header.terminal_id, "61316263600001");
        assert_eq!(header.serial_no, 3);
        assert_eq!(header.body_props.body_len, 5);
        assert_eq!(&bytes[header_len..], &[0x00, 0x27, 0x09, 0x00, 0x00]);
    }

    #[test]
    fn test_header_2019_subpackage() {
        let header = MsgHeader {
            msg_id: 0x0200,
            body_props: BodyProps {
                body_len: 100,
                encryption: 0,
                has_subpackage: true,
            },
            terminal_id: "013900000001".into(),
            serial_no: 9,
            subpackage: Some(SubpackageInfo { total: 3, seq: 2 }),
            version: Some(1),
        };
        let bytes = header.encode();
        assert_eq!(bytes.len(), 21);
        let (decoded, header_len) = MsgHeader::decode(&bytes).unwrap();
        assert_eq!(header_len, 21);
        assert_eq!(decoded.subpackage.as_ref().unwrap().total, 3);
        assert_eq!(decoded.subpackage.as_ref().unwrap().seq, 2);
    }
}
