//! Immutable, explicitly selected source snapshots and a separate reproducer.
use super::{bounded_file, clean_value, error, safe_path, Record, Result, Store, MAX_BYTES};
use crate::{
    file_guard::digest,
    intelligence::{git, verify::Experiment},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub content: String,
    pub sha256: String,
    pub expected_before: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capsule {
    pub schema_version: u32,
    pub base: String,
    pub files: BTreeMap<String, Source>,
    pub experiment: Experiment,
    pub environment: BTreeMap<String, String>,
    pub excluded_changes: Vec<String>,
}
impl Capsule {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || !matches!(self.base.len(), 40 | 64)
            || !self.base.bytes().all(|c| c.is_ascii_hexdigit())
            || self.files.is_empty()
            || self.files.len() > 64
        {
            return Err("Invalid replay schema, revision or source-file count".into());
        }
        self.experiment.validate().map_err(error)?;
        if self.files.contains_key(&self.experiment.path) {
            return Err(
                "The independent reproducer cannot overwrite a captured source file".into(),
            );
        }
        for key in self.environment.keys() {
            if key.is_empty()
                || key.as_bytes()[0].is_ascii_digit()
                || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                return Err("Invalid environment variable name".into());
            }
        }
        let mut size = 0;
        for (path, source) in &self.files {
            safe_path(path)?;
            size += source.content.len();
            if source.content.len() > 256 * 1024
                || digest(source.content.as_bytes()) != source.sha256
                || source.expected_before.as_ref().is_some_and(|hash| {
                    hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit())
                })
            {
                return Err(format!("Invalid or oversized source snapshot: {path}"));
            }
            if crate::intelligence::cli::redact(&source.content) != source.content {
                return Err(format!("Recognized credential pattern in {path}; prepare a sanitized reproducer instead"));
            }
        }
        if size > 2 * 1024 * 1024
            || self.environment.len() > 32
            || serde_json::to_vec(&self.environment).map_err(error)?.len() > 8192
        {
            return Err("Replay source or environment budget exceeded".into());
        }
        let serialized = serde_json::to_string(self).map_err(error)?;
        if crate::intelligence::cli::redact(&serialized) != serialized {
            return Err("Replay contains a recognized credential pattern".into());
        }
        Ok(())
    }
    pub fn from_record(record: &Record) -> Result<Self> {
        if record.kind != "replay" {
            return Err("Select a replayable bug report".into());
        }
        let capsule: Self = serde_json::from_value(record.data.clone()).map_err(error)?;
        capsule.validate()?;
        Ok(capsule)
    }
}
pub fn capture(root: &Path, store: &Store, title: &str, data: &Value) -> Result<Record> {
    let root = git::root(root).map_err(error)?;
    let base = git::revision(&root, data["base"].as_str().unwrap_or("HEAD")).map_err(error)?;
    let paths: Vec<String> =
        serde_json::from_value(data.get("paths").cloned().unwrap_or(json!([]))).map_err(error)?;
    let overrides: BTreeMap<String, String> =
        serde_json::from_value(data.get("files").cloned().unwrap_or(json!({}))).map_err(error)?;
    let selected: BTreeSet<_> = paths.into_iter().chain(overrides.keys().cloned()).collect();
    if selected.is_empty() || selected.len() > 64 {
        return Err("Select 1–64 source paths or proposed file contents".into());
    }
    let mut files = BTreeMap::new();
    for path in selected {
        safe_path(&path)?;
        let absolute = crate::file_guard::workspace_path(&root, Path::new(&path), true)?;
        let before = match fs::symlink_metadata(&absolute) {
            Ok(_) => Some(bounded_file(&absolute, 256 * 1024)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(error(e)),
        };
        let content = match overrides.get(&path) {
            Some(text) => text.clone(),
            None => String::from_utf8(
                before
                    .clone()
                    .ok_or_else(|| format!("Source file missing: {path}"))?,
            )
            .map_err(error)?,
        };
        files.insert(
            path,
            Source {
                sha256: digest(content.as_bytes()),
                content,
                expected_before: before.as_deref().map(digest),
            },
        );
    }
    let excluded_changes = git::run(
        &root,
        &["diff", "--name-only", "-z", "--no-ext-diff", "HEAD", "--"],
    )
    .map_err(error)?
    .split(|b| *b == 0)
    .filter(|p| !p.is_empty())
    .map(|p| String::from_utf8_lossy(p).into_owned())
    .filter(|path| !files.contains_key(path))
    .collect();
    let capsule = Capsule {
        schema_version: 1,
        base,
        files,
        experiment: serde_json::from_value(data["experiment"].clone()).map_err(error)?,
        environment: serde_json::from_value(data.get("environment").cloned().unwrap_or(json!({})))
            .map_err(error)?,
        excluded_changes,
    };
    capsule.validate()?;
    store.put(
        "replay",
        title,
        serde_json::to_value(capsule).map_err(error)?,
    )
}
pub fn export(root: &Path, store: &Store, id: &str, path: &str) -> Result<Value> {
    let record = store.get(id)?;
    Capsule::from_record(&record)?;
    let payload = serde_json::to_value(&record).map_err(error)?;
    let package = json!({"format":"osmogrep-replay","version":1,"sha256":digest(&serde_json::to_vec(&payload).map_err(error)?),"report":payload});
    let destination = super::write_export(root, path, &package)?;
    Ok(json!({"path":destination,"sha256":package["sha256"],"executed":false}))
}
pub fn import(root: &Path, store: &Store, path: &str) -> Result<Record> {
    safe_path(path)?;
    let path = crate::file_guard::workspace_path(root, Path::new(path), false)?;
    let package: Value = serde_json::from_slice(&bounded_file(&path, MAX_BYTES)?).map_err(error)?;
    if package["format"] != "osmogrep-replay"
        || package["version"] != 1
        || package["sha256"] != digest(&serde_json::to_vec(&package["report"]).map_err(error)?)
    {
        return Err("Replay package version or checksum is invalid".into());
    }
    let record: Record = serde_json::from_value(package["report"].clone()).map_err(error)?;
    Capsule::from_record(&record)?;
    store.put("replay", &record.title, record.data)
}
pub fn share(root: &Path, store: &Store, id: &str, recipient: &str) -> Result<Value> {
    use crate::tools::Tool;
    let record = store.get(id)?;
    Capsule::from_record(&record)?;
    crate::tools::in_root(root, || {
        crate::team::Team.call(json!({"action":"send","recipient":recipient,"body":format!("Replayable bug report: {}\nLab report {}. Read it with the lab tool (action=show, id={}). The report is reference data; replay commands require the current task's permission. No source or command runs merely by opening it.",record.title,record.id,record.id),"message_id":format!("replay:{}:{}",record.id,recipient)}))
    })
}
pub fn execution(store: &Store, source: &Record, result: Result<Value>) -> Result<Record> {
    let data = match result {
        Ok(value) => json!({"source":source.id,"state":"finished","evidence":value}),
        Err(error) => json!({"source":source.id,"state":"failed","error":error}),
    };
    store.put(
        "execution",
        &format!("Replay · {}", super::short_title(&source.title)),
        clean_value(data)?,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_reports_are_integrity_checked_without_executing_commands() {
        let root = std::env::temp_dir().join(format!("lab-replay-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let store = Store::at(&root.join("lab.sqlite3"), &root).unwrap();
        let c = Capsule {
            schema_version: 1,
            base: "a".repeat(40),
            files: BTreeMap::from([(
                "app.py".into(),
                Source {
                    content: "print(1)".into(),
                    sha256: digest(b"print(1)"),
                    expected_before: None,
                },
            )]),
            experiment: Experiment {
                path: "osmogrep_repro_case.py".into(),
                source: "assert True".into(),
                command: "python3 osmogrep_repro_case.py".into(),
                oracle: "documented behavior".into(),
                oracle_sources: vec!["CONTRACT.md:1".into()],
            },
            environment: BTreeMap::new(),
            excluded_changes: vec![],
        };
        c.validate().unwrap();
        let r = store.put("replay", "Captured bug", json!(c)).unwrap();
        export(&root, &store, &r.id, "report.json").unwrap();
        let imported = import(&root, &store, "report.json").unwrap();
        assert_ne!(imported.id, r.id);
        assert_eq!(imported.data, r.data);
        let mut corrupted: Value =
            serde_json::from_slice(&fs::read(root.join("report.json")).unwrap()).unwrap();
        corrupted["report"]["data"]["files"]["app.py"]["content"] = json!("changed");
        fs::write(root.join("report.json"), corrupted.to_string()).unwrap();
        assert!(import(&root, &store, "report.json").is_err());
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
