//! Explicit fault cases run in fresh VM snapshots, never in the user's app process.
use super::{error, replay::Capsule, Record, Result, Store};
use crate::intelligence::verify::Sandbox;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub name: String,
    pub environment: BTreeMap<String, String>,
    #[serde(default = "once")]
    pub repetitions: u32,
}
fn once() -> u32 {
    1
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub replay: String,
    pub cases: Vec<Case>,
}
fn validated(store: &Store, data: &Value) -> Result<(Plan, Capsule)> {
    let plan: Plan = serde_json::from_value(data.clone()).map_err(error)?;
    if plan.cases.is_empty()
        || plan.cases.len() > 16
        || plan
            .cases
            .iter()
            .map(|c| u64::from(c.repetitions))
            .sum::<u64>()
            > 32
    {
        return Err("A campaign needs 1–16 named cases and at most 32 isolated runs".into());
    }
    let capsule = Capsule::from_record(&store.get(&plan.replay)?)?;
    let mut names = std::collections::BTreeSet::new();
    for case in &plan.cases {
        if case.repetitions == 0
            || case.name.trim().is_empty()
            || case.name.len() > 80
            || case.name.chars().any(char::is_control)
            || !names.insert(&case.name)
        {
            return Err("Use distinct, nonempty case names and positive repetitions".into());
        }
        let mut sample = capsule.clone();
        sample.environment.extend(case.environment.clone());
        sample.validate()?;
        for key in case.environment.keys() {
            if key.is_empty()
                || key.as_bytes()[0].is_ascii_digit()
                || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                return Err("Fault environment keys must be shell-safe variable names".into());
            }
        }
    }
    Ok((plan, capsule))
}
pub fn plan(store: &Store, title: &str, data: &Value) -> Result<Record> {
    let (plan, _) = validated(store, data)?;
    store.put("scenario",title,json!({"plan":plan,"state":"planned","boundary":"Fresh isolated VM per run; environment variants and reproducer-controlled faults only. No host network, clock or device changes."}))
}
pub fn run(
    root: &Path,
    store: &Store,
    source: &Record,
    worker: &mut dyn Sandbox,
    cancel: &dyn Fn() -> bool,
) -> Result<Record> {
    if source.kind != "scenario" {
        return Err("Select a failure campaign".into());
    }
    let _lock = store.job_lock(&source.id)?;
    let (plan, base) = validated(store, &source.data["plan"])?;
    let mut data = source.data.clone();
    data["state"] = json!("running");
    data["results"] = json!([]);
    let mut current = store.get(&source.id)?;
    current = store.update(&current.id, current.version, data.clone())?;
    'cases: for case in plan.cases {
        for repeat in 0..case.repetitions {
            if cancel() {
                break 'cases;
            }
            let mut capsule = base.clone();
            capsule.environment.extend(case.environment.clone());
            let replay = store.put(
                "replay",
                &format!("Fault case · {} · {}", case.name, repeat + 1),
                json!(capsule),
            )?;
            let execution = super::replay::execution(
                store,
                &replay,
                super::runtime::replay(root, &capsule, worker, cancel),
            )?;
            let outcome = super::compare::summary(&json!(execution));
            data["results"].as_array_mut().unwrap().push(
                json!({"case":case.name,"repeat":repeat+1,"replay":replay.id,"outcome":outcome}),
            );
            current = store.update(
                &current.id,
                current.version,
                super::clean_value(data.clone())?,
            )?;
        }
    }
    data["state"] = json!(if cancel() { "cancelled" } else { "finished" });
    data["note"]=json!("Each recorded case is replayable. A nonzero exit is a check failure only with integrity and completion evidence; missing attestations stay inconclusive. VM close is attempted after every run, including failure and cancellation.");
    store.update(&current.id, current.version, super::clean_value(data)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cases_are_bounded_and_missing_attestations_are_inconclusive() {
        let result = super::super::compare::summary(
            &json!({"id":"x","data":{"state":"finished","evidence":{"comparison":{"candidate":{"exit_code":0,"timed_out":false,"cancelled":false}}}}}),
        );
        assert_eq!(result["state"], "inconclusive");
        assert!(result["command_duration_ms"].is_null());
        let case: Case =
            serde_json::from_value(json!({"name":"timezone","environment":{"TZ":"UTC"}})).unwrap();
        assert_eq!(case.repetitions, 1);
        assert!(serde_json::from_value::<Case>(
            json!({"name":"x","environment":{},"host_command":"shutdown"})
        )
        .is_err());
    }
}
