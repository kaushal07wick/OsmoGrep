use super::{error, replay, runtime, Result, Store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::Path,
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Help,
    CurrentContext,
    List,
    Show,
    CaptureReplay,
    Export,
    Import,
    Replay,
    Share,
    PlanComparison,
    RunComparison,
    Apply,
    CreateContract,
    AmendContract,
    AcceptContract,
    CheckContract,
    PlanFlow,
    RunFlow,
    CompareBehavior,
    ExportBehavior,
    ConfigureContext,
    CorrectContext,
    InspectContext,
    PlanScenario,
    RunScenario,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub action: Action,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub before: Option<i64>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub recipient: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub data: Value,
    #[serde(default = "timeout")]
    pub timeout_seconds: u64,
}
fn timeout() -> u64 {
    120
}
impl Request {
    fn id(&self) -> Result<&str> {
        self.id.as_deref().ok_or("Select a Lab record ID".into())
    }
    fn path(&self) -> Result<&str> {
        self.path
            .as_deref()
            .ok_or("Choose a relative workspace file path".into())
    }
}
pub fn execute(root: &Path, request: Request, cancel: &dyn Fn() -> bool) -> Result<Value> {
    execute_inner(root, request, cancel, false)
}
pub fn execute_as_user(root: &Path, request: Request, cancel: &dyn Fn() -> bool) -> Result<Value> {
    execute_inner(root, request, cancel, true)
}
fn execute_inner(
    root: &Path,
    request: Request,
    cancel: &dyn Fn() -> bool,
    user: bool,
) -> Result<Value> {
    if cancel() {
        return Err("Lab action cancelled".into());
    }
    if !(1..=1800).contains(&request.timeout_seconds) {
        return Err("Lab deadline must be 1–1,800 seconds".into());
    }
    if matches!(request.action, Action::Help) {
        return Ok(guide(request.kind.as_deref()));
    }
    if matches!(request.action, Action::CurrentContext) {
        return super::context::current().and_then(super::clean_value);
    }
    let deadline = Instant::now() + Duration::from_secs(request.timeout_seconds);
    let stopped = || cancel() || Instant::now() >= deadline;
    let cancel: &dyn Fn() -> bool = &stopped;
    let store = Store::open(root)?;
    let result = match request.action {
        Action::Help => unreachable!("handled before opening storage"),
        Action::CurrentContext => unreachable!("handled before opening storage"),
        Action::ConfigureContext => super::context::configure(root, &store, &request.data)?,
        Action::CorrectContext => json!(super::context::correction(
            root,
            &store,
            request.title.as_deref().unwrap_or("User correction"),
            &request.data
        )?),
        Action::InspectContext => super::context::inspect(root, &store, request.id()?)?,
        Action::PlanFlow => json!(super::behavior::plan(
            &store,
            request.title.as_deref().unwrap_or("App behavior flow"),
            &request.data
        )?),
        Action::RunFlow => {
            let source = store.get(request.id()?)?;
            let flow = super::behavior::Flow::from_record(&source)?;
            let server = super::behavior::connect(root, &flow)?;
            let mut device = super::behavior::Companion {
                server: &server,
                deadline,
                cancel,
            };
            json!(super::behavior::run(
                root,
                &store,
                &source,
                request.data["phase"]
                    .as_str()
                    .ok_or("Select before or after")?,
                request.data["build"]
                    .as_str()
                    .ok_or("Identify the installed build")?,
                &mut device,
                cancel
            )?)
        }
        Action::CompareBehavior => json!(super::behavior::compare(
            &store,
            request
                .title
                .as_deref()
                .unwrap_or("Before / after behavior"),
            request.data["before"]
                .as_str()
                .ok_or("Select the before run")?,
            request.data["after"]
                .as_str()
                .ok_or("Select the after run")?
        )?),
        Action::ExportBehavior => {
            super::behavior::export(root, &store, request.id()?, request.path()?)?
        }
        Action::PlanScenario => json!(super::scenarios::plan(
            &store,
            request.title.as_deref().unwrap_or("App failure campaign"),
            &request.data
        )?),
        Action::RunScenario => {
            let source = store.get(request.id()?)?;
            let server = request
                .server
                .as_deref()
                .ok_or("Select an existing isolated VM service")?;
            let mut vm = runtime::Vm {
                server,
                deadline,
                cancelled: cancel,
            };
            json!(super::scenarios::run(
                root, &store, &source, &mut vm, cancel
            )?)
        }
        Action::CreateContract => json!(super::contracts::create(
            root,
            &store,
            request.title.as_deref().unwrap_or("Code contract"),
            &request.data,
            &super::contracts::actor(root, user)?
        )?),
        Action::AmendContract => json!(super::contracts::amend(
            root,
            &store,
            request.id()?,
            &request.data,
            &super::contracts::actor(root, user)?
        )?),
        Action::AcceptContract => json!(super::contracts::accept(
            &store,
            request.id()?,
            &super::contracts::actor(root, user)?,
            request.data["definition_sha256"]
                .as_str()
                .ok_or("Inspect the contract fingerprint before accepting")?,
            request.data["subscribe"].as_bool().unwrap_or(false)
        )?),
        Action::CheckContract => super::contracts::inspect(root, &store, request.id()?)?,
        Action::PlanComparison => json!(super::compare::plan(
            &store,
            request
                .title
                .as_deref()
                .unwrap_or("Compare candidate fixes"),
            request.data["left"]
                .as_str()
                .ok_or("Choose the left replay report")?,
            request.data["right"]
                .as_str()
                .ok_or("Choose the right replay report")?
        )?),
        Action::RunComparison => {
            let record = store.get(request.id()?)?;
            let server = request
                .server
                .as_deref()
                .ok_or("Choose an isolated VM server")?;
            let mut vm = runtime::Vm {
                server,
                deadline,
                cancelled: cancel,
            };
            json!(super::compare::run(root, &store, &record, &mut vm, cancel)?)
        }
        Action::Apply => {
            let capsule = replay::Capsule::from_record(&store.get(request.id()?)?)?;
            super::compare::apply(root, &capsule.files)?
        }
        Action::List => json!({"records":store.list(request.kind.as_deref(),request.before)?}),
        Action::Show => {
            let record = store.get(request.id()?)?;
            if record.kind == "contract" {
                super::contracts::inspect(root, &store, &record.id)?
            } else if record.kind == "context" {
                super::context::inspect(root, &store, &record.id)?
            } else {
                json!(record)
            }
        }
        Action::CaptureReplay => json!(replay::capture(
            root,
            &store,
            request.title.as_deref().unwrap_or("Captured bug"),
            &request.data
        )?),
        Action::Export => replay::export(root, &store, request.id()?, request.path()?)?,
        Action::Import => json!(replay::import(root, &store, request.path()?)?),
        Action::Share => replay::share(
            root,
            &store,
            request.id()?,
            request
                .recipient
                .as_deref()
                .ok_or("Select a live teammate")?,
        )?,
        Action::Replay => {
            let report = store.get(request.id()?)?;
            let capsule = replay::Capsule::from_record(&report)?;
            let server=request.server.as_deref().ok_or("Choose a configured isolated VM server; repository commands never fall back to the host")?;
            let mut vm = runtime::Vm {
                server,
                deadline,
                cancelled: cancel,
            };
            json!(replay::execution(
                &store,
                &report,
                runtime::replay(root, &capsule, &mut vm, cancel)
            )?)
        }
    };
    super::clean_value(result).map_err(error)
}

pub(crate) fn guide(kind: Option<&str>) -> Value {
    let all = json!({
        "replay":{"action":"capture_replay","title":"Reproduce the reported bug","data":{"paths":["src/example.rs"],"files":{"src/example.rs":"optional proposed contents"},"experiment":{"path":"osmogrep_repro_case.py","source":"independent regression test","command":"python osmogrep_repro_case.py","oracle":"Expected behavior from specification","oracle_sources":["issue URL or original test reference"]},"environment":{"TZ":"UTC"}}},
        "comparison":{"plan":{"action":"plan_comparison","data":{"left":"replay UUID","right":"replay UUID"}},"run":{"action":"run_comparison","id":"plan UUID","server":"configured isolated VM MCP server","timeout_seconds":120},"apply":{"action":"apply","id":"explicitly selected replay UUID"}},
        "contract":{"create":{"action":"create_contract","data":{"definition":"API signature or JSON schema","sources":["src/api.rs"],"participants":["live session UUID or unique role"],"consumers":{"session UUID":["src/client.rs"]}}},"accept":{"action":"accept_contract","id":"contract UUID","data":{"definition_sha256":"hash from check_contract","subscribe":true}},"amend":{"action":"amend_contract","id":"contract UUID","data":{"definition":"updated interface"}}},
        "behavior":{"plan":{"action":"plan_flow","data":{"platform":"iphone","target":"dedicated-simulator","sources":["src/view.swift"],"steps":[{"tool":"phone_act","arguments":{"action":{"kind":"tap","target":{"identifier":"save"}},"condition":{"identifier":"saved"}},"expect":{"/action/verified":true}},{"tool":"phone_observe","arguments":{"includeImage":true}}]}},"mac_steps":"screenshot, status, click{x,y}, move{x,y}, drag{x,y,toX,toY}, scroll{x,y,dy}, type{text}, key{key}; named existing Mac VM only","run":{"action":"run_flow","id":"flow UUID","data":{"phase":"before or after","build":"installed build identifier"}},"compare":{"action":"compare_behavior","data":{"before":"before execution UUID","after":"after execution UUID"}},"export":{"action":"export_behavior","id":"behavior comparison UUID","path":"comparison.html"},"limits":"32 steps, stable identifiers instead of saved refs; explicit reset/navigation belongs in the flow. Installed build identity is user-declared. No console or network telemetry is fabricated."},
        "context":{"current":{"action":"current_context"},"enable":{"action":"configure_context","data":{"enabled":true}},"disable_and_clear":{"action":"configure_context","data":{"enabled":false,"clear_corrections":true}},"correct":{"action":"correct_context","title":"User factual correction","data":{"text":"Exact user-provided correction","sources":["src/config.rs"]}},"inspect":{"action":"inspect_context","id":"context UUID"},"privacy":"Opt-in, 100 most recent canonical request records per project, excluding image and reasoning payloads. Redaction is best effort. Corrections take effect next turn; source drift suppresses them."},
        "scenario":{"plan":{"action":"plan_scenario","data":{"replay":"replay UUID","cases":[{"name":"UTC","environment":{"TZ":"UTC"},"repetitions":1},{"name":"Fixture network timeout","environment":{"OSMOGREP_TEST_FAULT":"timeout"},"repetitions":2}]}},"run":{"action":"run_scenario","id":"scenario UUID","server":"configured isolated VM MCP server","timeout_seconds":120},"limits":"16 cases, 32 runs, fresh VM per run. Environment variables must be consumed by the reproducer/fixture to inject faults. This is not system-wide network or clock manipulation. Failed/inconclusive cases retain replay IDs; no automatic fixes."}
    });
    kind.and_then(|key| all.get(key)).cloned().unwrap_or(all)
}

/// Human-readable headers keep identifiers and large binary payloads out of the first glance.
pub fn present(value: &Value) -> String {
    fn summarize(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if map.get("type").is_some_and(|v| v == "image") {
                    if let Some(encoded) = map.get("data").and_then(Value::as_str) {
                        let bytes = encoded.len();
                        map.insert("data".into(),json!(format!("PNG retained privately ({bytes} encoded bytes); use export_behavior for paired images")));
                    }
                }
                for child in map.values_mut() {
                    summarize(child);
                }
            }
            Value::Array(items) => {
                for item in items {
                    summarize(item);
                }
            }
            _ => {}
        }
    }
    let record = value.get("record").unwrap_or(value);
    let mut output = String::new();
    if let Some(title) = record["title"].as_str() {
        output.push_str(&format!(
            "{title}\n{} · {}\nID {}\n\n",
            record["kind"].as_str().unwrap_or("Lab"),
            record["created"].as_str().unwrap_or(""),
            record["id"].as_str().unwrap_or("")
        ));
    }
    let mut details = value.clone();
    summarize(&mut details);
    output.push_str(&serde_json::to_string_pretty(&details).unwrap_or_default());
    output
}
