/// Database persistence module
/// SQLite for connections, messages, quick commands, and settings

pub mod schema;

use rusqlite::{params, Connection, Result as SqlResult};

use crate::app::{Direction, MessageEntry};
use crate::connection::ConnectionConfig;
use crate::protocol::types::ProtocolVersion;

/// Database manager
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create the database at the given path
    pub fn open(path: &str) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let db = Self { conn };
        db.initialize()?;
        Ok(db)
    }

    /// Create tables if they don't exist
    fn initialize(&self) -> SqlResult<()> {
        self.conn.execute_batch(schema::SCHEMA_SQL)?;
        self.migrate()?;
        Ok(())
    }

    /// Apply incremental migrations for databases created by older versions
    fn migrate(&self) -> SqlResult<()> {
        let mut columns: Vec<String> = Vec::new();
        {
            let mut stmt = self.conn.prepare("PRAGMA table_info(connections)")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                columns.push(row.get(1)?);
            }
        }
        if !columns.iter().any(|c| c == "protocol_version") {
            self.conn.execute(
                "ALTER TABLE connections ADD COLUMN protocol_version TEXT NOT NULL DEFAULT '2013'",
                [],
            )?;
        }
        if !columns.iter().any(|c| c == "imei") {
            self.conn.execute(
                "ALTER TABLE connections ADD COLUMN imei TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        if !columns.iter().any(|c| c == "software_version") {
            self.conn.execute(
                "ALTER TABLE connections ADD COLUMN software_version TEXT NOT NULL DEFAULT 'V1.0.0'",
                [],
            )?;
        }
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // ─── Connection CRUD ───

    pub fn save_connection(&self, config: &ConnectionConfig) -> SqlResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO connections (id, name, host, port, auto_reconnect, heartbeat_interval, terminal_phone, auth_code, protocol_version, imei, software_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                config.id,
                config.name,
                config.host,
                config.port,
                config.auto_reconnect as i32,
                config.heartbeat_interval_secs,
                config.terminal_phone,
                config.auth_code,
                config.protocol_version.short_name(),
                config.imei,
                config.software_version,
            ],
        )?;
        Ok(())
    }

    pub fn load_connections(&self) -> SqlResult<Vec<ConnectionConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, host, port, auto_reconnect, heartbeat_interval, terminal_phone, auth_code, protocol_version, imei, software_version
             FROM connections ORDER BY created_at"
        )?;
        let rows = stmt.query_map([], |row| {
            let version: String = row.get(8)?;
            Ok(ConnectionConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                host: row.get(2)?,
                port: row.get(3)?,
                auto_reconnect: row.get::<_, i32>(4)? != 0,
                heartbeat_interval_secs: row.get(5)?,
                terminal_phone: row.get(6)?,
                auth_code: row.get(7)?,
                protocol_version: ProtocolVersion::from_str_loose(&version),
                imei: row.get(9)?,
                software_version: row.get(10)?,
            })
        })?;
        rows.collect()
    }

    pub fn delete_connection(&self, id: &str) -> SqlResult<()> {
        self.conn.execute("DELETE FROM connections WHERE id = ?1", params![id])?;
        self.conn.execute("DELETE FROM messages WHERE connection_id = ?1", params![id])?;
        Ok(())
    }

    // ─── Message CRUD ───

    pub fn save_message(
        &self,
        connection_id: &str,
        direction: &str,
        msg_id: Option<u16>,
        msg_name: Option<&str>,
        raw_hex: &str,
        parsed_json: Option<&str>,
    ) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO messages (connection_id, direction, msg_id, msg_name, raw_hex, parsed_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![connection_id, direction, msg_id, msg_name, raw_hex, parsed_json],
        )?;
        Ok(())
    }

    /// Save a MessageEntry to the database
    pub fn save_message_entry(&self, connection_id: &str, entry: &MessageEntry) -> SqlResult<()> {
        let direction = match entry.direction {
            Direction::Send => "send",
            Direction::Receive => "recv",
        };
        let (msg_id, msg_name, parsed_json) = if let Some(ref parsed) = entry.parsed {
            (
                Some(parsed.msg_id() as i64),
                Some(parsed.name().to_string()),
                Some(serde_json::json!({
                    "id": parsed.msg_id(),
                    "name": parsed.name(),
                    "desc": parsed.description(),
                }).to_string()),
            )
        } else {
            (None, None, None)
        };
        self.conn.execute(
            "INSERT INTO messages (connection_id, direction, msg_id, msg_name, raw_hex, parsed_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![connection_id, direction, msg_id, msg_name, entry.raw_hex, parsed_json],
        )?;
        Ok(())
    }

    pub fn load_messages(&self, _connection_id: &str, _limit: i64) -> SqlResult<Vec<crate::app::MessageEntry>> {
        // This is a simplified version - in practice you'd load from DB
        Ok(Vec::new())
    }

    // ─── Quick Command CRUD ───

    pub fn save_quick_command(&self, id: &str, name: &str, msg_id: u16, params_json: &str, sort_order: i32) -> SqlResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO quick_commands (id, name, msg_id, params_json, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, msg_id, params_json, sort_order],
        )?;
        Ok(())
    }

    pub fn load_quick_commands(&self) -> SqlResult<Vec<(String, String, u16, String, i32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, msg_id, params_json, sort_order FROM quick_commands ORDER BY sort_order"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u16>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i32>(4)?,
            ))
        })?;
        rows.collect()
    }

    // ─── Settings ───

    pub fn get_setting(&self, key: &str) -> SqlResult<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_init() {
        let db = Database::open(":memory:").unwrap();
        // Verify tables exist
        let tables: Vec<String> = db.conn()
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<SqlResult<_>>()
            .unwrap();
        assert!(tables.contains(&"connections".to_string()));
        assert!(tables.contains(&"messages".to_string()));
        assert!(tables.contains(&"quick_commands".to_string()));
        assert!(tables.contains(&"app_settings".to_string()));
    }

    #[test]
    fn test_save_load_connection() {
        let db = Database::open(":memory:").unwrap();
        let config = ConnectionConfig::default();
        db.save_connection(&config).unwrap();
        let loaded = db.load_connections().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, config.id);
        assert_eq!(loaded[0].name, config.name);
        assert_eq!(loaded[0].protocol_version, ProtocolVersion::V2013);
    }

    #[test]
    fn test_save_load_connection_2019() {
        let db = Database::open(":memory:").unwrap();
        let mut config = ConnectionConfig::default();
        config.protocol_version = ProtocolVersion::V2019;
        db.save_connection(&config).unwrap();
        let loaded = db.load_connections().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].protocol_version, ProtocolVersion::V2019);
    }

    #[test]
    fn test_save_load_connection_generic_tcp() {
        let db = Database::open(":memory:").unwrap();
        let mut config = ConnectionConfig::default();
        config.protocol_version = ProtocolVersion::GenericTcp;
        db.save_connection(&config).unwrap();
        let loaded = db.load_connections().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].protocol_version, ProtocolVersion::GenericTcp);
    }

    #[test]
    fn test_migration_adds_protocol_version() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tcptool-migrate-{}.db", unique));
        let path_str = path.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&path);

        // Create a database with the old (pre-2019) connections table
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE connections (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    host TEXT NOT NULL DEFAULT '127.0.0.1',
                    port INTEGER NOT NULL DEFAULT 8080,
                    auto_reconnect INTEGER NOT NULL DEFAULT 0,
                    heartbeat_interval INTEGER NOT NULL DEFAULT 30,
                    terminal_phone TEXT NOT NULL DEFAULT '013900000001',
                    auth_code TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
                    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
                );
                INSERT INTO connections (id, name, host, port, auto_reconnect, heartbeat_interval, terminal_phone, auth_code)
                VALUES ('legacy-1', '旧连接', '127.0.0.1', 5100, 0, 30, '013900000001', '');",
            )
            .unwrap();
        }

        let db = Database::open(&path_str).unwrap();
        let loaded = db.load_connections().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "legacy-1");
        assert_eq!(loaded[0].protocol_version, ProtocolVersion::V2013);
        assert_eq!(loaded[0].imei, "");
        assert_eq!(loaded[0].software_version, "V1.0.0");

        drop(db);
        let _ = std::fs::remove_file(&path);
    }
}
