/// JT808 Message definitions
/// Message body structures for each message type (supports 2013 / 2019)

use serde::{Deserialize, Serialize};

use crate::protocol::types::{self, ProtocolVersion};

/// Generic message body trait.
/// `version` is provided so that message bodies that changed between
/// JT/T 808-2013 and JT/T 808-2019 can encode/decode correctly.
pub trait Jt808Message: Sized {
    fn msg_id() -> u16;
    fn decode(data: &[u8], version: ProtocolVersion) -> Result<Self, String>;
    fn encode(&self, version: ProtocolVersion) -> Vec<u8>;
}

/// Encode a string as GBK (platform STRING fields are GBK encoded)
fn gbk_encode(s: &str) -> Vec<u8> {
    let (encoded, _, _) = encoding_rs::GBK.encode(s);
    encoded.into_owned()
}

/// Decode a GBK string field
fn gbk_decode(data: &[u8]) -> String {
    let (decoded, _, _) = encoding_rs::GBK.decode(data);
    decoded.into_owned()
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
    fn decode(data: &[u8], _version: ProtocolVersion) -> Result<Self, String> {
        if data.len() < 5 {
            return Err("TerminalGeneralResponse: too short".into());
        }
        Ok(Self {
            response_serial_no: u16::from_be_bytes([data[0], data[1]]),
            response_msg_id: u16::from_be_bytes([data[2], data[3]]),
            result: data[4],
        })
    }
    fn encode(&self, _version: ProtocolVersion) -> Vec<u8> {
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
    fn decode(_data: &[u8], _version: ProtocolVersion) -> Result<Self, String> {
        Ok(Self)
    }
    fn encode(&self, _version: ProtocolVersion) -> Vec<u8> {
        Vec::new()
    }
}

// ==================== 0x0100: Terminal Register ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalRegister {
    pub province_id: u16,
    pub city_id: u16,
    pub manufacturer_id: String, // 2013: 5 bytes, 2019: 11 bytes
    pub terminal_model: String,  // 2013: 20 bytes, 2019: 30 bytes
    pub terminal_id: String,     // 2013: 7 bytes, 2019: 30 bytes
    pub color: u8,               // 1=blue, 2=yellow, 3=black
    pub plate_number: String,    // vehicle plate
}

impl TerminalRegister {
    /// Fixed field lengths for the given protocol version:
    /// (manufacturer_id, terminal_model, terminal_id)
    pub fn field_lengths(version: ProtocolVersion) -> (usize, usize, usize) {
        match version {
            ProtocolVersion::V2019 => (11, 30, 30),
            _ => (5, 20, 7),
        }
    }
}

impl Jt808Message for TerminalRegister {
    fn msg_id() -> u16 {
        0x0100
    }
    fn decode(data: &[u8], version: ProtocolVersion) -> Result<Self, String> {
        let (mfg_len, model_len, tid_len) = Self::field_lengths(version);
        let min_len = 4 + mfg_len + model_len + tid_len + 1;
        if data.len() < min_len {
            return Err(format!("TerminalRegister: too short (need {} bytes)", min_len));
        }
        let province_id = u16::from_be_bytes([data[0], data[1]]);
        let city_id = u16::from_be_bytes([data[2], data[3]]);

        let mut pos = 4;
        let manufacturer_id = String::from_utf8_lossy(&data[pos..pos + mfg_len])
            .trim_end_matches('\0')
            .to_string();
        pos += mfg_len;
        let terminal_model = String::from_utf8_lossy(&data[pos..pos + model_len])
            .trim_end_matches('\0')
            .to_string();
        pos += model_len;
        let terminal_id = String::from_utf8_lossy(&data[pos..pos + tid_len])
            .trim_end_matches('\0')
            .to_string();
        pos += tid_len;
        let color = data[pos];
        pos += 1;
        let plate_number = gbk_decode(&data[pos..])
            .trim_end_matches('\0')
            .to_string();
        Ok(Self {
            province_id,
            city_id,
            manufacturer_id,
            terminal_model,
            terminal_id,
            color,
            plate_number,
        })
    }
    fn encode(&self, version: ProtocolVersion) -> Vec<u8> {
        let (mfg_len, model_len, tid_len) = Self::field_lengths(version);
        let mut buf = Vec::with_capacity(4 + mfg_len + model_len + tid_len + 1 + self.plate_number.len());
        buf.extend_from_slice(&self.province_id.to_be_bytes());
        buf.extend_from_slice(&self.city_id.to_be_bytes());
        let mut mf = self.manufacturer_id.as_bytes().to_vec();
        mf.resize(mfg_len, 0x00);
        buf.extend_from_slice(&mf);
        let mut tm = self.terminal_model.as_bytes().to_vec();
        tm.resize(model_len, 0x00);
        buf.extend_from_slice(&tm);
        let mut tid = self.terminal_id.as_bytes().to_vec();
        tid.resize(tid_len, 0x00);
        buf.extend_from_slice(&tid);
        buf.push(self.color);
        buf.extend_from_slice(&gbk_encode(&self.plate_number));
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
    fn decode(data: &[u8], _version: ProtocolVersion) -> Result<Self, String> {
        if data.len() < 3 {
            return Err("RegisterResponse: too short".into());
        }
        let response_serial_no = u16::from_be_bytes([data[0], data[1]]);
        let result = data[2];
        let auth_code = String::from_utf8_lossy(&data[3..]).trim_end_matches('\0').to_string();
        Ok(Self { response_serial_no, result, auth_code })
    }
    fn encode(&self, _version: ProtocolVersion) -> Vec<u8> {
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
    /// 2019 only: 终端 IMEI, 15 bytes (fixed)
    #[serde(default)]
    pub imei: String,
    /// 2019 only: 软件版本号, 20 bytes (fixed)
    #[serde(default)]
    pub software_version: String,
}

/// Encode a string into a fixed-length zero-padded byte field
fn fixed_string(s: &str, len: usize) -> Vec<u8> {
    let mut buf = s.as_bytes().to_vec();
    buf.resize(len, 0x00);
    buf
}

/// Read a length-prefixed string (BYTE length + content)
fn read_len_prefixed_string(data: &[u8], pos: &mut usize) -> String {
    if *pos >= data.len() {
        return String::new();
    }
    let len = data[*pos] as usize;
    *pos += 1;
    if *pos + len > data.len() {
        return String::new();
    }
    let s = String::from_utf8_lossy(&data[*pos..*pos + len])
        .trim_end_matches('\0')
        .to_string();
    *pos += len;
    s
}

impl Jt808Message for TerminalAuth {
    fn msg_id() -> u16 {
        0x0102
    }
    fn decode(data: &[u8], version: ProtocolVersion) -> Result<Self, String> {
        match version {
            // 2019: 鉴权码长度(1) + 鉴权码 + 终端IMEI(15) + 软件版本号(20)
            ProtocolVersion::V2019 => {
                if data.is_empty() {
                    return Err("TerminalAuth: too short".into());
                }
                let len = data[0] as usize;
                let code_end = (1 + len).min(data.len());
                let auth_code = String::from_utf8_lossy(&data[1..code_end])
                    .trim_end_matches('\0')
                    .to_string();
                let mut pos = code_end;
                let imei = if data.len() >= pos + 15 {
                    let s = String::from_utf8_lossy(&data[pos..pos + 15])
                        .trim_end_matches('\0')
                        .to_string();
                    pos += 15;
                    s
                } else {
                    String::new()
                };
                let software_version = if data.len() >= pos + 20 {
                    String::from_utf8_lossy(&data[pos..pos + 20])
                        .trim_end_matches('\0')
                        .to_string()
                } else {
                    String::new()
                };
                Ok(Self {
                    auth_code,
                    imei,
                    software_version,
                })
            }
            // 2013: 鉴权码 STRING
            ProtocolVersion::V2013 | ProtocolVersion::GenericTcp => {
                let auth_code = String::from_utf8_lossy(data)
                    .trim_end_matches('\0')
                    .to_string();
                Ok(Self {
                    auth_code,
                    imei: String::new(),
                    software_version: String::new(),
                })
            }
        }
    }
    fn encode(&self, version: ProtocolVersion) -> Vec<u8> {
        match version {
            ProtocolVersion::V2019 => {
                let code = self.auth_code.as_bytes();
                let mut buf = Vec::with_capacity(1 + code.len() + 35);
                buf.push(code.len() as u8);
                buf.extend_from_slice(code);
                buf.extend_from_slice(&fixed_string(&self.imei, 15));
                buf.extend_from_slice(&fixed_string(&self.software_version, 20));
                buf
            }
            ProtocolVersion::V2013 | ProtocolVersion::GenericTcp => {
                self.auth_code.as_bytes().to_vec()
            }
        }
    }
}

// ==================== 0x0200: Location Report ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationReport {
    pub alarm_flags: u32,
    pub status: u32,
    pub latitude: u32,     // degrees * 10^6
    pub longitude: u32,    // degrees * 10^6
    pub altitude: u16,     // meters
    pub speed: u16,        // 0.1 km/h
    pub direction: u16,    // 0-359 degrees
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
    fn decode(data: &[u8], _version: ProtocolVersion) -> Result<Self, String> {
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
    fn encode(&self, _version: ProtocolVersion) -> Vec<u8> {
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
    fn decode(data: &[u8], _version: ProtocolVersion) -> Result<Self, String> {
        if data.len() < 5 {
            return Err("PlatformGeneralResponse: too short".into());
        }
        Ok(Self {
            response_serial_no: u16::from_be_bytes([data[0], data[1]]),
            response_msg_id: u16::from_be_bytes([data[2], data[3]]),
            result: data[4],
        })
    }
    fn encode(&self, _version: ProtocolVersion) -> Vec<u8> {
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
    fn decode(data: &[u8], _version: ProtocolVersion) -> Result<Self, String> {
        if data.is_empty() {
            return Err("ParameterSet: empty".into());
        }
        let total = data[0];
        let mut items = Vec::new();
        let mut pos = 1;

        // 参数项 = 参数ID(4) + 参数长度(1) + 参数值(n)（平台协议表 8-8）
        while pos + 5 <= data.len() {
            let param_id = u32::from_be_bytes([
                data[pos],
                data[pos + 1],
                data[pos + 2],
                data[pos + 3],
            ]);
            pos += 4;
            let param_len = data[pos] as usize;
            pos += 1;
            if pos + param_len > data.len() {
                break;
            }
            items.push(ParameterItem {
                param_id,
                param_value: data[pos..pos + param_len].to_vec(),
            });
            pos += param_len;
        }
        Ok(Self { total, items })
    }
    fn encode(&self, _version: ProtocolVersion) -> Vec<u8> {
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
    pub flag: u8, // 0=emergency, 1=normal, etc.
    pub text: String,
}

impl Jt808Message for TextMessage {
    fn msg_id() -> u16 {
        0x8300
    }
    fn decode(data: &[u8], _version: ProtocolVersion) -> Result<Self, String> {
        if data.is_empty() {
            return Err("TextMessage: empty".into());
        }
        let flag = data[0];
        let text = gbk_decode(&data[1..])
            .trim_end_matches('\0')
            .to_string();
        Ok(Self { flag, text })
    }
    fn encode(&self, _version: ProtocolVersion) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1 + self.text.len());
        buf.push(self.flag);
        buf.extend_from_slice(&gbk_encode(&self.text));
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
    fn decode(data: &[u8], _version: ProtocolVersion) -> Result<Self, String> {
        if data.is_empty() {
            return Err("VehicleControl: empty".into());
        }
        Ok(Self { control_flag: data[0] })
    }
    fn encode(&self, _version: ProtocolVersion) -> Vec<u8> {
        vec![self.control_flag]
    }
}

// ==================== 0x0107: Query Terminal Attribute Response ====================
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryTerminalAttrResponse {
    pub terminal_type: u16,
    pub manufacturer_id: String, // 5 bytes
    pub terminal_model: String,  // 2013: 20, 2019: 30
    pub terminal_id: String,     // 2013: 7, 2019: 30
    pub iccid: String,           // BCD[10]
    pub hardware_version: String,
    pub firmware_version: String,
    pub gnss_attr: u8,
    pub comm_attr: u8,
}

impl QueryTerminalAttrResponse {
    /// (terminal_model len, terminal_id len) for the given version
    pub fn field_lengths(version: ProtocolVersion) -> (usize, usize) {
        match version {
            ProtocolVersion::V2019 => (30, 30),
            _ => (20, 7),
        }
    }
}

impl Jt808Message for QueryTerminalAttrResponse {
    fn msg_id() -> u16 {
        0x0107
    }
    fn decode(data: &[u8], version: ProtocolVersion) -> Result<Self, String> {
        let (model_len, tid_len) = Self::field_lengths(version);
        let fixed = 2 + 5 + model_len + tid_len + 10;
        if data.len() < fixed {
            return Err("QueryTerminalAttrResponse: too short".into());
        }
        let terminal_type = u16::from_be_bytes([data[0], data[1]]);
        let manufacturer_id = String::from_utf8_lossy(&data[2..7])
            .trim_end_matches('\0')
            .to_string();
        let mut pos = 7;
        let terminal_model = String::from_utf8_lossy(&data[pos..pos + model_len])
            .trim_end_matches('\0')
            .to_string();
        pos += model_len;
        let terminal_id = String::from_utf8_lossy(&data[pos..pos + tid_len])
            .trim_end_matches('\0')
            .to_string();
        pos += tid_len;
        let iccid = types::bcd_to_string(&data[pos..pos + 10]);
        pos += 10;
        let hardware_version = read_len_prefixed_string(data, &mut pos);
        let firmware_version = read_len_prefixed_string(data, &mut pos);
        let gnss_attr = if pos < data.len() {
            let v = data[pos];
            pos += 1;
            v
        } else {
            0
        };
        let comm_attr = if pos < data.len() { data[pos] } else { 0 };
        Ok(Self {
            terminal_type,
            manufacturer_id,
            terminal_model,
            terminal_id,
            iccid,
            hardware_version,
            firmware_version,
            gnss_attr,
            comm_attr,
        })
    }
    fn encode(&self, version: ProtocolVersion) -> Vec<u8> {
        let (model_len, tid_len) = Self::field_lengths(version);
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.terminal_type.to_be_bytes());
        buf.extend_from_slice(&fixed_string(&self.manufacturer_id, 5));
        buf.extend_from_slice(&fixed_string(&self.terminal_model, model_len));
        buf.extend_from_slice(&fixed_string(&self.terminal_id, tid_len));
        buf.extend_from_slice(&types::string_to_bcd(&self.iccid, 10));
        let hw = self.hardware_version.as_bytes();
        buf.push(hw.len() as u8);
        buf.extend_from_slice(hw);
        let fw = self.firmware_version.as_bytes();
        buf.push(fw.len() as u8);
        buf.extend_from_slice(fw);
        buf.push(self.gnss_attr);
        buf.push(self.comm_attr);
        buf
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
    QueryTerminalAttrResponse(QueryTerminalAttrResponse),
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
            Self::QueryTerminalAttrResponse(_) => 0x0107,
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
            Self::QueryTerminalAttrResponse(_) => "查询终端属性应答",
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
            Self::QueryTerminalAttrResponse(r) => {
                format!("型号:{} 终端ID:{}", r.terminal_model, r.terminal_id)
            }
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

/// Parse a message body given the protocol version and msg_id
pub fn parse_message(version: ProtocolVersion, msg_id: u16, body: &[u8]) -> ParsedMessage {
    macro_rules! try_parse {
        ($ty:ty, $variant:ident) => {
            match <$ty as Jt808Message>::decode(body, version) {
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
        0x0107 => try_parse!(QueryTerminalAttrResponse, QueryTerminalAttrResponse),
        0x8001 => try_parse!(PlatformGeneralResponse, PlatformGeneralResponse),
        0x8100 => try_parse!(RegisterResponse, RegisterResponse),
        0x0200 => try_parse!(LocationReport, LocationReport),
        0x8103 => try_parse!(ParameterSet, ParameterSet),
        0x8300 => try_parse!(TextMessage, TextMessage),
        0x8500 => try_parse!(VehicleControl, VehicleControl),
        _ => ParsedMessage::Unknown { msg_id, body: body.to_vec() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_roundtrip_2013() {
        let reg = TerminalRegister {
            province_id: 31,
            city_id: 1,
            manufacturer_id: "MFG01".into(),
            terminal_model: "MODEL01".into(),
            terminal_id: "TID0001".into(),
            color: 1,
            plate_number: "京A88888".into(),
        };
        let body = reg.encode(ProtocolVersion::V2013);
        let decoded = TerminalRegister::decode(&body, ProtocolVersion::V2013).unwrap();
        assert_eq!(decoded.manufacturer_id, "MFG01");
        assert_eq!(decoded.terminal_model, "MODEL01");
        assert_eq!(decoded.terminal_id, "TID0001");
        assert_eq!(decoded.plate_number, "京A88888");
    }

    #[test]
    fn test_register_roundtrip_2019() {
        let reg = TerminalRegister {
            province_id: 31,
            city_id: 1,
            manufacturer_id: "MFG2019ABCD".into(),
            terminal_model: "TERMINAL-MODEL-2019".into(),
            terminal_id: "013900000001".into(),
            color: 1,
            plate_number: "京A88888".into(),
        };
        let body = reg.encode(ProtocolVersion::V2019);
        let (mfg, model, tid) = TerminalRegister::field_lengths(ProtocolVersion::V2019);
        assert_eq!(
            body.len(),
            4 + mfg + model + tid + 1 + gbk_encode("京A88888").len()
        );
        let decoded = TerminalRegister::decode(&body, ProtocolVersion::V2019).unwrap();
        assert_eq!(decoded.manufacturer_id, "MFG2019ABCD");
        assert_eq!(decoded.terminal_model, "TERMINAL-MODEL-2019");
        assert_eq!(decoded.terminal_id, "013900000001");
        assert_eq!(decoded.plate_number, "京A88888");
    }

    #[test]
    fn test_auth_2013_vs_2019() {
        let auth = TerminalAuth {
            auth_code: "AUTH1234".into(),
            imei: "861234567890123".into(),
            software_version: "V1.0.0".into(),
        };
        let body_2013 = auth.encode(ProtocolVersion::V2013);
        assert_eq!(body_2013, b"AUTH1234");
        let body_2019 = auth.encode(ProtocolVersion::V2019);
        assert_eq!(body_2019[0], 8);
        assert_eq!(&body_2019[1..9], b"AUTH1234");
        assert_eq!(body_2019.len(), 1 + 8 + 15 + 20);
        assert_eq!(&body_2019[9..24], b"861234567890123");
        assert_eq!(&body_2019[24..30], b"V1.0.0");

        let decoded_2019 = TerminalAuth::decode(&body_2019, ProtocolVersion::V2019).unwrap();
        assert_eq!(decoded_2019.auth_code, "AUTH1234");
        assert_eq!(decoded_2019.imei, "861234567890123");
        assert_eq!(decoded_2019.software_version, "V1.0.0");
        let decoded_2013 = TerminalAuth::decode(&body_2013, ProtocolVersion::V2013).unwrap();
        assert_eq!(decoded_2013.auth_code, "AUTH1234");
    }

    #[test]
    fn test_parse_platform_2019_response_frame() {
        // Real frame from the platform (from server log):
        // 7e 80014005010000006131626360000100030027090000d8 7e
        let frame = [
            0x7e, 0x80, 0x01, 0x40, 0x05, 0x01, 0x00, 0x00, 0x00, 0x61, 0x31, 0x62, 0x63,
            0x60, 0x00, 0x01, 0x00, 0x03, 0x00, 0x27, 0x09, 0x00, 0x00, 0xd8, 0x7e,
        ];
        let (msg, _rest) = crate::protocol::codec::decode_message(&frame).unwrap();
        let (header, header_len) = types::MsgHeader::decode(&msg).unwrap();
        assert_eq!(header.protocol_version(), ProtocolVersion::V2019);
        assert_eq!(header.terminal_id, "61316263600001");
        assert_eq!(header.serial_no, 3);
        let body = &msg[header_len..];
        match parse_message(header.protocol_version(), header.msg_id, body) {
            ParsedMessage::PlatformGeneralResponse(r) => {
                assert_eq!(r.response_serial_no, 39);
                assert_eq!(r.response_msg_id, 0x0900);
                assert_eq!(r.result, 0);
            }
            other => panic!("expected PlatformGeneralResponse, got {}", other.name()),
        }
    }

    #[test]
    fn test_query_terminal_attr_roundtrip_2019() {
        let attr = QueryTerminalAttrResponse {
            terminal_type: 0x0001,
            manufacturer_id: "MFG01".into(),
            terminal_model: "MODEL-2019".into(),
            terminal_id: "013900000001".into(),
            iccid: "89860012345678901234".into(),
            hardware_version: "HW1.0".into(),
            firmware_version: "FW2.0".into(),
            gnss_attr: 0x03,
            comm_attr: 0x20,
        };
        let body = attr.encode(ProtocolVersion::V2019);
        // 2 + 5 + 30 + 30 + 10 + (1+5) + (1+5) + 1 + 1
        assert_eq!(body.len(), 2 + 5 + 30 + 30 + 10 + 6 + 6 + 1 + 1);
        let decoded = QueryTerminalAttrResponse::decode(&body, ProtocolVersion::V2019).unwrap();
        assert_eq!(decoded.terminal_model, "MODEL-2019");
        assert_eq!(decoded.terminal_id, "013900000001");
        assert_eq!(decoded.iccid, "89860012345678901234");
        assert_eq!(decoded.hardware_version, "HW1.0");
        assert_eq!(decoded.firmware_version, "FW2.0");
        assert_eq!(decoded.gnss_attr, 0x03);
        assert_eq!(decoded.comm_attr, 0x20);
    }

    #[test]
    fn test_parse_message_2019() {
        let reg = TerminalRegister {
            province_id: 31,
            city_id: 1,
            manufacturer_id: "MFG".into(),
            terminal_model: "MODEL".into(),
            terminal_id: "013900000001".into(),
            color: 1,
            plate_number: "京A88888".into(),
        };
        let body = reg.encode(ProtocolVersion::V2019);
        let parsed = parse_message(ProtocolVersion::V2019, 0x0100, &body);
        assert_eq!(parsed.name(), "终端注册");
    }
}
