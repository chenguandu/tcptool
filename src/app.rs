use std::collections::HashMap;

use eframe::egui;
use egui::{CentralPanel, SidePanel, TopBottomPanel};

use crate::connection::{self, ConnEvent, ConnState, ConnectionConfig, TcpConnectionHandle};
use crate::db::Database;
use crate::protocol::builder;
use crate::protocol::message::{self, Jt808Message, ParsedMessage};
use crate::protocol::types::{MsgHeader, ProtocolVersion};

/// A single message entry in the stream
#[derive(Debug, Clone)]
pub struct MessageEntry {
    pub direction: Direction,
    pub raw_hex: String,
    pub raw_bytes: Vec<u8>,
    pub parsed: Option<ParsedMessage>,
    pub timestamp: chrono::NaiveDateTime,
    pub encoding: String,
    /// JT808 protocol version (None for non-JT808 / unknown frames)
    pub protocol_version: Option<ProtocolVersion>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Direction {
    Send,
    Receive,
}

impl Direction {
    pub fn arrow(&self) -> &'static str {
        match self {
            Self::Send => "→",
            Self::Receive => "←",
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Send => "发送",
            Self::Receive => "接收",
        }
    }
}

/// Encoding options
#[derive(Debug, Clone, PartialEq)]
pub enum Encoding {
    GBK,
    UTF8,
    ASCII,
    HEX,
}

impl Encoding {
    pub fn all() -> &'static [Encoding] {
        &[Self::GBK, Self::UTF8, Self::ASCII, Self::HEX]
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::GBK => "GBK",
            Self::UTF8 => "UTF-8",
            Self::ASCII => "ASCII",
            Self::HEX => "HEX",
        }
    }
}

/// A custom quick command
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuickCommand {
    pub id: String,
    pub name: String,
    /// JT808 message ID (0 for raw hex)
    pub msg_id: u16,
    /// Raw hex data to send
    pub raw_hex: String,
}

impl QuickCommand {
    pub fn new(name: &str, raw_hex: &str) -> Self {
        Self {
            id: format!("cmd-{:x}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()),
            name: name.to_string(),
            msg_id: 0,
            raw_hex: raw_hex.to_string(),
        }
    }
}

/// Main application state
pub struct TcpToolApp {
    /// List of connection configs (the source of truth for the list UI)
    pub connections: Vec<ConnectionConfig>,
    /// Index of selected connection
    pub selected_conn: Option<usize>,
    /// Active connection handles (by connection id)
    pub handles: HashMap<String, TcpConnectionHandle>,
    /// Connection states (by connection id)
    pub conn_states: HashMap<String, ConnState>,
    /// Message stream for each connection (by connection id)
    pub messages: HashMap<String, Vec<MessageEntry>>,
    /// Selected message index (by connection id)
    pub selected_msg: HashMap<String, usize>,
    /// Current encoding setting
    pub encoding: Encoding,
    /// Default protocol version used for new connections / when none is selected
    pub default_protocol_version: ProtocolVersion,
    /// New connection dialog state
    pub show_new_conn_dialog: bool,
    /// Index of connection being edited (None = new connection)
    pub editing_conn_idx: Option<usize>,
    /// Editing connection config
    pub edit_conn: ConnectionConfig,
    /// Receiver for connection events
    event_rx: tokio::sync::mpsc::UnboundedReceiver<(String, ConnEvent)>,
    /// Sender for connection events (cloneable)
    event_tx: tokio::sync::mpsc::UnboundedSender<(String, ConnEvent)>,
    /// Toolbar text input for sending raw data
    pub send_text: String,
    /// Per-connection serial number counter
    pub serial_numbers: HashMap<String, u16>,
    /// Per-connection last heartbeat time (seconds since app start)
    pub last_heartbeat: HashMap<String, f64>,
    /// Per-connection heartbeat enabled flag
    pub heartbeat_enabled: HashMap<String, bool>,
    /// App start time for relative timing
    app_start: std::time::Instant,
    /// Database manager
    pub db: Option<Database>,
    /// Whether app has been initialized
    initialized: bool,
    /// Export dialog visibility
    show_export_dialog: bool,
    /// Custom quick commands
    pub custom_commands: Vec<QuickCommand>,
    /// Quick command dialog
    show_cmd_dialog: bool,
    edit_cmd_index: Option<usize>,
    edit_cmd: QuickCommand,
    /// 快捷指令编辑框中的消息ID文本（十六进制，空/0 表示完整帧或原始数据）
    edit_cmd_msg_id_text: String,
    /// Toast notification timestamp
    toast_time: Option<std::time::Instant>,
}

impl Default for TcpToolApp {
    fn default() -> Self {
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            connections: Vec::new(),
            selected_conn: None,
            handles: HashMap::new(),
            conn_states: HashMap::new(),
            messages: HashMap::new(),
            selected_msg: HashMap::new(),
            encoding: Encoding::HEX,
            default_protocol_version: ProtocolVersion::V2013,
            show_new_conn_dialog: false,
            editing_conn_idx: None,
            edit_conn: ConnectionConfig::default(),
            event_rx,
            event_tx,
            send_text: String::new(),
            serial_numbers: HashMap::new(),
            last_heartbeat: HashMap::new(),
            heartbeat_enabled: HashMap::new(),
            app_start: std::time::Instant::now(),
            db: None,
            initialized: false,
            show_export_dialog: false,
            custom_commands: Vec::new(),
            show_cmd_dialog: false,
            edit_cmd_index: None,
            edit_cmd: QuickCommand::new("", ""),
            edit_cmd_msg_id_text: String::new(),
            toast_time: None,
        }
    }
}

impl TcpToolApp {
    /// Process pending connection events
    fn process_events(&mut self) {
        while let Ok((conn_id, event)) = self.event_rx.try_recv() {
            match event {
                ConnEvent::Connected => {
                    self.conn_states.insert(conn_id.clone(), ConnState::Connected);
                }
                ConnEvent::Disconnected(reason) => {
                    self.conn_states.insert(conn_id.clone(), ConnState::Disconnected);
                    self.add_message(&conn_id, Direction::Receive, Vec::new(), Some(format!("断开: {}", reason)));
                }
                ConnEvent::DataReceived(data) => {
                    self.add_received_data(&conn_id, data);
                }
                ConnEvent::Error(msg) => {
                    self.conn_states.insert(conn_id.clone(), ConnState::Disconnected);
                    self.add_message(&conn_id, Direction::Receive, Vec::new(), Some(msg));
                }
            }
        }
    }

    /// Add a received data message to the stream
    fn add_received_data(&mut self, conn_id: &str, data: Vec<u8>) {
        let conn_version = self.conn_protocol_version(conn_id);
        // Try to parse as JT808 message
        let mut parsed = None;
        let mut protocol_version = None;
        if conn_version.is_generic() {
            // 通用TCP：原样显示，不做 JT808 解析
            protocol_version = Some(ProtocolVersion::GenericTcp);
        } else if !data.is_empty() && data[0] == 0x7E {
            // Try to decode JT808 frame
            if let Some((msg_bytes, _rest)) = crate::protocol::codec::decode_message(&data) {
                match MsgHeader::decode(&msg_bytes) {
                    Ok((header, header_len)) => {
                        // The version flag in the header is authoritative:
                        // 2019 headers carry a version byte, 2013 headers do not.
                        let version = header.protocol_version();
                        protocol_version = Some(version);
                        let body = if msg_bytes.len() > header_len {
                            &msg_bytes[header_len..]
                        } else {
                            &[]
                        };
                        parsed = Some(message::parse_message(version, header.msg_id, body));
                    }
                    Err(e) => {
                        log::warn!("Failed to decode JT808 header: {}", e);
                    }
                }
            }
        }

        let parsed_clone = parsed.clone();
        let hex_str = data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
        let entry = MessageEntry {
            direction: Direction::Receive,
            raw_hex: hex_str,
            raw_bytes: data,
            parsed,
            timestamp: chrono::Local::now().naive_local(),
            encoding: self.encoding.name().to_string(),
            protocol_version,
        };
        self.messages.entry(conn_id.to_string()).or_default().push(entry);

        // 自动传递：把 bit14 识别出的协议版本同步到连接配置和运行中的连接任务，
        // 后续自动心跳/鉴权/位置汇报等均自动使用该版本
        if let Some(version) = protocol_version {
            self.sync_protocol_version(conn_id, version);
        }

        // Auto handle registration response: send auth
        if let Some(ParsedMessage::RegisterResponse(ref resp)) = parsed_clone {
            if resp.result == 0 {
                // 鉴权使用触发它的注册应答报文版本（bit14 识别结果）
                let version = protocol_version
                    .unwrap_or_else(|| self.conn_protocol_version(conn_id));
                self.handle_register_response(conn_id, &resp.auth_code, version);
            }
        }
    }

    /// Add a text message (for non-data events)
    fn add_message(&mut self, conn_id: &str, direction: Direction, data: Vec<u8>, text: Option<String>) {
        let hex_str = if !data.is_empty() {
            data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
        } else {
            text.unwrap_or_default()
        };
        let entry = MessageEntry {
            direction,
            raw_hex: hex_str,
            raw_bytes: data,
            parsed: None,
            timestamp: chrono::Local::now().naive_local(),
            encoding: self.encoding.name().to_string(),
            protocol_version: None,
        };
        self.messages.entry(conn_id.to_string()).or_default().push(entry);
    }

    /// Send raw bytes on the selected connection
    fn send_raw(&mut self, data: Vec<u8>) {
        if let Some(idx) = self.selected_conn {
            if idx < self.connections.len() {
                let conn_id = self.connections[idx].id.clone();
                let proto = self.connections[idx].protocol_version;
                if let Some(handle) = self.handles.get(&conn_id) {
                    handle.send(data.clone());
                    let hex_str = data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
                    let entry = MessageEntry {
                        direction: Direction::Send,
                        raw_hex: hex_str,
                        raw_bytes: data,
                        parsed: None,
                        timestamp: chrono::Local::now().naive_local(),
                        encoding: self.encoding.name().to_string(),
                        protocol_version: Some(proto),
                    };
                    self.messages.entry(conn_id).or_default().push(entry);
                }
            }
        }
    }

    /// Protocol version of the selected connection (or the app default)
    fn current_protocol_version(&self) -> ProtocolVersion {
        self.selected_conn
            .and_then(|idx| self.connections.get(idx))
            .map(|c| c.protocol_version)
            .unwrap_or(self.default_protocol_version)
    }

    /// Protocol version configured for a specific connection (or the app default)
    fn conn_protocol_version(&self, conn_id: &str) -> ProtocolVersion {
        self.connections
            .iter()
            .find(|c| c.id == conn_id)
            .map(|c| c.protocol_version)
            .unwrap_or(self.default_protocol_version)
    }

    /// 自动传递：把从报文头 bit14 识别出的协议版本同步到对应连接、
    /// 运行中的连接任务（自动心跳）以及持久化配置。
    fn sync_protocol_version(&mut self, conn_id: &str, version: ProtocolVersion) {
        let changed = self
            .connections
            .iter()
            .find(|c| c.id == conn_id)
            .map(|c| c.protocol_version != version)
            .unwrap_or(false);
        if !changed {
            return;
        }

        let mut updated_config = None;
        if let Some(conn) = self.connections.iter_mut().find(|c| c.id == conn_id) {
            conn.protocol_version = version;
            updated_config = Some(conn.clone());
        }
        if let Some(config) = updated_config {
            if let Some(ref db) = self.db {
                let _ = db.save_connection(&config);
            }
        }
        if let Some(handle) = self.handles.get(conn_id) {
            handle.set_protocol_version(version);
        }
        self.default_protocol_version = version;
        if let Some(ref db) = self.db {
            let _ = db.set_setting("protocol_version", version.short_name());
        }

        self.add_message(
            conn_id,
            Direction::Receive,
            Vec::new(),
            Some(format!(
                "ℹ️ 报文头 bit14 指示协议版本为 {}，已自动切换并传递到后续发送",
                version.name()
            )),
        );
    }

    /// Get next serial number for a connection
    fn next_serial(&mut self, conn_id: &str) -> u16 {
        let entry = self.serial_numbers.entry(conn_id.to_string()).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Send a JT808 auth message on the selected connection
    fn send_auth(&mut self) {
        // Clone the data we need first to avoid borrow conflicts
        let info = self.selected_conn.and_then(|idx| {
            self.connections.get(idx).map(|c| {
                (
                    c.terminal_phone.clone(),
                    c.auth_code.clone(),
                    c.id.clone(),
                    c.protocol_version,
                    c.imei.clone(),
                    c.software_version.clone(),
                )
            })
        });
        if let Some((phone, auth_code, conn_id, version, imei, software_version)) = info {
            if auth_code.is_empty() {
                self.add_message(&conn_id, Direction::Send, Vec::new(),
                    Some("⚠️ 鉴权码为空！请在连接配置中填写鉴权码，或先执行注册流程获取鉴权码".into()));
                return;
            }
            let serial = self.next_serial(&conn_id);
            let frame = builder::build_auth(
                version,
                &phone,
                serial,
                &auth_code,
                &imei,
                &software_version,
            );
            self.send_raw(frame);
        }
    }

    /// Send a JT808 location report on the selected connection
    fn send_location_report(&mut self) {
        let info = self.selected_conn.and_then(|idx| {
            self.connections.get(idx).map(|c| (c.terminal_phone.clone(), c.id.clone(), c.protocol_version))
        });
        if let Some((phone, conn_id, version)) = info {
            let serial = self.next_serial(&conn_id);
            let frame = builder::build_location_report(
                version,
                &phone, serial,
                116397428, // lat * 10^6
                39916527,  // lng * 10^6
                50, 0, 0,
            );
            self.send_raw(frame);
        }
    }

    /// Send a JT808 heartbeat on the selected connection
    fn send_heartbeat(&mut self) {
        let info = {
            let conn_id = self.current_conn_id();
            conn_id.as_ref().and_then(|id| {
                self.connections.iter().find(|c| c.id == *id)
                    .map(|c| (c.terminal_phone.clone(), id.clone(), c.protocol_version))
            })
        };
        if let Some((phone, conn_id, version)) = info {
            let serial = self.next_serial(&conn_id);
            let frame = builder::build_heartbeat(version, &phone, serial);
            if let Some(handle) = self.handles.get(&conn_id) {
                handle.send(frame.clone());
            }
            let hex_str = frame.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
            let entry = MessageEntry {
                direction: Direction::Send,
                raw_hex: hex_str,
                raw_bytes: frame,
                parsed: Some(ParsedMessage::Heartbeat(crate::protocol::message::Heartbeat)),
                timestamp: chrono::Local::now().naive_local(),
                encoding: "JT808".to_string(),
                protocol_version: Some(version),
            };
            self.messages.entry(conn_id.clone()).or_default().push(entry);
            self.last_heartbeat.insert(conn_id, self.elapsed_secs());
        }
    }

    /// Run the terminal registration flow on the selected connection
    fn run_registration_flow(&mut self) {
        let info = {
            let conn_id = self.current_conn_id();
            conn_id.as_ref().and_then(|id| {
                self.connections.iter().find(|c| c.id == *id)
                    .map(|c| (c.terminal_phone.clone(), id.clone(), c.protocol_version))
            })
        };
        if let Some((phone, conn_id, version)) = info {
            if let Some(handle) = self.handles.get(&conn_id) {
                let serial = 1u16;
                // Step 1: Send register
                let reg_frame = builder::build_register(
                    version,
                    &phone, serial,
                    31, 1, "MFG01", "JT808", &phone, 1, "测试车辆",
                );
                handle.send(reg_frame.clone());
                let hex = reg_frame.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
                self.messages.entry(conn_id.clone()).or_default().push(MessageEntry {
                    direction: Direction::Send,
                    raw_hex: hex,
                    raw_bytes: reg_frame,
                    parsed: None,
                    timestamp: chrono::Local::now().naive_local(),
                    encoding: "JT808".to_string(),
                    protocol_version: Some(version),
                });

                // Note: After receiving the register response (0x8100), the app will need
                // to send auth (0x0102). This is handled automatically by add_received_data
                // when it detects a registration response.
                self.serial_numbers.insert(conn_id, 1);
            }
        }
    }

    /// Handle auto-registration response - called when we receive a 0x8100
    fn handle_register_response(&mut self, conn_id: &str, auth_code: &str, version: ProtocolVersion) {
        let info = self.connections.iter().find(|c| c.id == conn_id)
            .map(|c| (c.terminal_phone.clone(), c.imei.clone(), c.software_version.clone()));
        if let Some((phone, imei, software_version)) = info {
            let serial = self.next_serial(conn_id);
            let auth_frame = builder::build_auth(
                version,
                &phone,
                serial,
                auth_code,
                &imei,
                &software_version,
            );
            if let Some(handle) = self.handles.get(conn_id) {
                handle.send(auth_frame.clone());
            }
            let hex = auth_frame.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
            self.messages.entry(conn_id.to_string()).or_default().push(MessageEntry {
                direction: Direction::Send,
                raw_hex: hex,
                raw_bytes: auth_frame,
                parsed: None,
                timestamp: chrono::Local::now().naive_local(),
                encoding: "JT808".to_string(),
                protocol_version: Some(version),
            });
            // Save auth_code back to connection config so "鉴权" button works later
            if let Some(conn) = self.connections.iter_mut().find(|c| c.id == conn_id) {
                conn.auth_code = auth_code.to_string();
            }
            // Enable heartbeat after successful auth
            self.heartbeat_enabled.insert(conn_id.to_string(), true);
            self.last_heartbeat.insert(conn_id.to_string(), self.elapsed_secs());
        }
    }

    fn elapsed_secs(&self) -> f64 {
        self.app_start.elapsed().as_secs_f64()
    }

    /// Check heartbeats - call every frame
    fn check_heartbeats(&mut self) {
        for conn in &self.connections.clone() {
            let conn_id = conn.id.clone();
            if self.conn_states.get(&conn_id) != Some(&ConnState::Connected) {
                continue;
            }
            if !self.heartbeat_enabled.get(&conn_id).copied().unwrap_or(false) {
                continue;
            }
            // 通用TCP模式不发送 JT808 心跳
            if !conn.protocol_version.is_jt808() {
                continue;
            }
            let interval = (conn.heartbeat_interval_secs as f64).max(5.0);
            let last = self.last_heartbeat.get(&conn_id).copied().unwrap_or(0.0);
            if self.elapsed_secs() - last >= interval {
                // Send heartbeat for this connection
                if let Some(handle) = self.handles.get(&conn_id) {
                    let serial = self.serial_numbers.entry(conn_id.clone()).or_insert(0);
                    *serial += 1;
                    let frame = builder::build_heartbeat(conn.protocol_version, &conn.terminal_phone, *serial);
                    handle.send(frame.clone());
                    let hex = frame.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
                    self.messages.entry(conn_id.clone()).or_default().push(MessageEntry {
                        direction: Direction::Send,
                        raw_hex: hex,
                        raw_bytes: frame,
                        parsed: Some(ParsedMessage::Heartbeat(crate::protocol::message::Heartbeat)),
                        timestamp: chrono::Local::now().naive_local(),
                        encoding: "JT808".to_string(),
                        protocol_version: Some(conn.protocol_version),
                    });
                    self.last_heartbeat.insert(conn_id.clone(), self.elapsed_secs());
                }
            }
        }
    }

    fn current_conn_id(&self) -> Option<String> {
        self.selected_conn.and_then(|idx| {
            if idx < self.connections.len() {
                Some(self.connections[idx].id.clone())
            } else {
                None
            }
        })
    }

    fn current_conn_state(&self) -> Option<ConnState> {
        self.current_conn_id().and_then(|id| self.conn_states.get(&id).cloned())
    }

    fn current_messages(&self) -> &[MessageEntry] {
        self.current_conn_id()
            .and_then(|id| self.messages.get(&id))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    fn current_selected_msg(&self) -> Option<&MessageEntry> {
        self.current_conn_id().and_then(|id| {
            self.selected_msg.get(&id).and_then(|&idx| {
                self.messages.get(&id).and_then(|msgs| msgs.get(idx))
            })
        })
    }
}

impl eframe::App for TcpToolApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Initialize on first frame
        if !self.initialized {
            self.initialized = true;
            if let Ok(db) = Database::open("tcptool.db") {
                // Load saved connections
                if let Ok(conns) = db.load_connections() {
                    for config in conns {
                        let conn_id = config.id.clone();
                        let handle = connection::start_connection(config.clone(), self.event_tx.clone());
                        self.conn_states.insert(conn_id.clone(), ConnState::Disconnected);
                        self.handles.insert(conn_id, handle);
                    }
                    self.connections = db.load_connections().unwrap_or_default();
                }
                // Load custom quick commands from DB
                if let Ok(cmds) = db.load_quick_commands() {
                    for (id, name, msg_id, raw_hex, _sort) in cmds {
                        self.custom_commands.push(QuickCommand {
                            id,
                            name,
                            msg_id,
                            raw_hex,
                        });
                    }
                }
                // Load last used protocol version
                if let Ok(Some(version)) = db.get_setting("protocol_version") {
                    self.default_protocol_version = ProtocolVersion::from_str_loose(&version);
                }
                self.db = Some(db);
            }
        }

        // Process connection events first
        self.process_events();
        // Check and send heartbeats
        self.check_heartbeats();
        // Check if toast should still be shown
        let show_toast = self.toast_time.map_or(false, |t| t.elapsed().as_secs_f64() < 2.0);

        // ── Top toolbar ──
        TopBottomPanel::top("toolbar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                if ui.button("➕ 新建连接").clicked() {
                    self.edit_conn = ConnectionConfig::default();
                    self.edit_conn.protocol_version = self.default_protocol_version;
                    self.show_new_conn_dialog = true;
                }
                ui.separator();

                // Connection action buttons
                let state = self.current_conn_state();
                match state {
                    Some(ConnState::Connected) => {
                        if ui.button("⏹ 断开").clicked() {
                            if let Some(id) = self.current_conn_id() {
                                if let Some(h) = self.handles.get(&id) {
                                    h.disconnect();
                                }
                            }
                        }
                    }
                    Some(ConnState::Disconnected) | None => {
                        if ui.button("🔗 连接").clicked() {
                            if let Some(id) = self.current_conn_id() {
                                if let Some(h) = self.handles.get(&id) {
                                    h.connect();
                                }
                            }
                        }
                    }
                    _ => {}
                }

                ui.separator();
                // Encoding selector
                ui.label("编码:");
                egui::ComboBox::from_id_salt("encoding")
                    .selected_text(self.encoding.name())
                    .show_ui(ui, |ui| {
                        for enc in Encoding::all() {
                            ui.selectable_value(&mut self.encoding, enc.clone(), enc.name());
                        }
                    });
                ui.separator();

                // Protocol version selector (applies to the selected connection)
                ui.label("协议:");
                let current_version = self.current_protocol_version();
                let mut selected_version = current_version;
                egui::ComboBox::from_id_salt("protocol_version")
                    .selected_text(current_version.name())
                    .show_ui(ui, |ui| {
                        for v in ProtocolVersion::all() {
                            ui.selectable_value(&mut selected_version, *v, v.name());
                        }
                    })
                    .response
                    .on_hover_text(
                        "JT808-2013/2019：发送按所选版本组帧，接收按 bit14 自动识别并自动切换\n\
                         通用TCP：发送内容原样发出，不解析、不组帧",
                    );
                if selected_version != current_version {
                    if let Some(idx) = self.selected_conn {
                        if idx < self.connections.len() {
                            self.connections[idx].protocol_version = selected_version;
                            let config = self.connections[idx].clone();
                            if let Some(ref db) = self.db {
                                let _ = db.save_connection(&config);
                            }
                            if let Some(handle) = self.handles.get(&config.id) {
                                handle.set_protocol_version(selected_version);
                            }
                        }
                    }
                    self.default_protocol_version = selected_version;
                    if let Some(ref db) = self.db {
                        let _ = db.set_setting("protocol_version", selected_version.short_name());
                    }
                }
                ui.separator();

                if ui.button("清空日志").clicked() {
                    if let Some(id) = self.current_conn_id() {
                        self.messages.get_mut(&id).map(|m| m.clear());
                    }
                }
                if ui.button("📤 导出").clicked() {
                    self.show_export_dialog = true;
                }
            });
        });

        // ── Left panel: Connection list ──
        SidePanel::left("left_panel")
            .resizable(true)
            .default_width(250.0)
            .min_width(180.0)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.heading("📡 连接列表");
                    ui.separator();
                    ui.add_space(4.0);

                    if self.connections.is_empty() {
                        ui.label("(暂无连接)");
                        ui.add_space(4.0);
                        if ui.button("新建连接").clicked() {
                            self.edit_conn = ConnectionConfig::default();
                            self.edit_conn.protocol_version = self.default_protocol_version;
                            self.show_new_conn_dialog = true;
                        }
                    } else {
                        let mut to_delete: Option<usize> = None;
                        for (i, conn) in self.connections.iter().enumerate() {
                            let state = self.conn_states.get(&conn.id).cloned().unwrap_or(ConnState::Disconnected);
                            let selected = self.selected_conn == Some(i);
                            let color = state.color();
                            let label = format!("{} [{}:{}]", conn.name, conn.host, conn.port);

                            ui.horizontal(|ui| {
                                // Status indicator dot
                                egui::Frame::NONE.fill(color).corner_radius(4.0).show(ui, |ui| {
                                    ui.set_min_size(egui::vec2(8.0, 8.0));
                                });
                                let response = ui.selectable_label(selected, label);
                                if response.clicked() {
                                    self.selected_conn = Some(i);
                                }
                                if response.double_clicked() {
                                    // Double-click to auto-connect
                                    if let Some(h) = self.handles.get(&conn.id) {
                                        h.connect();
                                    }
                                }
                                // Right-click context menu
                                response.context_menu(|ui| {
                                    if ui.button("📝编辑").clicked() {
                                        self.edit_conn = self.connections[i].clone();
                                        self.editing_conn_idx = Some(i);
                                        self.show_new_conn_dialog = true;
                                        ui.close_menu();
                                    }
                                    if ui.button("✕删除").clicked() {
                                        to_delete = Some(i);
                                        ui.close_menu();
                                    }
                                });
                            });
                        }
                        if let Some(idx) = to_delete {
                            let conn_id = self.connections[idx].id.clone();
                            // Disconnect and remove handle
                            if let Some(h) = self.handles.remove(&conn_id) {
                                h.disconnect();
                            }
                            // Delete from database
                            if let Some(ref db) = self.db {
                                let _ = db.delete_connection(&conn_id);
                            }
                            self.conn_states.remove(&conn_id);
                            self.messages.remove(&conn_id);
                            self.connections.remove(idx);
                            if self.selected_conn == Some(idx) {
                                self.selected_conn = None;
                            } else if let Some(sel) = self.selected_conn {
                                if sel > idx {
                                    self.selected_conn = Some(sel - 1);
                                }
                            }
                        }
                    }
                });
            });

        // ── Right panel: Details + quick commands ──
        SidePanel::right("right_panel")
            .resizable(true)
            .default_width(320.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            ui.heading("📄 报文详情");
                            ui.separator();
                            ui.add_space(4.0);

                            if let Some(msg) = self.current_selected_msg() {
                                let raw_hex = msg.raw_hex.clone();
                                let ts = msg.timestamp.format("%H:%M:%S").to_string();
                                let dir_arrow = msg.direction.arrow().to_string();
                                let dir_name = msg.direction.name().to_string();
                                let encoding = msg.encoding.clone();
                                let protocol_version = msg.protocol_version;
                                let parsed = msg.parsed.clone();
                                ui.label(format!("时间: {}", ts));
                                ui.label(format!("方向: {} {}", dir_arrow, dir_name));
                                ui.label(format!("编码: {}", encoding));
                                if let Some(pv) = protocol_version {
                                    ui.label(format!("协议: {}", pv.name()));
                                }
                                ui.separator();
                                ui.horizontal(|ui| {
                                    ui.label("原始 HEX:");
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button("复制").clicked() {
                                            ui.ctx().copy_text(raw_hex.clone());
                                            self.toast_time = Some(std::time::Instant::now());
                                        }
                                    });
                                });
                                egui::Frame::group(ui.style())
                                    .inner_margin(egui::Margin::symmetric(4, 2))
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::Label::new(&raw_hex)
                                                .sense(egui::Sense::click())
                                                .selectable(true),
                                        );
                                    });
                                ui.separator();

                        // Parsed message details
                        if let Some(ref parsed) = parsed {
                            egui::CollapsingHeader::new(format!("📊 解析: {}", parsed.name()))
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.label(format!("消息ID: 0x{:04X}", parsed.msg_id()));
                                    ui.label(format!("描述: {}", parsed.description()));
                                });
                        }
                    } else {
                        ui.label("(选中报文后显示详情)");
                    }

                    ui.separator();
                    ui.add_space(8.0);
                    ui.heading("⚡ 快捷指令");
                    ui.separator();

                    if self.current_conn_state() == Some(ConnState::Connected) {
                        // 组帧目标：当前连接的协议版本与设备 SN
                        let frame_version = self.current_protocol_version();
                        let frame_sn = self
                            .selected_conn
                            .and_then(|idx| self.connections.get(idx))
                            .map(|c| c.terminal_phone.clone())
                            .unwrap_or_default();
                        if frame_version.is_jt808() {
                            ui.label(
                                egui::RichText::new(format!(
                                    "组帧: {} | SN: {}",
                                    frame_version.short_name(),
                                    frame_sn
                                ))
                                .small()
                                .weak(),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("通用TCP: 发送内容原样发出，不解析、不组帧")
                                    .small()
                                    .weak(),
                            );
                        }

                        if frame_version.is_jt808() {
                            // Built-in commands
                            egui::CollapsingHeader::new("内置指令")
                                .default_open(true)
                                .show(ui, |ui| {
                                    if ui.button("💓 心跳 (0x0002)").clicked() {
                                        self.send_heartbeat();
                                    }
                                    if ui.button("📝 注册 (0x0100)").clicked() {
                                        self.run_registration_flow();
                                    }
                                    let ack_btn = ui.button("✅ 通用应答 (0x0001)")
                                        .on_hover_text("对选中的接收报文进行通用应答，流水号/消息ID取自已选报文");
                                    if ack_btn.clicked() {
                                        self.send_general_response();
                                    }
                                    let has_auth = self.selected_conn
                                        .and_then(|idx| self.connections.get(idx))
                                        .map(|c| !c.auth_code.is_empty())
                                        .unwrap_or(false);
                                    let auth_btn = egui::Button::new("🔑 鉴权 (0x0102)")
                                        .min_size(egui::vec2(ui.available_width(), 0.0));
                                    let auth_resp = ui.add_enabled(has_auth, auth_btn);
                                    if auth_resp.clicked() {
                                        self.send_auth();
                                    } else if !has_auth {
                                        auth_resp.clone().on_disabled_hover_text("鉴权码为空，请先执行注册流程或填写鉴权码");
                                    }
                                    if ui.button("📍 位置汇报 (0x0200)").clicked() {
                                        self.send_location_report();
                                    }
                                });

                            ui.add_space(4.0);
                        }

                        // Custom commands
                        egui::CollapsingHeader::new(format!("自定义指令 ({})", self.custom_commands.len()))
                            .default_open(true)
                            .show(ui, |ui| {
                                let mut to_delete: Option<usize> = None;
                                let cmds: Vec<QuickCommand> = self.custom_commands.clone();
                                for (i, cmd) in cmds.iter().enumerate() {
                                    let btn = ui.button(&cmd.name);
                                    if btn.clicked() {
                                        // 按当前连接的版本 + 设备 SN 重新组帧后发送
                                        self.send_quick_command(cmd);
                                    }
                                    btn.context_menu(|ui| {
                                        if ui.button("编辑").clicked() {
                                            let idx = i; // Copy the index
                                            self.edit_cmd = self.custom_commands[idx].clone();
                                            self.edit_cmd_msg_id_text = if self.edit_cmd.msg_id == 0 {
                                                String::new()
                                            } else {
                                                format!("{:04X}", self.edit_cmd.msg_id)
                                            };
                                            self.edit_cmd_index = Some(idx);
                                            self.show_cmd_dialog = true;
                                            ui.close_menu();
                                        }
                                        if ui.button("删除").clicked() {
                                            to_delete = Some(i);
                                            ui.close_menu();
                                        }
                                    });
                                }
                                if let Some(idx) = to_delete {
                                    self.custom_commands.remove(idx);
                                    self.save_custom_commands_to_db();
                                }
                            });

                        if ui.button("➕ 添加指令").clicked() {
                            self.edit_cmd = QuickCommand::new("新指令", "");
                            self.edit_cmd_msg_id_text.clear();
                            self.edit_cmd_index = None;
                            self.show_cmd_dialog = true;
                        }

                        if frame_version.is_jt808() {
                            ui.add_space(4.0);
                            ui.separator();
                            ui.label("流程控制:");
                            let conn_id_owned = self.current_conn_id();
                            let hb_enabled = conn_id_owned.as_ref()
                                .and_then(|id| self.heartbeat_enabled.get(id))
                                .copied()
                                .unwrap_or(false);
                            if hb_enabled {
                                if ui.button("⏸ 暂停心跳").clicked() {
                                    if let Some(ref id) = conn_id_owned {
                                        self.heartbeat_enabled.insert(id.clone(), false);
                                    }
                                }
                            } else {
                                if ui.button("▶️ 启用心跳").clicked() {
                                    if let Some(ref id) = conn_id_owned {
                                        self.heartbeat_enabled.insert(id.clone(), true);
                                        self.last_heartbeat.insert(id.clone(), self.elapsed_secs());
                                    }
                                }
                            }
                            if ui.button("📋 完整注册流程").clicked() {
                                self.run_registration_flow();
                            }
                        }
                    } else {
                        ui.label("(请先连接服务器)");
                    }
                        });
                });
            });

        // ── Bottom panel: Send input (fixed at bottom) ──
        TopBottomPanel::bottom("send_panel")
            .resizable(true)
            .default_height(200.0)
            .min_height(80.0)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("📝 发送:");
                        let connected = self.current_conn_state() == Some(ConnState::Connected);
                        let mut btn = ui.button("📤 发送");
                        if !connected {
                            btn = btn.on_disabled_hover_text("请先连接服务器");
                        }
                        if btn.clicked() {
                            if connected && !self.send_text.is_empty() {
                                self.send_from_input();
                            }
                        }
                        if ui.button("清空").clicked() {
                            self.send_text.clear();
                        }
                    });
                    let resp = ui.add(
                        egui::TextEdit::multiline(&mut self.send_text)
                            .desired_rows(8)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("输入 HEX (空格分隔) 或文本，Shift+Enter 换行，Enter 发送")
                            .lock_focus(true),
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if !ui.input(|i| i.modifiers.shift) {
                            if !self.send_text.is_empty() {
                                self.send_from_input();
                            }
                        }
                    }
                });
            });

        // ── Center panel: Message stream ──
        CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                let conn_name = self.selected_conn
                    .and_then(|idx| self.connections.get(idx))
                    .map(|c| c.name.as_str())
                    .unwrap_or("未选择");
                let state = self.current_conn_state();
                let state_text = state.as_ref().map(|s| s.label()).unwrap_or("");
                ui.heading(format!("📋 报文流 - {} [{}]", conn_name, state_text));
                ui.separator();

                let conn_id_opt = self.current_conn_id();
                let selected_idx = conn_id_opt.as_ref()
                    .and_then(|id| self.selected_msg.get(id))
                    .copied();
                let msg_count = self.current_messages().len();
                if msg_count == 0 {
                    ui.add_space(80.0);
                    ui.vertical_centered(|ui| {
                        ui.label("暂无报文");
                        ui.label("请新建连接并发送/接收数据");
                    });
                } else {
                    let conn_id_clone = conn_id_opt.clone();
                    let msgs: Vec<_> = self.current_messages().iter().map(|m| {
                        let summary = if let Some(ref parsed) = m.parsed {
                            format!("[{}] {}", parsed.name(), parsed.description())
                        } else if m.raw_hex.is_empty() {
                            String::new()
                        } else {
                            let short = if m.raw_hex.len() > 60 {
                                format!("{}...", &m.raw_hex[..60])
                            } else {
                                m.raw_hex.clone()
                            };
                            short
                        };
                        (m.timestamp.format("%H:%M:%S").to_string(), m.direction.arrow().to_string(), summary)
                    }).collect();

                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            for (i, (ts, arrow, summary)) in msgs.iter().enumerate() {
                                let is_selected = selected_idx == Some(i);
                                let label = format!("[{}] {} {}", ts, arrow, summary);
                                let response = ui.selectable_label(is_selected, label);
                                if response.clicked() {
                                    if let Some(ref id) = conn_id_clone {
                                        self.selected_msg.insert(id.clone(), i);
                                    }
                                }
                                ui.separator();
                            }
                        });
                }
            });
        });

        // ── Toast notification ──
        if show_toast {
            egui::Area::new(egui::Id::new("toast_area"))
                .anchor(egui::Align2::CENTER_TOP, [0.0, 40.0])
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(egui::Color32::from_black_alpha(200))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(12, 6))
                        .show(ui, |ui| {
                            ui.colored_label(egui::Color32::WHITE, "✅ 已复制到剪贴板");
                        });
                });
        }

        // ── Connection dialog (new/edit) ──
        if self.show_new_conn_dialog {
            let is_new = self.editing_conn_idx.is_none();
            egui::Window::new(if is_new { "新建连接" } else { "编辑连接" })
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    egui::Grid::new("new_conn_grid")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("名称:");
                            ui.text_edit_singleline(&mut self.edit_conn.name);
                            ui.end_row();

                            ui.label("主机:");
                            ui.text_edit_singleline(&mut self.edit_conn.host);
                            ui.end_row();

                            ui.label("端口:");
                            ui.add(egui::DragValue::new(&mut self.edit_conn.port).range(1..=65535));
                            ui.end_row();

                            ui.label("终端手机号/SN:");
                            ui.text_edit_singleline(&mut self.edit_conn.terminal_phone)
                                .on_hover_text(
                                    "平台用该字段承载终端 SN（十六进制字符）：\n\
                                     2013 版 12 个字符；2019 版 20 个字符，不足时左侧补 0",
                                );
                            ui.end_row();

                            ui.label("鉴权码:");
                            ui.text_edit_singleline(&mut self.edit_conn.auth_code);
                            ui.end_row();

                            ui.label("协议版本:");
                            egui::ComboBox::from_id_salt("edit_conn_protocol_version")
                                .selected_text(self.edit_conn.protocol_version.name())
                                .show_ui(ui, |ui| {
                                    for v in ProtocolVersion::all() {
                                        ui.selectable_value(
                                            &mut self.edit_conn.protocol_version,
                                            *v,
                                            v.name(),
                                        );
                                    }
                                })
                                .response
                                .on_hover_text(
                                    "JT808-2013/2019：发送时按所选版本组帧，接收按 bit14 自动识别\n\
                                     通用TCP：发送内容原样发出，不解析、不组帧",
                                );
                            ui.end_row();

                            ui.label("终端IMEI (2019):");
                            ui.text_edit_singleline(&mut self.edit_conn.imei);
                            ui.end_row();

                            ui.label("软件版本号 (2019):");
                            ui.text_edit_singleline(&mut self.edit_conn.software_version);
                            ui.end_row();

                            ui.label("心跳间隔(s):");
                            ui.add(egui::DragValue::new(&mut self.edit_conn.heartbeat_interval_secs).range(0..=3600));
                            ui.end_row();

                            ui.label("自动重连:");
                            ui.checkbox(&mut self.edit_conn.auto_reconnect, "");
                            ui.end_row();
                        });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("✅ 确定").clicked() {
                            let config = self.edit_conn.clone();
                            let conn_id = config.id.clone();
                            let version = config.protocol_version;
                            // Save to database for persistence
                            if let Some(ref db) = self.db {
                                let _ = db.save_connection(&config);
                            }
                            if is_new {
                                // Create new connection
                                let handle = connection::start_connection(config.clone(), self.event_tx.clone());
                                self.conn_states.insert(conn_id.clone(), ConnState::Disconnected);
                                self.handles.insert(conn_id.clone(), handle);
                                self.connections.push(config);
                                self.selected_conn = Some(self.connections.len() - 1);
                            } else {
                                // Update existing connection
                                if let Some(idx) = self.editing_conn_idx {
                                    if idx < self.connections.len() {
                                        self.connections[idx] = config;
                                        self.selected_conn = Some(idx);
                                    }
                                }
                            }
                            // Keep the running connection task in sync
                            if let Some(handle) = self.handles.get(&conn_id) {
                                handle.set_protocol_version(version);
                            }
                            self.default_protocol_version = version;
                            self.editing_conn_idx = None;
                            self.show_new_conn_dialog = false;
                        }
                        if ui.button("❌ 取消").clicked() {
                            self.editing_conn_idx = None;
                            self.show_new_conn_dialog = false;
                        }
                    });
                });
        }

        // ── Export dialog ──
        if self.show_export_dialog {
            egui::Window::new("导出报文")
                .collapsible(false)
                .resizable(true)
                .default_size([500.0, 400.0])
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.label("选择导出格式:");
                        ui.horizontal(|ui| {
                            if ui.button("📄 导出 JSON").clicked() {
                                let conn_id = self.current_conn_id();
                                let json = self.export_json(conn_id.as_deref());
                                if let Err(e) = std::fs::write("export.json", &json) {
                                    log::error!("Export failed: {}", e);
                                } else {
                                    log::info!("Exported to export.json");
                                }
                                self.show_export_dialog = false;
                            }
                            if ui.button("📊 导出 CSV").clicked() {
                                let conn_id = self.current_conn_id();
                                let csv = self.export_csv(conn_id.as_deref());
                                if let Err(e) = std::fs::write("export.csv", &csv) {
                                    log::error!("Export failed: {}", e);
                                } else {
                                    log::info!("Exported to export.csv");
                                }
                                self.show_export_dialog = false;
                            }
                        });
                        ui.separator();
                        let conn_id = self.current_conn_id();
                        let msg_count = conn_id.as_ref()
                            .and_then(|id| self.messages.get(id))
                            .map(|v| v.len())
                            .unwrap_or(0);
                        ui.label(format!("当前连接报文数: {}", msg_count));
                        if ui.button("❌ 关闭").clicked() {
                            self.show_export_dialog = false;
                        }
                    });
                });
        }

        // ── Quick command dialog ──
        if self.show_cmd_dialog {
            let is_new = self.edit_cmd_index.is_none();
            egui::Window::new(if is_new { "添加快捷指令" } else { "编辑快捷指令" })
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    egui::Grid::new("cmd_grid")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("名称:");
                            ui.text_edit_singleline(&mut self.edit_cmd.name);
                            ui.end_row();

                            ui.label("消息ID (HEX):");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.edit_cmd_msg_id_text)
                                    .desired_width(120.0)
                                    .hint_text("空/0=完整帧或原始数据"),
                            )
                            .on_hover_text(
                                "非0时 RAW 数据视为消息体，发送时按当前版本+SN组帧；\n\
                                 为空/0时若 RAW 是完整 JT808 帧，也会按当前版本+SN重组帧",
                            );
                            ui.end_row();

                            ui.label("HEX 数据 (多行):");
                            ui.add(
                                egui::TextEdit::multiline(&mut self.edit_cmd.raw_hex)
                                    .desired_rows(5)
                                    .desired_width(280.0)
                                    .font(egui::TextStyle::Monospace)
                                    .hint_text(
                                        "完整 JT808 帧或消息体 HEX，如:\n7E00020000...\n\
                                         发送时自动按当前版本+设备SN重新组帧",
                                    ),
                            );
                            ui.end_row();
                        });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("✅ 确定").clicked() {
                            if !self.edit_cmd.name.is_empty() && !self.edit_cmd.raw_hex.is_empty() {
                                let msg_id_text = self
                                    .edit_cmd_msg_id_text
                                    .trim()
                                    .trim_start_matches("0x")
                                    .trim_start_matches("0X");
                                self.edit_cmd.msg_id = if msg_id_text.is_empty() {
                                    0
                                } else {
                                    u16::from_str_radix(msg_id_text, 16).unwrap_or(0)
                                };
                                if let Some(idx) = self.edit_cmd_index {
                                    if idx < self.custom_commands.len() {
                                        self.custom_commands[idx] = self.edit_cmd.clone();
                                    }
                                } else {
                                    self.custom_commands.push(self.edit_cmd.clone());
                                }
                                // Save all custom commands to DB
                                self.save_custom_commands_to_db();
                            }
                            self.show_cmd_dialog = false;
                        }
                        if ui.button("❌ 取消").clicked() {
                            self.show_cmd_dialog = false;
                        }
                    });
                });
        }
    }
}

impl TcpToolApp {
    /// 快捷指令发送：按当前连接选择的协议版本和设备 SN 重新组帧后再发送。
    ///
    /// - `msg_id != 0`：raw_hex 视为消息体，按当前版本+SN 组帧
    /// - 完整 JT808 帧（0x7E 开头）：解析后按当前版本+SN 重组帧，已知消息体按版本重编码
    /// - 其他裸数据：原样发送
    fn send_quick_command(&mut self, cmd: &QuickCommand) {
        let info = self.selected_conn.and_then(|idx| {
            self.connections.get(idx).map(|c| {
                (
                    c.id.clone(),
                    c.terminal_phone.clone(),
                    c.protocol_version,
                    c.imei.clone(),
                    c.software_version.clone(),
                )
            })
        });
        let Some((conn_id, phone, version, imei, software_version)) = info else {
            return;
        };

        let hex = cmd.raw_hex.replace(['\n', '\r', ' ', ','], "");
        let bytes: Vec<u8> = match (0..hex.len())
            .step_by(2)
            .filter(|&i| i + 1 <= hex.len())
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
            .collect::<Result<Vec<u8>, _>>()
        {
            Ok(b) => b,
            Err(_) => {
                self.add_message(
                    &conn_id,
                    Direction::Send,
                    Vec::new(),
                    Some(format!("⚠️ 快捷指令 [{}] HEX 格式错误", cmd.name)),
                );
                return;
            }
        };
        if bytes.is_empty() {
            return;
        }

        // 通用TCP：原样发送，不做任何组帧/重组
        if version.is_generic() {
            self.send_raw(bytes);
            return;
        }

        // 1) 仅消息体模式：按当前版本+SN 组帧
        if cmd.msg_id != 0 {
            let serial = self.next_serial(&conn_id);
            let frame = builder::build_frame(version, cmd.msg_id, &phone, serial, &bytes);
            self.send_raw(frame);
            return;
        }

        // 2) 完整 JT808 帧：解析后按当前版本+SN 重新组帧
        if let Some((msg_id, new_body)) =
            reframe_quick_command(&bytes, version, &phone, &imei, &software_version)
        {
            let serial = self.next_serial(&conn_id);
            let frame = builder::build_frame(version, msg_id, &phone, serial, &new_body);
            self.send_raw(frame);
            return;
        }

        // 3) 非 JT808 数据原样发送
        self.send_raw(bytes);
    }

    /// 对当前选中的接收报文发送终端通用应答 (0x0001)，按当前版本+SN 组帧
    fn send_general_response(&mut self) {
        let info = self.selected_conn.and_then(|idx| {
            self.connections
                .get(idx)
                .map(|c| (c.id.clone(), c.terminal_phone.clone(), c.protocol_version))
        });
        let Some((conn_id, phone, version)) = info else {
            return;
        };

        let selected = self
            .selected_msg
            .get(&conn_id)
            .copied()
            .and_then(|idx| self.messages.get(&conn_id).and_then(|m| m.get(idx)))
            .filter(|m| m.direction == Direction::Receive)
            .map(|m| m.raw_bytes.clone());
        let Some(raw) = selected else {
            self.add_message(
                &conn_id,
                Direction::Send,
                Vec::new(),
                Some("⚠️ 请先在报文流中选中一条接收报文，再发送通用应答".into()),
            );
            return;
        };
        if raw.is_empty() || raw[0] != 0x7E {
            self.add_message(
                &conn_id,
                Direction::Send,
                Vec::new(),
                Some("⚠️ 选中的报文不是有效的 JT808 帧".into()),
            );
            return;
        }
        let Some((msg, _rest)) = crate::protocol::codec::decode_message(&raw) else {
            self.add_message(
                &conn_id,
                Direction::Send,
                Vec::new(),
                Some("⚠️ 选中的报文解析失败（校验码错误？）".into()),
            );
            return;
        };
        let Ok((header, _header_len)) = MsgHeader::decode(&msg) else {
            return;
        };

        let body = message::TerminalGeneralResponse {
            response_serial_no: header.serial_no,
            response_msg_id: header.msg_id,
            result: 0,
        };
        let serial = self.next_serial(&conn_id);
        let frame = builder::build_frame(version, 0x0001, &phone, serial, &body.encode(version));
        self.send_raw(frame);
    }

    fn send_from_input(&mut self) {
        let text = self.send_text.trim().to_string();
        if text.is_empty() {
            return;
        }

        let data = match self.encoding {
            Encoding::HEX => {
                // Parse space-separated hex bytes
                let hex_str = text.replace(' ', "").replace(',', "");
                let bytes: Result<Vec<u8>, _> = (0..hex_str.len())
                    .step_by(2)
                    .filter(|&i| i + 1 <= hex_str.len())
                    .map(|i| u8::from_str_radix(&hex_str[i..i + 2], 16))
                    .collect();
                match bytes {
                    Ok(b) => b,
                    Err(_) => {
                        // Treat as text
                        text.bytes().collect()
                    }
                }
            }
            Encoding::GBK => {
                use encoding_rs::GBK;
                let (encoded, _, _) = GBK.encode(&text);
                encoded.into_owned()
            }
            Encoding::UTF8 => text.as_bytes().to_vec(),
            Encoding::ASCII => text.as_bytes().to_vec(),
        };

        self.send_raw(data);
    }

    /// Export messages as JSON string
    pub fn export_json(&self, conn_id: Option<&str>) -> String {
        let msgs: Vec<&MessageEntry> = match conn_id.and_then(|id| self.messages.get(id)) {
            Some(v) => v.iter().collect(),
            None => self.messages.values().flatten().collect(),
        };

        let entries: Vec<serde_json::Value> = msgs.iter().map(|m| {
            serde_json::json!({
                "time": m.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(),
                "direction": m.direction.name(),
                "encoding": m.encoding,
                "protocol": m.protocol_version.map(|v| v.short_name()),
                "hex": m.raw_hex,
                "parsed": m.parsed.as_ref().map(|p| serde_json::json!({
                    "id": p.msg_id(),
                    "name": p.name(),
                    "desc": p.description(),
                }))
            })
        }).collect();

        serde_json::to_string_pretty(&serde_json::json!({
            "export_time": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            "message_count": msgs.len(),
            "messages": entries
        })).unwrap_or_else(|_| "{}".to_string())
    }

    /// Export messages as CSV string
    pub fn export_csv(&self, conn_id: Option<&str>) -> String {
        let msgs: Vec<&MessageEntry> = match conn_id.and_then(|id| self.messages.get(id)) {
            Some(v) => v.iter().collect(),
            None => self.messages.values().flatten().collect(),
        };

        let mut csv = String::from("时间,方向,编码,协议,HEX,消息ID,消息名称\n");
        for m in msgs {
            let (msg_id, msg_name) = m.parsed.as_ref()
                .map(|p| (format!("0x{:04X}", p.msg_id()), p.name().to_string()))
                .unwrap_or_else(|| ("".into(), "".into()));
            csv.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                m.timestamp.format("%Y-%m-%d %H:%M:%S"),
                m.direction.name(),
                m.encoding,
                m.protocol_version.map(|v| v.short_name()).unwrap_or(""),
                m.raw_hex,
                msg_id,
                msg_name,
            ));
        }
        csv
    }

    /// Save all custom commands to SQLite
    fn save_custom_commands_to_db(&self) {
        if let Some(ref db) = self.db {
            for (i, cmd) in self.custom_commands.iter().enumerate() {
                let _ = db.save_quick_command(&cmd.id, &cmd.name, cmd.msg_id, &cmd.raw_hex, i as i32);
            }
        }
    }
}

/// 按目标协议版本和 SN 重新组织 JT808 快捷指令帧。
/// 返回 `(msg_id, 新消息体)`；非 JT808 帧返回 None（由调用方原样发送）。
fn reframe_quick_command(
    bytes: &[u8],
    version: ProtocolVersion,
    sn: &str,
    imei: &str,
    software_version: &str,
) -> Option<(u16, Vec<u8>)> {
    if bytes.is_empty() || bytes[0] != 0x7E {
        return None;
    }
    let (msg, _rest) = crate::protocol::codec::decode_message(bytes)?;
    let (header, header_len) = MsgHeader::decode(&msg).ok()?;
    let body = if msg.len() > header_len {
        &msg[header_len..]
    } else {
        &[]
    };
    let parsed = message::parse_message(header.protocol_version(), header.msg_id, body);
    let new_body = reencode_known_body(&parsed, version, sn, imei, software_version)
        .unwrap_or_else(|| body.to_vec());
    Some((header.msg_id, new_body))
}

/// 将已解析的消息体按目标协议版本重新编码（用于快捷指令按版本+SN 重组）。
/// 返回 None 表示未知消息体，调用方保留原消息体直接重封帧。
fn reencode_known_body(
    parsed: &ParsedMessage,
    version: ProtocolVersion,
    sn: &str,
    imei: &str,
    software_version: &str,
) -> Option<Vec<u8>> {
    match parsed {
        ParsedMessage::Heartbeat(_) => Some(Vec::new()),
        ParsedMessage::TerminalRegister(r) => {
            let mut reg = r.clone();
            // 注册消息体中的终端ID与头部的设备SN保持一致
            if !sn.is_empty() {
                reg.terminal_id = sn.to_string();
            }
            Some(reg.encode(version))
        }
        ParsedMessage::TerminalAuth(a) => {
            let mut auth = a.clone();
            if auth.imei.is_empty() {
                auth.imei = imei.to_string();
            }
            if auth.software_version.is_empty() {
                auth.software_version = software_version.to_string();
            }
            Some(auth.encode(version))
        }
        ParsedMessage::TerminalGeneralResponse(r) => Some(r.encode(version)),
        ParsedMessage::PlatformGeneralResponse(r) => Some(r.encode(version)),
        ParsedMessage::RegisterResponse(r) => Some(r.encode(version)),
        ParsedMessage::LocationReport(r) => Some(r.encode(version)),
        ParsedMessage::ParameterSet(p) => Some(p.encode(version)),
        ParsedMessage::TextMessage(t) => Some(t.encode(version)),
        ParsedMessage::VehicleControl(v) => Some(v.encode(version)),
        ParsedMessage::QueryTerminalAttrResponse(a) => Some(a.encode(version)),
        ParsedMessage::Unknown { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_to_bytes(s: &str) -> Vec<u8> {
        let clean: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        (0..clean.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).unwrap())
            .collect()
    }

    // 用户数据库中保存的真实快捷指令（整帧硬编码 2019 版、SN=61316263600001）
    const REGISTER_2019: &str = "7e0100405301000000613162636000010001000000004e54424d530000000000004e542d424d532d323031390000000000000000000000000000000000000036313331363236333630303030310000000000000000000000000000000000544553543030310c7e";
    const AUTH_2019: &str = "7e0102403a010000006131626360000100021636313331363236333630303030312c5445535430303138363132333435363738393031323356312e302e300000000000000000000000000000000c7e";
    const HEARTBEAT_2019: &str = "7e0002400001000000613162636000010003707e";

    #[test]
    fn test_reframe_register_uses_target_sn() {
        let bytes = hex_to_bytes(REGISTER_2019);
        let (msg_id, body) =
            reframe_quick_command(&bytes, ProtocolVersion::V2019, "99999999999999", "", "")
                .unwrap();
        assert_eq!(msg_id, 0x0100);
        let frame =
            builder::build_frame(ProtocolVersion::V2019, msg_id, "99999999999999", 7, &body);
        let (msg, _) = crate::protocol::codec::decode_message(&frame).unwrap();
        let (header, header_len) = MsgHeader::decode(&msg).unwrap();
        assert_eq!(header.terminal_id, "99999999999999");
        match message::parse_message(header.protocol_version(), header.msg_id, &msg[header_len..]) {
            ParsedMessage::TerminalRegister(r) => {
                assert_eq!(r.terminal_id, "99999999999999");
                assert_eq!(r.manufacturer_id, "NTBMS");
            }
            other => panic!("expected TerminalRegister, got {}", other.name()),
        }
    }

    #[test]
    fn test_reframe_auth_2019_to_2013() {
        let bytes = hex_to_bytes(AUTH_2019);
        let (msg_id, body) =
            reframe_quick_command(&bytes, ProtocolVersion::V2013, "082255301774", "", "").unwrap();
        assert_eq!(msg_id, 0x0102);
        // 2013 版鉴权体应为裸鉴权码
        assert_eq!(body, b"61316263600001,TEST001");
    }

    #[test]
    fn test_reframe_auth_2019_keeps_2019_fields() {
        let bytes = hex_to_bytes(AUTH_2019);
        let (msg_id, body) =
            reframe_quick_command(&bytes, ProtocolVersion::V2019, "61316263600001", "", "")
                .unwrap();
        assert_eq!(msg_id, 0x0102);
        let decoded =
            crate::protocol::message::TerminalAuth::decode(&body, ProtocolVersion::V2019).unwrap();
        assert_eq!(decoded.auth_code, "61316263600001,TEST001");
        assert_eq!(decoded.imei, "861234567890123");
        assert_eq!(decoded.software_version, "V1.0.0");
    }

    #[test]
    fn test_reframe_heartbeat_and_unknown() {
        let bytes = hex_to_bytes(HEARTBEAT_2019);
        let (msg_id, body) =
            reframe_quick_command(&bytes, ProtocolVersion::V2019, "082255301774", "", "").unwrap();
        assert_eq!(msg_id, 0x0002);
        assert!(body.is_empty());

        // 未知消息体（如 0x0900 透传）应保留原消息体
        let frame = builder::build_frame(
            ProtocolVersion::V2013,
            0x0900,
            "082255301774",
            1,
            &[0xAA, 0xBB, 0xCC],
        );
        let (msg_id, body) =
            reframe_quick_command(&frame, ProtocolVersion::V2019, "61316263600001", "", "")
                .unwrap();
        assert_eq!(msg_id, 0x0900);
        assert_eq!(body, vec![0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn test_reframe_non_jt808_returns_none() {
        assert!(reframe_quick_command(
            &[0x01, 0x02, 0x03],
            ProtocolVersion::V2019,
            "x",
            "",
            ""
        )
        .is_none());
    }
}
