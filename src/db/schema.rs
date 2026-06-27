/// SQLite schema definitions
pub const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS connections (
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

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id TEXT NOT NULL,
    direction TEXT NOT NULL CHECK(direction IN ('send', 'recv')),
    msg_id INTEGER,
    msg_name TEXT,
    raw_hex TEXT NOT NULL,
    parsed_json TEXT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    FOREIGN KEY (connection_id) REFERENCES connections(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS quick_commands (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    msg_id INTEGER NOT NULL,
    params_json TEXT NOT NULL DEFAULT '{}',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR IGNORE INTO app_settings (key, value) VALUES ('encoding', 'GBK');
INSERT OR IGNORE INTO app_settings (key, value) VALUES ('theme', 'dark');
";
