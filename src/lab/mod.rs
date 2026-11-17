//! Optional workspace experiments. Nothing opens this store during ordinary startup.
pub mod api;
pub mod behavior;
pub mod compare;
pub mod context;
pub mod contracts;
#[cfg(test)]
mod integration_tests;
pub mod replay;
pub mod runtime;
pub mod scenarios;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    time::Duration,
};
pub type Result<T> = std::result::Result<T, String>;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub version: i64,
    pub created: String,
    pub data: Value,
}
pub struct Store {
    connection: Connection,
    project: String,
    path: PathBuf,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        Self::at(&crate::persistence::config_dir().join("lab.sqlite3"), root)
    }
    pub fn read_for_peer(peer: &crate::team::Peer) -> Result<Option<Self>> {
        let path = crate::persistence::config_dir().join("lab.sqlite3");
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Ok(meta) if meta.is_file() => {}
            _ => return Err("Lab store is not a regular private file".into()),
        }
        let connection =
            Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(error)?;
        connection
            .busy_timeout(Duration::from_millis(100))
            .map_err(error)?;
        Ok(Some(Self {
            connection,
            project: peer.project.clone(),
            path,
        }))
    }
    pub fn at(path: &Path, root: &Path) -> Result<Self> {
        if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Lab database cannot be a symlink".into());
        }
        let parent = path.parent().ok_or("Lab store needs a parent")?;
        fs::create_dir_all(parent).map_err(error)?;
        let connection = Connection::open(path).map_err(error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(error)?;
        }
        connection
            .busy_timeout(Duration::from_millis(500))
            .map_err(error)?;
        let schema: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(error)?;
        if schema > 1 {
            return Err(
                "Lab storage was created by a newer application; update OsmoGrep to open it".into(),
            );
        }
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA user_version=1; CREATE TABLE IF NOT EXISTS lab_records(project TEXT NOT NULL,id TEXT NOT NULL,kind TEXT NOT NULL,title TEXT NOT NULL,version INTEGER NOT NULL,created TEXT NOT NULL,data TEXT NOT NULL,PRIMARY KEY(project,id)); CREATE INDEX IF NOT EXISTS lab_recent ON lab_records(project,kind,created);").map_err(error)?;
        Ok(Self {
            connection,
            project: crate::team::project_key(root)?,
            path: path.to_path_buf(),
        })
    }
    pub fn job_lock(&self, id: &str) -> Result<fs::File> {
        uuid::Uuid::parse_str(id).map_err(|_| "Invalid Lab job UUID")?;
        crate::storage::lock_file(&self.path.with_extension(format!("{id}.lock")))
    }
    pub fn resource_lock(&self, resource: &str) -> Result<fs::File> {
        crate::storage::lock_file(&self.path.with_extension(format!(
            "resource-{}.lock",
            crate::file_guard::digest(resource.as_bytes())
        )))
    }
    pub fn put(&self, kind: &str, title: &str, data: Value) -> Result<Record> {
        if !matches!(
            kind,
            "replay"
                | "comparison"
                | "contract"
                | "behavior"
                | "context"
                | "scenario"
                | "execution"
        ) {
            return Err("Unknown Lab record kind".into());
        }
        let title = title.trim();
        if crate::intelligence::cli::redact(title) != title {
            return Err("Remove credentials from the report title".into());
        }
        if title.is_empty() || title.len() > 200 || title.chars().any(char::is_control) {
            return Err("Use a nonempty title of at most 200 bytes".into());
        }
        let data = serde_json::to_string(&data).map_err(error)?;
        if data.len() > MAX_BYTES {
            return Err("Lab record exceeds 8 MiB".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created = chrono::Utc::now().to_rfc3339();
        self.connection
            .execute(
                "INSERT INTO lab_records VALUES (?1,?2,?3,?4,1,?5,?6)",
                params![self.project, id, kind, title, created, data],
            )
            .map_err(error)?;
        self.get(&id)
    }
    pub fn get(&self, id: &str) -> Result<Record> {
        uuid::Uuid::parse_str(id).map_err(|_| "Use a valid Lab record UUID")?;
        self.connection.query_row("SELECT id,kind,title,version,created,data FROM lab_records WHERE project=?1 AND id=?2",params![self.project,id],|row|{
            let data:String=row.get(5)?;
            Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,data))
        }).optional().map_err(error)?.map(|(id,kind,title,version,created,data)|Ok(Record{id,kind,title,version,created,data:serde_json::from_str(&data).map_err(error)?})).unwrap_or_else(||Err("Lab record not found in this project".into()))
    }
    pub fn list(&self, kind: Option<&str>, before: Option<i64>) -> Result<Vec<Value>> {
        let mut query=self.connection.prepare("SELECT rowid,id,kind,title,version,created FROM lab_records WHERE project=?1 AND (?2 IS NULL OR kind=?2) AND rowid<?3 ORDER BY rowid DESC LIMIT 200").map_err(error)?;
        let rows=query.query_map(params![self.project,kind,before.unwrap_or(i64::MAX)],|r|Ok(json!({"cursor":r.get::<_,i64>(0)?,"id":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?,"title":r.get::<_,String>(3)?,"version":r.get::<_,i64>(4)?,"created":r.get::<_,String>(5)?}))).map_err(error)?;
        rows.map(|value| value.map_err(error)).collect()
    }
    pub fn update(&self, id: &str, version: i64, data: Value) -> Result<Record> {
        if version < 1 || version == i64::MAX {
            return Err("Invalid record version".into());
        }
        let data = serde_json::to_string(&data).map_err(error)?;
        if data.len() > MAX_BYTES {
            return Err("Lab record exceeds 8 MiB".into());
        }
        let changed=self.connection.execute("UPDATE lab_records SET data=?1,version=version+1 WHERE project=?2 AND id=?3 AND version=?4",params![data,self.project,id,version]).map_err(error)?;
        if changed != 1 {
            return Err("Record changed in another session; reload before updating".into());
        }
        self.get(id)
    }
}
pub fn short_title(value: &str) -> &str {
    let mut end = value.len().min(160);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}
pub fn error(value: impl std::fmt::Display) -> String {
    value.to_string()
}
pub fn safe_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 1024
        || path.contains(['\\', '"'])
        || path.chars().any(char::is_control)
        || !Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err("Use a relative workspace path without traversal or control characters".into());
    }
    if Path::new(path).components().any(|c| {
        let part = c.as_os_str().to_string_lossy().to_ascii_lowercase();
        part == ".git"
            || part == ".context"
            || part == ".aws"
            || part == ".ssh"
            || part.starts_with(".env")
            || matches!(
                part.as_str(),
                "credentials" | "credentials.json" | "id_rsa" | "id_ed25519"
            )
            || part.ends_with(".pem")
            || part.ends_with(".key")
    }) {
        return Err("Private credentials and internal metadata cannot be captured".into());
    }
    Ok(())
}
pub fn bounded_file(path: &Path, max: usize) -> Result<Vec<u8>> {
    if !fs::symlink_metadata(path).map_err(error)?.is_file() {
        return Err("Expected a regular file".into());
    }
    let mut bytes = vec![];
    fs::File::open(path)
        .map_err(error)?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() > max {
        return Err(format!("File exceeds {max} bytes"));
    }
    Ok(bytes)
}
pub fn clean_value(value: Value) -> Result<Value> {
    serde_json::from_str(&crate::intelligence::cli::redact(&value.to_string())).map_err(error)
}
pub fn write_export(root: &Path, path: &str, value: &Value) -> Result<PathBuf> {
    write_export_bytes(
        root,
        path,
        serde_json::to_string_pretty(value)
            .map_err(error)?
            .as_bytes(),
    )
}
pub fn write_export_bytes(root: &Path, path: &str, bytes: &[u8]) -> Result<PathBuf> {
    if bytes.len() > MAX_BYTES {
        return Err("Export exceeds 8 MiB".into());
    }
    safe_path(path)?;
    let path = crate::file_guard::workspace_path(root, Path::new(path), true)?;
    if path.exists() {
        return Err("Export destination already exists; choose a new file".into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error)?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    use std::io::Write;
    let mut file = options.open(&path).map_err(error)?;
    file.write_all(bytes).map_err(error)?;
    file.sync_all().map_err(error)?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_are_project_scoped_and_updates_do_not_clobber_peers() {
        let root = std::env::temp_dir().join(format!("lab-store-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir_all(root.join("b")).unwrap();
        let a = Store::at(&root.join("lab.sqlite3"), &root.join("a")).unwrap();
        let b = Store::at(&root.join("lab.sqlite3"), &root.join("b")).unwrap();
        let record = a.put("contract", "API", json!({"value":1})).unwrap();
        assert!(b.get(&record.id).is_err());
        assert_eq!(
            a.update(&record.id, 1, json!({"value":2})).unwrap().version,
            2
        );
        assert!(a.update(&record.id, 1, json!({})).is_err());
        assert_eq!(a.get(&record.id).unwrap().data["value"], 2);
        drop(a);
        drop(b);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn private_and_traversing_paths_are_rejected() {
        for path in [
            "../a",
            "/a",
            ".env",
            "a/.git/config",
            ".ssh/id_rsa",
            "a\\b",
            "x\nfile",
        ] {
            assert!(safe_path(path).is_err(), "{path}");
        }
        assert!(safe_path("src/main.rs").is_ok());
    }
}
