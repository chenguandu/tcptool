/// TCP connection management module
/// Manages multiple TCP connections using tokio async tasks

use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::{self, Duration};

use serde::{Deserialize, Serialize};

/// Connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub auto_reconnect: bool,
    pub heartbeat_interval_secs: u32,
    pub terminal_phone: String,
    pub auth_code: String,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            id: uuid_v4(),
            name: "新连接".into(),
            host: "127.0.0.1".into(),
            port: 5100,
            auto_reconnect: false,
            heartbeat_interval_secs: 30,
            terminal_phone: "013900000001".into(),
            auth_code: "".into(),
        }
    }
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("conn-{:016x}", nanos)
}

/// Connection state
#[derive(Debug, Clone, PartialEq)]
pub enum ConnState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

impl ConnState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Disconnected => "已断开",
            Self::Connecting => "连接中",
            Self::Connected => "已连接",
            Self::Disconnecting => "断开中",
        }
    }
    pub fn color(&self) -> egui::Color32 {
        match self {
            Self::Disconnected => egui::Color32::GRAY,
            Self::Connecting => egui::Color32::YELLOW,
            Self::Connected => egui::Color32::GREEN,
            Self::Disconnecting => egui::Color32::ORANGE,
        }
    }
}

/// Events sent from the connection task to the UI
#[derive(Debug, Clone)]
pub enum ConnEvent {
    Connected,
    Disconnected(String),
    DataReceived(Vec<u8>),
    Error(String),
}

/// Commands sent from the UI to the connection task
#[derive(Debug)]
pub enum ConnCommand {
    Connect,
    Disconnect,
    Send(Vec<u8>),
}

/// A handle to a managed TCP connection
pub struct TcpConnectionHandle {
    pub config: ConnectionConfig,
    pub state: Arc<Mutex<ConnState>>,
    command_tx: tokio::sync::mpsc::UnboundedSender<ConnCommand>,
    /// Accumulated received data buffer
    pub rx_buffer: Arc<Mutex<Vec<u8>>>,
}

impl TcpConnectionHandle {
    pub fn connect(&self) {
        let _ = self.command_tx.send(ConnCommand::Connect);
    }
    pub fn disconnect(&self) {
        let _ = self.command_tx.send(ConnCommand::Disconnect);
    }
    pub fn send(&self, data: Vec<u8>) {
        let _ = self.command_tx.send(ConnCommand::Send(data));
    }
}

/// Start a managed TCP connection in a background tokio task.
/// Returns a handle for the UI to interact with.
pub fn start_connection(
    config: ConnectionConfig,
    event_tx: tokio::sync::mpsc::UnboundedSender<(String, ConnEvent)>,
) -> TcpConnectionHandle {
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<ConnCommand>();
    let state = Arc::new(Mutex::new(ConnState::Disconnected));
    let rx_buffer = Arc::new(Mutex::new(Vec::new()));
    let conn_id = config.id.clone();

    let state_clone = state.clone();
    let rx_buf_clone = rx_buffer.clone();
    let config_clone = config.clone();

    tokio::spawn(async move {
        let mut stream: Option<TcpStream> = None;
        let mut intentional_disconnect = false;
        let mut heartbeat_interval = time::interval(Duration::from_secs(
            config_clone.heartbeat_interval_secs.max(1) as u64,
        ));

        loop {
            // Use biased select so we check for commands first, then read, then heartbeat
            tokio::select! {
                biased;

                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        ConnCommand::Connect => {
                            intentional_disconnect = false;
                            *state_clone.lock().await = ConnState::Connecting;
                            let addr = format!("{}:{}", config_clone.host, config_clone.port);
                            match TcpStream::connect(&addr).await {
                                Ok(s) => {
                                    stream = Some(s);
                                    *state_clone.lock().await = ConnState::Connected;
                                    let _ = event_tx.send((conn_id.clone(), ConnEvent::Connected));
                                }
                                Err(e) => {
                                    *state_clone.lock().await = ConnState::Disconnected;
                                    let _ = event_tx.send((conn_id.clone(), ConnEvent::Error(format!("连接失败: {}", e))));
                                }
                            }
                        }
                        ConnCommand::Disconnect => {
                            intentional_disconnect = true;
                            if let Some(mut s) = stream.take() {
                                *state_clone.lock().await = ConnState::Disconnecting;
                                let _ = s.shutdown().await;
                            }
                            *state_clone.lock().await = ConnState::Disconnected;
                            let _ = event_tx.send((conn_id.clone(), ConnEvent::Disconnected("用户断开".into())));
                        }
                        ConnCommand::Send(data) => {
                            if let Some(ref mut s) = stream {
                                match s.write_all(&data).await {
                                    Ok(_) => {
                                        let _ = s.flush().await;
                                    }
                                    Err(e) => {
                                        let _ = event_tx.send((conn_id.clone(), ConnEvent::Error(format!("发送失败: {}", e))));
                                    }
                                }
                            }
                        }
                    }
                }

                // Wait for socket to become readable (data available)
                _readable = async {
                    if let Some(ref mut s) = stream {
                        s.readable().await.ok()
                    } else {
                        None::<()>
                    }
                }, if stream.is_some() => {
                    // Drain all available data
                    if let Some(ref mut s) = stream {
                        loop {
                            let mut buf = vec![0u8; 4096];
                            match s.try_read(&mut buf) {
                                Ok(0) => {
                                    stream = None;
                                    *state_clone.lock().await = ConnState::Disconnected;
                                    let _ = event_tx.send((conn_id.clone(), ConnEvent::Disconnected("连接关闭".into())));
                                    break;
                                }
                                Ok(n) => {
                                    buf.truncate(n);
                                    rx_buf_clone.lock().await.extend_from_slice(&buf);
                                    let _ = event_tx.send((conn_id.clone(), ConnEvent::DataReceived(buf)));
                                }
                                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    break;
                                }
                                Err(_) => {
                                    stream = None;
                                    *state_clone.lock().await = ConnState::Disconnected;
                                    break;
                                }
                            }
                        }
                    }
                }

                _ = heartbeat_interval.tick(), if stream.is_some() => {
                    // Send heartbeat automatically from connection task
                    let hb = crate::protocol::builder::build_heartbeat(&config_clone.terminal_phone, 1);
                    if let Some(ref mut s) = stream {
                        let _ = s.write_all(&hb).await;
                        let _ = s.flush().await;
                    }
                }
            }

            // Auto-reconnect if enabled (skip if user manually disconnected)
            if stream.is_none() && config_clone.auto_reconnect && !intentional_disconnect {
                time::sleep(Duration::from_secs(3)).await;
                *state_clone.lock().await = ConnState::Connecting;
                let addr = format!("{}:{}", config_clone.host, config_clone.port);
                if let Ok(s) = TcpStream::connect(&addr).await {
                    stream = Some(s);
                    *state_clone.lock().await = ConnState::Connected;
                    let _ = event_tx.send((conn_id.clone(), ConnEvent::Connected));
                } else {
                    *state_clone.lock().await = ConnState::Disconnected;
                }
            }
        }
    });

    TcpConnectionHandle {
        config,
        state,
        command_tx: cmd_tx,
        rx_buffer,
    }
}
