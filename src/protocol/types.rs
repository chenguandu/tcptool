/// JT808 message types and constants
/// Based on JT/T 808-2013 specification

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
}

/// Message header (12 bytes basic + optional subpackage info)
#[derive(Debug, Clone)]
pub struct MsgHeader {
    /// Message ID (2 bytes)
    pub msg_id: u16,
    /// Message body properties (2 bytes)
    pub body_props: BodyProps,
    /// Terminal phone number / device ID (6 bytes BCD)
    pub terminal_id: String,
    /// Message serial number (2 bytes)
    pub serial_no: u16,
    /// Optional subpackage info
    pub subpackage: Option<SubpackageInfo>,
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
    /// Whether it's a platform response (bit 7 of attr2)
    pub is_response: bool,
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
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(12);
        buf.extend_from_slice(&self.msg_id.to_be_bytes());
        // Body properties
        let props = self.body_props.encode();
        buf.extend_from_slice(&props.to_be_bytes());
        // Terminal ID (6 bytes BCD)
        let bcd = string_to_bcd(&self.terminal_id, 6);
        buf.extend_from_slice(&bcd);
        // Serial number
        buf.extend_from_slice(&self.serial_no.to_be_bytes());
        // Subpackage info if present
        if let Some(ref sp) = self.subpackage {
            buf.extend_from_slice(&sp.total.to_be_bytes());
            buf.extend_from_slice(&sp.seq.to_be_bytes());
        }
        buf
    }
}

impl BodyProps {
    pub fn encode(&self) -> u16 {
        let mut val = self.body_len & 0x03FF;
        val |= (self.encryption as u16 & 0x07) << 10;
        if self.has_subpackage {
            val |= 1 << 13;
        }
        if self.is_response {
            val |= 1 << 14;
        }
        val
    }
}

/// Convert string to BCD bytes (right-padded with 0xF)
pub fn string_to_bcd(s: &str, len: usize) -> Vec<u8> {
    let digits: Vec<u8> = s.bytes().filter(|b| b.is_ascii_digit()).map(|b| b - b'0').collect();
    let mut buf = vec![0x00u8; len];
    for i in 0..digits.len().min(len * 2) {
        let byte_idx = i / 2;
        let nibble_shift = if i % 2 == 0 { 4 } else { 0 };
        buf[byte_idx] |= digits[i] << nibble_shift;
    }
    buf
}

/// Convert BCD bytes to string
pub fn bcd_to_string(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for &b in data {
        let high = (b >> 4) & 0x0F;
        let low = b & 0x0F;
        if high <= 9 {
            s.push((b'0' + high) as char);
        }
        if low <= 9 {
            s.push((b'0' + low) as char);
        }
    }
    s
}
