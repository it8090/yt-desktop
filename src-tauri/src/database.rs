use crate::models::*;
use rusqlite::{params, Connection};
use std::{path::Path, sync::Mutex};

pub struct Database {
    conn: Mutex<Connection>,
}
impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; CREATE TABLE IF NOT EXISTS kv(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY,dedupe TEXT NOT NULL,status TEXT NOT NULL,retry_at INTEGER,created_at INTEGER NOT NULL,data TEXT NOT NULL); CREATE INDEX IF NOT EXISTS jobs_status ON jobs(status,retry_at,created_at); CREATE INDEX IF NOT EXISTS jobs_dedupe ON jobs(dedupe); CREATE TABLE IF NOT EXISTS sources(id TEXT PRIMARY KEY,updated_at INTEGER NOT NULL,data TEXT NOT NULL);")?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn.prepare("SELECT value FROM kv WHERE key=?1")?;
        let mut rows = st.query([key])?;
        match rows.next()? {
            Some(row) => Ok(Some(serde_json::from_str(&row.get::<_, String>(0)?)?)),
            None => Ok(None),
        }
    }
    pub fn set<T: serde::Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.lock().unwrap().execute("INSERT INTO kv(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,serde_json::to_string(value)?])?;
        Ok(())
    }
    pub fn save_job(&self, job: &Job) -> Result<()> {
        self.conn.lock().unwrap().execute("INSERT INTO jobs(id,dedupe,status,retry_at,created_at,data) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET status=excluded.status,retry_at=excluded.retry_at,data=excluded.data",params![job.id,job.dedupe_key,job.status,job.retry_at,job.created_at,serde_json::to_string(job)?])?;
        Ok(())
    }
    pub fn job(&self, id: &str) -> Result<Job> {
        let conn = self.conn.lock().unwrap();
        let data: String = conn
            .query_row("SELECT data FROM jobs WHERE id=?1", [id], |r| r.get(0))
            .map_err(|_| AppError::new("not_found", "Task not found"))?;
        Ok(serde_json::from_str(&data)?)
    }
    pub fn jobs(&self) -> Result<Vec<Job>> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn.prepare("SELECT data FROM jobs ORDER BY created_at DESC,rowid DESC")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        let mut jobs = vec![];
        for row in rows {
            jobs.push(serde_json::from_str(&row?)?);
        }
        Ok(jobs)
    }
    pub fn eligible(&self) -> Result<Vec<Job>> {
        let conn = self.conn.lock().unwrap();
        let mut st=conn.prepare("SELECT data FROM jobs WHERE status='queued' OR (status='retry_wait' AND retry_at<=?1) ORDER BY created_at,rowid LIMIT 16")?;
        let rows = st.query_map([now()], |r| r.get::<_, String>(0))?;
        let mut out = vec![];
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }
    pub fn existing(&self, key: &str) -> Result<Vec<Job>> {
        let conn = self.conn.lock().unwrap();
        let mut st =
            conn.prepare("SELECT data FROM jobs WHERE dedupe=?1 ORDER BY created_at DESC")?;
        let rows = st.query_map([key], |r| r.get::<_, String>(0))?;
        let mut out = vec![];
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }
    pub fn save_source(&self, source: &Source) -> Result<()> {
        self.conn.lock().unwrap().execute("INSERT INTO sources(id,updated_at,data) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at,data=excluded.data",params![source.id,source.updated_at,serde_json::to_string(source)?])?;
        Ok(())
    }
    pub fn sources(&self) -> Result<Vec<Source>> {
        let conn = self.conn.lock().unwrap();
        let mut st =
            conn.prepare("SELECT data FROM sources ORDER BY updated_at DESC,rowid DESC LIMIT 20")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = vec![];
        for row in rows {
            out.push(serde_json::from_str(&row?)?);
        }
        Ok(out)
    }
    pub fn recover(&self) -> Result<()> {
        for mut j in self.jobs()? {
            if j.is_active() {
                j.status = "paused".into();
                j.speed = None;
                j.eta = None;
                j.updated_at = now();
                j.error = Some(AppError::new(
                    "interrupted",
                    "Task can be resumed after restart",
                ));
                self.save_job(&j)?;
            }
        }
        Ok(())
    }
}
