//! State on disk: SQLite in WAL mode. Items are rows with a JSON body (the schema can grow without
//! migrations for every field) plus the columns queries need. Logs are their own table, capped
//! per item. Writes go through one background thread in batched transactions, so the core never
//! waits on the disk and a crash loses at most the last moment of progress numbers, never an item.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{Item, ItemId};

const SCHEMA_VERSION: i64 = 1;
const LOG_LINES_KEPT: i64 = 400;

#[derive(Debug)]
pub struct StoreError(pub String);

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError(e.to_string())
    }
}

#[derive(Clone, Debug)]
pub enum StoreOp {
    Upsert { item: Box<Item>, position: i64 },
    Delete(ItemId),
    /// The whole queue order.
    Order(Vec<ItemId>),
    Log { id: ItemId, line: String },
    Setting { key: String, value: String },
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StoreError(format!("Can't create {}: {e}", parent.display())))?;
        }
        let conn = Connection::open(path)?;
        Self::prepare(conn)
    }

    pub fn open_in_memory() -> Result<Store, StoreError> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(conn: Connection) -> Result<Store, StoreError> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(StoreError(format!(
                "This state was written by a newer Dawnload (schema {version}); update Dawnload to open it."
            )));
        }
        if version < 1 {
            conn.execute_batch(
                "BEGIN;
                 CREATE TABLE IF NOT EXISTS items (
                     id INTEGER PRIMARY KEY,
                     position INTEGER NOT NULL,
                     status TEXT NOT NULL,
                     data TEXT NOT NULL,
                     updated_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS items_position ON items(position);
                 CREATE TABLE IF NOT EXISTS logs (
                     item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                     seq INTEGER NOT NULL,
                     line TEXT NOT NULL,
                     PRIMARY KEY (item_id, seq)
                 );
                 CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        Ok(Store { conn })
    }

    /// Every item in queue order. Rows that don't parse (a newer field type, damage) are skipped
    /// rather than stopping Dawnload from starting.
    pub fn load_items(&self) -> Result<Vec<Item>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT data FROM items ORDER BY position, id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).filter_map(|json| serde_json::from_str(&json).ok()).collect())
    }

    pub fn load_log(&self, id: ItemId) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT line FROM logs WHERE item_id = ?1 ORDER BY seq")?;
        let rows = stmt.query_map([id.0 as i64], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn apply(&mut self, ops: &[StoreOp]) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        let mut logged = std::collections::HashSet::new();
        {
            let now = crate::model::now_millis();
            let mut upsert = tx.prepare_cached(
                "INSERT INTO items (id, position, status, data, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET position = excluded.position, status = excluded.status,
                 data = excluded.data, updated_at = excluded.updated_at",
            )?;
            let mut log = tx.prepare_cached(
                "INSERT INTO logs (item_id, seq, line)
                 SELECT ?1, COALESCE(MAX(seq), 0) + 1, ?2 FROM logs WHERE item_id = ?1",
            )?;
            for op in ops {
                match op {
                    StoreOp::Upsert { item, position } => {
                        let json = serde_json::to_string(item).map_err(|e| StoreError(e.to_string()))?;
                        upsert.execute(params![item.id.0 as i64, position, item.status.as_str(), json, now])?;
                    }
                    StoreOp::Delete(id) => {
                        tx.execute("DELETE FROM items WHERE id = ?1", [id.0 as i64])?;
                    }
                    StoreOp::Order(ids) => {
                        let mut stmt = tx.prepare_cached("UPDATE items SET position = ?2 WHERE id = ?1")?;
                        for (position, id) in ids.iter().enumerate() {
                            stmt.execute(params![id.0 as i64, position as i64])?;
                        }
                    }
                    StoreOp::Log { id, line } => {
                        // Only for items that exist (a removed item's late lines are dropped).
                        let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM items WHERE id = ?1)", [id.0 as i64], |r| r.get(0))?;
                        if exists {
                            log.execute(params![id.0 as i64, line])?;
                            logged.insert(*id);
                        }
                    }
                    StoreOp::Setting { key, value } => {
                        tx.execute(
                            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                            params![key, value],
                        )?;
                    }
                }
            }
            for id in logged {
                tx.execute(
                    "DELETE FROM logs WHERE item_id = ?1 AND seq <= (SELECT MAX(seq) FROM logs WHERE item_id = ?1) - ?2",
                    params![id.0 as i64, LOG_LINES_KEPT],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

enum WriterMessage {
    Ops(Vec<StoreOp>),
    Flush(mpsc::Sender<()>),
}

/// The background writer. Cloning shares the thread.
#[derive(Clone)]
pub struct StoreWriter {
    tx: mpsc::Sender<WriterMessage>,
}

impl StoreWriter {
    pub fn spawn(mut store: Store) -> StoreWriter {
        let (tx, rx) = mpsc::channel::<WriterMessage>();
        std::thread::Builder::new()
            .name("dawnload-store".into())
            .spawn(move || {
                while let Ok(first) = rx.recv() {
                    let mut ops = Vec::new();
                    let mut flushes = Vec::new();
                    let mut take = |m: WriterMessage| match m {
                        WriterMessage::Ops(o) => ops.extend(o),
                        WriterMessage::Flush(done) => flushes.push(done),
                    };
                    take(first);
                    while let Ok(more) = rx.try_recv() {
                        take(more);
                    }
                    if !ops.is_empty() {
                        if let Err(e) = store.apply(&ops) {
                            eprintln!("dawnload: couldn't save state: {e}");
                        }
                    }
                    for done in flushes {
                        let _ = done.send(());
                    }
                }
            })
            .expect("store thread");
        StoreWriter { tx }
    }

    pub fn send(&self, ops: Vec<StoreOp>) {
        if !ops.is_empty() {
            let _ = self.tx.send(WriterMessage::Ops(ops));
        }
    }

    /// Waits until everything sent so far is on disk.
    pub fn flush_blocking(&self) {
        let (done_tx, done_rx) = mpsc::channel();
        if self.tx.send(WriterMessage::Flush(done_tx)).is_ok() {
            let _ = done_rx.recv();
        }
    }
}

pub fn default_path(data_dir: &Path) -> PathBuf {
    crate::paths::database_path(data_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Options, Status};
    use crate::source::DownloadSource;

    fn item(id: u64) -> Item {
        Item::new(ItemId(id), DownloadSource::parse(&format!("https://example.com/{id}.zip")).unwrap(), Options::default())
    }

    #[test]
    fn round_trips_items_order_logs_and_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite3");
        {
            let mut store = Store::open(&path).unwrap();
            let mut running = item(2);
            running.status = Status::Pulling;
            store
                .apply(&[
                    StoreOp::Upsert { item: Box::new(item(1)), position: 0 },
                    StoreOp::Upsert { item: Box::new(running), position: 1 },
                    StoreOp::Upsert { item: Box::new(item(3)), position: 2 },
                    StoreOp::Order(vec![ItemId(3), ItemId(1), ItemId(2)]),
                    StoreOp::Log { id: ItemId(1), line: "one".into() },
                    StoreOp::Log { id: ItemId(1), line: "two".into() },
                    StoreOp::Log { id: ItemId(99), line: "nobody".into() },
                    StoreOp::Setting { key: "k".into(), value: "v".into() },
                    StoreOp::Delete(ItemId(3)),
                ])
                .unwrap();
        }
        let store = Store::open(&path).unwrap();
        let items = store.load_items().unwrap();
        assert_eq!(items.iter().map(|i| i.id.0).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(items[1].status, Status::Pulling, "the store keeps what it was given; recovery is the core's job");
        assert_eq!(store.load_log(ItemId(1)).unwrap(), vec!["one", "two"]);
        assert_eq!(store.setting("k").unwrap().as_deref(), Some("v"));
    }

    #[test]
    fn caps_logs_per_item() {
        let mut store = Store::open_in_memory().unwrap();
        store.apply(&[StoreOp::Upsert { item: Box::new(item(1)), position: 0 }]).unwrap();
        let ops: Vec<StoreOp> = (0..450).map(|i| StoreOp::Log { id: ItemId(1), line: format!("line {i}") }).collect();
        store.apply(&ops).unwrap();
        let log = store.load_log(ItemId(1)).unwrap();
        assert_eq!(log.len(), LOG_LINES_KEPT as usize);
        assert_eq!(log.last().unwrap(), "line 449");
    }

    #[test]
    fn refuses_a_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.sqlite3");
        Connection::open(&path).unwrap().pragma_update(None, "user_version", 99).unwrap();
        assert!(Store::open(&path).is_err());
    }
}
