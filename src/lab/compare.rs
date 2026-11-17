//! Compare immutable candidates; applying one uses the existing staged patch writer.
use super::{
    error,
    replay::{Capsule, Source},
    runtime, Record, Result, Store,
};
use crate::intelligence::verify::Sandbox;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path, time::Instant};
pub fn plan(store: &Store, title: &str, left: &str, right: &str) -> Result<Record> {
    if left == right {
        return Err("Choose two different candidate reports".into());
    }
    let a = Capsule::from_record(&store.get(left)?)?;
    let b = Capsule::from_record(&store.get(right)?)?;
    if a.base != b.base
        || serde_json::to_value(&a.experiment).map_err(error)?
            != serde_json::to_value(&b.experiment).map_err(error)?
        || a.environment != b.environment
    {
        return Err("Candidates must have the same base, identical reproducer, independent oracle and environment".into());
    }
    store.put("comparison",title,json!({"left":left,"right":right,"state":"planned","base":a.base,"experiment_sha256":crate::file_guard::digest(a.experiment.source.as_bytes())}))
}
pub(crate) fn summary(result: &Value) -> Value {
    let evidence = &result["data"]["evidence"];
    let candidate = &evidence["comparison"]["candidate"];
    let exit = candidate["exit_code"].as_i64();
    let state = if result["data"]["state"] == "failed" {
        "execution_failed"
    } else if evidence["integrity_attested"] != true
        || exit.is_none()
        || candidate["timed_out"] != false
        || candidate["cancelled"] != false
    {
        "inconclusive"
    } else if exit == Some(0) {
        "check_passed"
    } else if candidate["failure_kind"] == "assertion" {
        "check_failed"
    } else {
        "execution_failed"
    };
    json!({"execution":result["id"],"state":state,"exit_code":exit,"command_duration_ms":candidate["duration_ms"],"environment_fingerprint":evidence["comparison"]["environment_fingerprint"]})
}
pub fn run(
    root: &Path,
    store: &Store,
    report: &Record,
    worker: &mut dyn Sandbox,
    cancel: &dyn Fn() -> bool,
) -> Result<Record> {
    if report.kind != "comparison" {
        return Err("Choose a comparison plan".into());
    }
    let _lock = store.job_lock(&report.id)?;
    let fresh = store.get(&report.id)?;
    let mut data = fresh.data.clone();
    data.as_object_mut()
        .ok_or("Invalid comparison data")?
        .remove("left_result");
    data.as_object_mut().unwrap().remove("right_result");
    data["state"] = json!("running");
    let mut claimed = store.update(&fresh.id, fresh.version, data.clone())?;
    let mut snapshots = vec![];
    for side in ["left", "right"] {
        if cancel() {
            data["state"] = json!("cancelled");
            break;
        }
        let id = data[side]
            .as_str()
            .ok_or("Comparison has no candidate ID")?;
        let source = store.get(id)?;
        let capsule = Capsule::from_record(&source)?;
        let started = Instant::now();
        let outcome = super::replay::execution(
            store,
            &source,
            runtime::replay(root, &capsule, worker, cancel),
        )?;
        let mut result = summary(&json!(outcome));
        result["wall_ms"] = json!(started.elapsed().as_millis() as u64);
        data[side.to_string() + "_result"] = result.clone();
        snapshots.push(result);
        claimed = store.update(
            &claimed.id,
            claimed.version,
            super::clean_value(data.clone())?,
        )?;
    }
    if data["state"] == "running" {
        data["state"] = json!("finished");
    }
    data["same_environment_attested"] = json!(
        snapshots.len() == 2
            && snapshots[0]["environment_fingerprint"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
            && snapshots[0]["environment_fingerprint"] == snapshots[1]["environment_fingerprint"]
    );
    data["selection"] = Value::Null;
    data["note"]=json!("No automatic winner or merge. Wall time includes snapshot preparation and VM interaction; unavailable measurements are null.");
    store.update(&claimed.id, claimed.version, super::clean_value(data)?)
}
pub fn expand_apply(root: &Path, mut args: Value) -> Result<Value> {
    if args["action"] != "apply" {
        return Ok(args);
    }
    let store = Store::open(root)?;
    let id = args["id"].as_str().ok_or("Select a captured candidate")?;
    let capsule = Capsule::from_record(&store.get(id)?)?;
    args["data"] = json!({"files":capsule.files});
    Ok(args)
}
pub fn apply(root: &Path, files: &BTreeMap<String, Source>) -> Result<Value> {
    if crate::sandbox::mode() == "read-only" {
        return Err("Read-only sandbox denies applying a candidate".into());
    }
    if files.is_empty() || files.len() > 64 {
        return Err("Choose 1–64 candidate files".into());
    }
    let mut staged = vec![];
    let mut before_bytes = 0usize;
    for (path, source) in files {
        super::safe_path(path)?;
        let absolute = crate::file_guard::workspace_path(root, Path::new(path), true)?;
        let before = match fs::symlink_metadata(&absolute) {
            Ok(_) => Some(
                String::from_utf8(super::bounded_file(&absolute, 256 * 1024)?).map_err(error)?,
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(error(e)),
        };
        before_bytes += before.as_ref().map_or(0, String::len);
        if before_bytes > 2 * 1024 * 1024 {
            return Err("Candidate base files exceed the 2 MiB application budget".into());
        }
        if before
            .as_ref()
            .map(|text| crate::file_guard::digest(text.as_bytes()))
            != source.expected_before
            || crate::file_guard::digest(source.content.as_bytes()) != source.sha256
        {
            return Err(format!("{path} changed since capture; user edits were preserved. Capture a new candidate against the current workspace."));
        }
        if before.as_deref() != Some(&source.content) {
            staged.push((path.clone(), before, Some(source.content.clone())));
        }
    }
    let args = json!({"action":"apply","data":{"files":files}});
    let mut checkpoint =
        crate::checkpoints::Turn::start(root, &uuid::Uuid::new_v4().to_string(), vec![])?;
    checkpoint.capture(root, "lab", &args)?;
    let mut result = crate::tools::patch::apply_files(staged)?;
    // Preserve a restore point even if an external save interrupts a multi-file apply.
    let changes = files
        .iter()
        .filter_map(|(path, source)| {
            let after = fs::read_to_string(root.join(path)).ok()?;
            Some(json!({"path":path,"after":after,"created":source.expected_before.is_none()}))
        })
        .collect::<Vec<_>>();
    checkpoint.complete(root, "lab", &args, &json!({"changes":changes}))?;
    result["checkpoint"] = json!(checkpoint.id);
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_candidate_application_preserves_external_edits() {
        let root = std::env::temp_dir().join(format!("lab-apply-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "external edit").unwrap();
        let files = BTreeMap::from([(
            "a.txt".into(),
            Source {
                content: "candidate".into(),
                sha256: crate::file_guard::digest(b"candidate"),
                expected_before: Some(crate::file_guard::digest(b"old")),
            },
        )]);
        assert!(apply(&root, &files).unwrap_err().contains("preserved"));
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "external edit"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod apply_tests {
    use super::*;
    #[test]
    fn selected_bytes_apply_exactly_and_have_a_conflict_aware_restore_point() {
        let root = std::env::temp_dir().join(format!("lab-apply-exact-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "original").unwrap();
        let files = BTreeMap::from([(
            "a.txt".into(),
            Source {
                content: "selected without a final newline".into(),
                sha256: crate::file_guard::digest(b"selected without a final newline"),
                expected_before: Some(crate::file_guard::digest(b"original")),
            },
        )]);
        crate::tools::in_root(&root, || {
            let result = apply(&root, &files)?;
            assert!(result.get("error").is_none(), "{result}");
            assert_eq!(
                fs::read(root.join("a.txt")).unwrap(),
                b"selected without a final newline"
            );
            let turn = crate::checkpoints::load(&root, result["checkpoint"].as_str().unwrap())?;
            assert_eq!(crate::checkpoints::restore_files(&root, &turn)?, 1);
            assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "original");
            Ok::<(), String>(())
        })
        .unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
