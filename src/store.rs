//! The action history in `~/.local/state/niruflow/history.sqlite3` (the same
//! table NIRUFLOW 0.x wrote, so old quarantine entries stay restorable).
use crate::config::{state_dir, R};
use rusqlite::Connection;
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct Action {
    pub id: i64,
    pub ts: String,
    pub rule: String,
    pub source: String,
    pub destination: String,
    pub status: String,
    pub bytes: i64,
    pub message: String,
}

impl Action {
    pub fn to_json(&self) -> Value {
        json!([
            self.id,
            self.ts,
            self.rule,
            self.source,
            self.destination,
            self.status,
            self.bytes,
            self.message
        ])
    }
}

fn connect() -> R<Connection> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let db = Connection::open(dir.join("history.sqlite3")).map_err(|e| e.to_string())?;
    db.busy_timeout(std::time::Duration::from_secs(20))
        .map_err(|e| e.to_string())?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS actions(
       id INTEGER PRIMARY KEY, ts TEXT NOT NULL, rule TEXT NOT NULL,
       source TEXT NOT NULL, destination TEXT NOT NULL, status TEXT NOT NULL,
       bytes INTEGER NOT NULL DEFAULT 0, message TEXT NOT NULL DEFAULT '')",
    )
    .map_err(|e| e.to_string())?;
    Ok(db)
}

pub fn record(
    rule: &str,
    source: &str,
    destination: &str,
    status: &str,
    size: i64,
    message: &str,
) -> R<()> {
    let db = connect()?;
    db.execute(
        "INSERT INTO actions(ts,rule,source,destination,status,bytes,message) VALUES(?,?,?,?,?,?,?)",
        rusqlite::params![
            crate::fmt::iso_utc(crate::fmt::now_f64()),
            rule,
            source,
            destination,
            status,
            size,
            message
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn history(limit: i64) -> R<Vec<Action>> {
    let db = connect()?;
    let mut st = db
        .prepare("SELECT id,ts,rule,source,destination,status,bytes,message FROM actions ORDER BY id DESC LIMIT ?")
        .map_err(|e| e.to_string())?;
    let rows = st
        .query_map([limit], |r| {
            Ok(Action {
                id: r.get(0)?,
                ts: r.get(1)?,
                rule: r.get(2)?,
                source: r.get(3)?,
                destination: r.get(4)?,
                status: r.get(5)?,
                bytes: r.get(6)?,
                message: r.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
