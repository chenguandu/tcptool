/// JT808 Message definitions
/// Message body structures for each message type

use serde::{Deserialize, Serialize};

use crate::protocol::types;

/// Generic message body trait
pub trait Jt808Message: Sized {
    fn msg_id() -> u16;
    fn decode(data: &[u8]) -> Result<Self, String>;
    fn encode(&self) -> Vec<u8>;
}

// ==================== 0x0001: Terminal General Response ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalGeneralResponse {
    pub response_serial_no: u16,
    pub response_msg_id: u16,
    pub result: u8,
}

impl Jt808Message for TerminalGeneralResponse {
    fn msg_id() -> u16 {
        0x0001
    }
    fn decode(data: &[u8]) -> Result<Self, String> {
        if data.len() < 5 {
            return Err("TerminalGeneralResponse: too short".into());
        }
        Ok(Self {
            response_serial_no: u16::from_be_bytes([data[0], data[1]]),
            response_msg_id: u16::from_be_bytes([data[2], data[3]]),
            result: data[4],
        })
    }
    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(5);
        buf.extend_from_slice(&self.response_serial_no.to_be_bytes());
        buf.extend_from_slice(&self.response_msg_id.to_be_bytes());
        buf.push(self.result);
        buf
    }
}

// ==================== 0x0002: Heartbeat ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat;

impl Jt808Message for Heartbeat {
    fn msg_id() -> u16 {
        0x0002
    }
    fn decode(_data: &[u8]) -> Result<Self, String> {
        Ok(Self)
    }
    fn encode(&self) -> Vec<u8> {
        Vec::new()
    }
}

// ==================== 0x0100: Terminal Register ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalRegister {
    pub province_id: u16,
    pub city_id: u16,
    pub manufacturer_id: String,      // 5 bytes
    pub terminal_model: String,       // 20 bytes (right-padded with 0x00)
    pub terminal_id: String,          // 7 bytes
    pub color: u8,                    // 1=blue, 2=yellow, 3=black
    pub plate_number: String,         // vehicle plate
}

impl Jt808Message for TerminalRegister {
    fn msg_id() -> u16 {
        0x0100
    }
    fn decode(data: &[u8]) -> Result<Self, String> {
        if data.len() < 36 {
            return Err("TerminalRegister: too short".into());
        }
        let province_id = u16::from_be_bytes([data[0], data[1]]);
        let city_id = u16::from_be_bytes([data[2], data[3]]);
        let manufacturer_id = String::from_utf8_lossy(&data[4..9]).to_string();
        let terminal_model = String::from_utf8_lossy(&data[9..29]).to_string();
        let terminal_id = String::from_utf8_lossy(&data[29..36]).to_string();
        let color = data[36];
        let plate_number = String::from_utf8_lossy(&data[37..]).trim_end_matches('\0').to_string();
        Ok(Self {
            province_id,
            city_id,
            manufacturer_id: manufacturer_id.trim_end_matches('\0').to_string(),
            terminal_model: terminal_model.trim_end_matches('\0').to_string(),
            terminal_id: terminal_id.trim_end_matches('\0').to_string(),
            color,
            plate_number: plate_number.trim_end_matches('\0').to_string(),
        })
    }
    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(37 + self.plate_number.len());
        buf.extend_from_slice(&self.province_id.to_be_bytes());
        buf.extend_from_slice(&self.city_id.to_be_bytes());
        let mut mf = self.manufacturer_id.as_bytes().to_vec();
        mf.resize(5, 0x00);
        buf.extend_from_slice(&mf);
        let mut tm = self.terminal_model.as_bytes().to_vec();
        tm.resize(20, 0x00);
        buf.extend_from_slice(&tm);
        let mut tid = self.terminal_id.as_bytes().to_vec();
        tid.resize(7, 0x00);
        buf.extend_from_slice(&tid);
        buf.push(self.color);
        buf.extend_from_slice(self.plate_number.as_bytes());
        buf
    }
}

// ==================== 0x8100: Register Response ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub response_serial_no: u16,
    pub result: u8,  // 0=success, 1=vehicle already registered, 2=not found, etc.
    pub auth_code: String,
}

impl Jt808Message for RegisterResponse {
    fn msg_id() -> u16 {
        0x8100
    }
    fn decode(data: &[u8]) -> Result<Self, String> {
        if data.len() < 3 {
            return Err("RegisterResponse: too short".into());
        }
        let response_serial_no = u16::from_be_bytes([data[0], data[1]]);
        let result = data[2];
        let auth_code = String::from_utf8_lossy(&data[3..]).trim_end_matches('\0').to_string();
        Ok(Self { response_serial_no, result, auth_code })
    }
    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(3 + self.auth_code.len());
        buf.extend_from_slice(&self.response_serial_no.to_be_bytes());
        buf.push(self.result);
        buf.extend_from_slice(self.auth_code.as_bytes());
        buf
    }
}

// ==================== 0x0102: Terminal Auth ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalAuth {
    pub auth_code: String,
}

impl Jt808Message for TerminalAuth {
    fn msg_id() -> u16 {
        0x0102
    }
    fn decode(data: &[u8]) -> Result<Self, String> {
        let auth_code = String::from_utf8_lossy(data).trim_end_matches('\0').to_string();
        Ok(Self { auth_code })
    }
    fn encode(&self) -> Vec<u8> {
        self.auth_code.as_bytes().to_vec()
    }
}

// ==================== 0x0200: Location Report ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationReport {
    pub alarm_flags: u32,
    pub status: u32,
    pub latitude: u32,    // degrees * 10^6
    pub longitude: u32,   // degrees * 10^6
    pub altitude: u16,    // meters
    pub speed: u16,       // 0.1 km/h
    pub direction: u16,   // 0-359 degrees
    pub timestamp: String, // BCD 6 bytes: YYMMDDHHMMSS
    pub extra_items: Vec<ExtraItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraItem {
    pub id: u8,
    pub value: Vec<u8>,
}

impl Jt808Message for LocationReport {
    fn msg_id() -> u16 {
        0x0200
    }
    fn decode(data: &[u8]) -> Result<Self, String> {
        if data.len() < 28 {
            return Err("LocationReport: too short".into());
        }
        let alarm_flags = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let status = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let latitude = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let longitude = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
        let altitude = u16::from_be_bytes([data[16], data[17]]);
        let speed = u16::from_be_bytes([data[18], data[19]]);
        let direction = u16::from_be_bytes([data[20], data[21]]);
        let timestamp = types::bcd_to_string(&data[22..28]);

        let mut extra_items = Vec::new();
        let mut pos = 28;
        while pos + 2 < data.len() {
            let item_id = data[pos];
            let item_len = data[pos + 1] as usize;
            pos += 2;
            if pos + item_len > data.len() {
                break;
            }
            extra_items.push(ExtraItem {
                id: item_id,
                value: data[pos..pos + item_len].to_vec(),
            });
            pos += item_len;
        }

        Ok(Self {
            alarm_flags,
            status,
            latitude,
            longitude,
            altitude,
            speed,
            direction,
            timestamp,
            extra_items,
        })
    }
    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(28);
        buf.extend_from_slice(&self.alarm_flags.to_be_bytes());
        buf.extend_from_slice(&self.status.to_be_bytes());
        buf.extend_from_slice(&self.latitude.to_be_bytes());
        buf.extend_from_slice(&self.longitude.to_be_bytes());
        buf.extend_from_slice(&self.altitude.to_be_bytes());
        buf.extend_from_slice(&self.speed.to_be_bytes());
        buf.extend_from_slice(&self.direction.to_be_bytes());
        // Encode timestamp as BCD
        let ts_bytes = types::string_to_bcd(&self.timestamp, 6);
        buf.extend_from_slice(&ts_bytes);
        for item in &self.extra_items {
            buf.push(item.id);
            buf.push(item.value.len() as u8);
            buf.extend_from_slice(&item.value);
        }
        buf
    }
}

// ==================== 0x8001: Platform General Response ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformGeneralResponse {
    pub response_serial_no: u16,
    pub response_msg_id: u16,
    pub result: u8,
}

impl Jt808Message for PlatformGeneralResponse {
    fn msg_id() -> u16 {
        0x8001
    }
    fn decode(data: &[u8]) -> Result<Self, String> {
        if data.len() < 5 {
            return Err("PlatformGeneralResponse: too short".into());
        }
        Ok(Self {
            response_serial_no: u16::from_be_bytes([data[0], data[1]]),
            response_msg_id: u16::from_be_bytes([data[2], data[3]]),
            result: data[4],
        })
    }
    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(5);
        buf.extend_from_slice(&self.response_serial_no.to_be_bytes());
        buf.extend_from_slice(&self.response_msg_id.to_be_bytes());
        buf.push(self.result);
        buf
    }
}

// ==================== 0x8103: Parameter Set ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterItem {
    pub param_id: u32,
    pub param_value: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterSet {
    pub total: u8,
    pub items: Vec<ParameterItem>,
}

impl Jt808Message for ParameterSet {
    fn msg_id() -> u16 {
        0x8103
    }
    fn decode(data: &[u8]) -> Result<Self, String> {
        if data.is_empty() {
            return Err("ParameterSet: empty".into());
        }
        let total = data[0];
        let mut items = Vec::new();
        let mut pos = 1;
        while pos + 4 < data.len() {
            let param_id = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            pos += 4;
            let param_len = match param_id {
                // 1 byte params
                0x0001 | 0x0002 | 0x0003 | 0x0010 | 0x0011 | 0x0012 | 0x0013 | 0x0014 | 0x0015
                | 0x0016 | 0x0017 | 0x0018 | 0x0019 | 0x001A | 0x001B => 1,
                // 2 byte params
                0x0020 | 0x0021 | 0x0022 | 0x0023 | 0x0024 | 0x0025 | 0x0026 => 2,
                // 4 byte params
                0x0027 | 0x0028 | 0x0029 | 0x002A | 0x002B | 0x002C | 0x002D | 0x002E | 0x002F => 4,
                _ => {
                    if pos >= data.len() { break; }
                    data[pos] as usize
                }
            };
            if param_len == 0 {
                // Try to read length from data
                if pos >= data.len() { break; }
                let param_len = data[pos] as usize;
                pos += 1;
                if pos + param_len > data.len() { break; }
                items.push(ParameterItem {
                    param_id,
                    param_value: data[pos..pos + param_len].to_vec(),
                });
                pos += param_len;
            } else {
                if pos + param_len > data.len() { break; }
                items.push(ParameterItem {
                    param_id,
                    param_value: data[pos..pos + param_len].to_vec(),
                });
                pos += param_len;
            }
        }
        Ok(Self { total, items })
    }
    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(self.total);
        for item in &self.items {
            buf.extend_from_slice(&item.param_id.to_be_bytes());
            buf.push(item.param_value.len() as u8);
            buf.extend_from_slice(&item.param_value);
        }
        buf
    }
}

// ==================== 0x8300: Text Message ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextMessage {
    pub flag: u8,  // 0=emergency, 1=normal, etc.
    pub text: String,
}

impl Jt808Message for TextMessage {
    fn msg_id() -> u16 {
        0x8300
    }
    fn decode(data: &[u8]) -> Result<Self, String> {
        if data.is_empty() {
            return Err("TextMessage: empty".into());
        }
        let flag = data[0];
        let text = String::from_utf8_lossy(&data[1..]).trim_end_matches('\0').to_string();
        Ok(Self { flag, text })
    }
    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + self.text.len());
        buf.push(self.flag);
        buf.extend_from_slice(self.text.as_bytes());
        buf
    }
}

// ==================== 0x8500: Vehicle Control ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleControl {
    pub control_flag: u8,
}

impl Jt808Message for VehicleControl {
    fn msg_id() -> u16 {
        0x8500
    }
    fn decode(data: &[u8]) -> Result<Self, String> {
        if data.is_empty() {
            return Err("VehicleControl: empty".into());
        }
        Ok(Self { control_flag: data[0] })
    }
    fn encode(&self) -> Vec<u8> {
        vec![self.control_flag]
    }
}

// ==================== Message dispatch ====================
/// A parsed message with its type info
#[derive(Debug, Clone)]
pub enum ParsedMessage {
    TerminalGeneralResponse(TerminalGeneralResponse),
    Heartbeat(Heartbeat),
    TerminalRegister(TerminalRegister),
    TerminalAuth(TerminalAuth),
    RegisterResponse(RegisterResponse),
    PlatformGeneralResponse(PlatformGeneralResponse),
    LocationReport(LocationReport),
    ParameterSet(ParameterSet),
    TextMessage(TextMessage),
    VehicleControl(VehicleControl),
    Unknown { msg_id: u16, body: Vec<u8> },
}

impl ParsedMessage {
    pub fn msg_id(&self) -> u16 {
        match self {
            Self::TerminalGeneralResponse(_) => 0x0001,
            Self::Heartbeat(_) => 0x0002,
            Self::TerminalRegister(_) => 0x0100,
            Self::TerminalAuth(_) => 0x0102,
            Self::RegisterResponse(_) => 0x8100,
            Self::PlatformGeneralResponse(_) => 0x8001,
            Self::LocationReport(_) => 0x0200,
            Self::ParameterSet(_) => 0x8103,
            Self::TextMessage(_) => 0x8300,
            Self::VehicleControl(_) => 0x8500,
            Self::Unknown { msg_id, .. } => *msg_id,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::TerminalGeneralResponse(_) => "终端通用应答",
            Self::Heartbeat(_) => "终端心跳",
            Self::TerminalRegister(_) => "终端注册",
            Self::TerminalAuth(_) => "终端鉴权",
            Self::RegisterResponse(_) => "平台注册应答",
            Self::PlatformGeneralResponse(_) => "平台通用应答",
            Self::LocationReport(_) => "位置信息汇报",
            Self::ParameterSet(_) => "参数设置",
            Self::TextMessage(_) => "文本信息下发",
            Self::VehicleControl(_) => "车辆控制",
            Self::Unknown { .. } => "未知消息",
        }
    }

    pub fn description(&self) -> String {
        match self {
            Self::TerminalGeneralResponse(r) => format!("应答ID:0x{:04X} 结果:{}", r.response_msg_id, r.result),
            Self::Heartbeat(_) => "心跳".into(),
            Self::TerminalRegister(r) => format!("终端:{}", r.terminal_id),
            Self::TerminalAuth(r) => format!("鉴权码:{}", r.auth_code),
            Self::RegisterResponse(r) => format!("结果:{} 鉴权码:{}", r.result, r.auth_code),
            Self::PlatformGeneralResponse(r) => format!("应答ID:0x{:04X} 结果:{}", r.response_msg_id, r.result),
            Self::LocationReport(r) => format!("位置:({},{})", r.longitude, r.latitude),
            Self::ParameterSet(r) => format!("{}个参数", r.total),
            Self::TextMessage(r) => format!("{}", r.text),
            Self::VehicleControl(r) => format!("控制标志:0x{:02X}", r.control_flag),
            Self::Unknown { msg_id, body } => format!("ID:0x{:04X} 长度:{}", msg_id, body.len()),
        }
    }
}

/// Parse a message body given its msg_id
pub fn parse_message(msg_id: u16, body: &[u8]) -> ParsedMessage {
    macro_rules! try_parse {
        ($ty:ty, $variant:ident) => {
            match <$ty as Jt808Message>::decode(body) {
                Ok(m) => ParsedMessage::$variant(m),
                Err(e) => {
                    log::warn!("Failed to parse 0x{:04X}: {}", msg_id, e);
                    ParsedMessage::Unknown { msg_id, body: body.to_vec() }
                }
            }
        };
    }
    match msg_id {
        0x0001 => try_parse!(TerminalGeneralResponse, TerminalGeneralResponse),
        0x0002 => try_parse!(Heartbeat, Heartbeat),
        0x0100 => try_parse!(TerminalRegister, TerminalRegister),
        0x0102 => try_parse!(TerminalAuth, TerminalAuth),
        0x8001 => try_parse!(PlatformGeneralResponse, PlatformGeneralResponse),
        0x8100 => try_parse!(RegisterResponse, RegisterResponse),
        0x0200 => try_parse!(LocationReport, LocationReport),
        0x8103 => try_parse!(ParameterSet, ParameterSet),
        0x8300 => try_parse!(TextMessage, TextMessage),
        0x8500 => try_parse!(VehicleControl, VehicleControl),
        _ => ParsedMessage::Unknown { msg_id, body: body.to_vec() },
    }
}
