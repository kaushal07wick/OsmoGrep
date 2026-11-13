use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::{Args, Subcommand};
use serde_json::{json, Value};

use super::{
    git,
    predict::{self, Mode, Policy, Report, Risk},
    verify::{self, Experiment, McpSandbox, Verdict},
    Result,
};
use crate::context::{
    graph::{EvidenceClass, Memory, Store},
    runtime::Observation,
};

#[derive(Args, Debug)]
pub struct IndexArgs {
    #[arg(long, default_value = ".")]
    pub repo_root: PathBuf,
    #[arg(long, default_value = "HEAD")]
    pub commit: String,
    /// Explicitly compact unreferenced revision snapshots; reports and memory pin revisions.
    #[arg(long)]
    pub retain_revisions: Option<usize>,
}

#[derive(Args, Debug)]
pub struct ContextArgs {
    #[arg(long, default_value = ".", global = true)]
    pub repo_root: PathBuf,
    #[command(subcommand)]
    pub command: ContextCommand,
}
#[derive(Subcommand, Debug)]
pub enum ContextCommand {
    Impact {
        #[arg(long, default_value = "HEAD")]
        commit: String,
        #[arg(long)]
        symbol: Vec<String>,
    },
    History {
        resource: String,
    },
    Status,
    Observe {
        file: PathBuf,
    },
}

#[derive(Args, Debug)]
pub struct PredictArgs {
    #[arg(long, default_value = ".", global = true)]
    pub repo_root: PathBuf,
    #[command(subcommand)]
    pub command: Option<PredictCommand>,
    #[arg(long, default_value = "HEAD^")]
    pub base: String,
    #[arg(long, default_value = "HEAD")]
    pub commit: String,
    #[arg(long)]
    pub pr: Option<u64>,
    #[arg(long,value_delimiter=',',conflicts_with="pr",num_args=1..=2)]
    pub prs: Vec<u64>,
    #[arg(long, requires = "right", conflicts_with_all=["pr","prs"])]
    pub left: Option<String>,
    #[arg(long, requires = "left")]
    pub right: Option<String>,
    #[arg(long, value_enum)]
    pub mode: Option<Mode>,
    /// JSON policy; analysis remains off unless this command or an integration invokes it.
    #[arg(long)]
    pub policy: Option<PathBuf>,
    #[arg(long)]
    pub verify: bool,
    /// Explicit experiment JSON. No repository command is run on the host.
    #[arg(long, requires = "verify")]
    pub experiment: Option<PathBuf>,
    /// Ask the existing configured provider for one bounded test based on the recorded oracle.
    #[arg(long, requires = "verify", conflicts_with = "experiment")]
    pub generate_experiment: bool,
    #[arg(long, requires = "verify")]
    pub sandbox: Option<String>,
}
#[derive(Subcommand, Debug)]
pub enum PredictCommand {
    Report { run_id: String },
}

#[derive(Args, Debug)]
pub struct McpArgs {
    #[arg(long, default_value = ".")]
    pub repo_root: PathBuf,
    #[arg(long)]
    pub policy: Option<PathBuf>,
    /// Enable explicit verification calls through this configured VM bridge.
    #[arg(long)]
    pub sandbox: Option<String>,
}
pub fn mcp(args: McpArgs) -> Result<()> {
    let root = git::root(&args.repo_root)?;
    let policy: Policy = match args.policy {
        Some(path) => serde_json::from_str(&read_bounded(&path)?)?,
        None => Policy::default(),
    };
    super::mcp::serve(&root, &policy, args.sandbox.as_deref())
}

pub fn index(args: IndexArgs) -> Result<()> {
    let root = git::root(&args.repo_root)?;
    let mut store = Store::open(&root)?;
    let mut value = serde_json::to_value(store.index(&root, &args.commit, &|| false)?)?;
    if let Some(keep) = args.retain_revisions {
        value["pruned_revisions"] = json!(store.compact(keep)?);
    }
    print(&value)
}

pub fn context(args: ContextArgs) -> Result<()> {
    let root = git::root(&args.repo_root)?;
    let value = match args.command {
        ContextCommand::Status => Store::read(&root)?.status()?,
        ContextCommand::History { resource } => {
            serde_json::to_value(Store::read(&root)?.history(&resource, 10)?)?
        }
        ContextCommand::Observe { file } => {
            let observation: Observation = serde_json::from_str(&read_bounded(&file)?)?;
            Store::open(&root)?.observe(&observation)?;
            json!({"status":"stored","evidence_class":"runtime_observation","observation":observation})
        }
        ContextCommand::Impact { commit, symbol } => {
            let revision = git::revision(&root, &commit)?;
            let seeds = if symbol.is_empty() {
                git::changed(
                    &root,
                    &git::revision(&root, &format!("{commit}^"))?,
                    &revision,
                )?
            } else {
                symbol
            };
            json!({"revision":revision,"schema_version":1,"graph":Store::read(&root)?.neighborhood(&revision,&seeds,3,200)?})
        }
    };
    print(&value)
}

fn pr_ref(root: &Path, number: u64) -> Result<String> {
    for reference in [
        format!("refs/pull/{number}/head"),
        format!("refs/remotes/origin/pr/{number}"),
    ] {
        if let Ok(revision) = git::revision(root, &reference) {
            return Ok(revision);
        }
    }
    Err(format!("PR {number} is not fetched. Fetch its head into refs/pull/{number}/head using your existing Git authorization, then retry.").into())
}

pub fn predict(args: PredictArgs) -> Result<()> {
    let root = git::root(&args.repo_root)?;
    if let Some(PredictCommand::Report { run_id }) = args.command {
        return print(&Store::read(&root)?.report(&run_id)?);
    }
    let mut policy: Policy = match args.policy {
        Some(path) => serde_json::from_str(&read_bounded(&path)?)?,
        None => Policy::default(),
    };
    if let Some(mode) = args.mode {
        policy.mode = mode;
    }
    let pull_requests = if let Some(number) = args.pr {
        vec![number]
    } else {
        args.prs.clone()
    };
    let mut report = if let (Some(left), Some(right)) = (args.left, args.right) {
        predict::analyze_pair(&root, &left, &right, policy, &|| false)?
    } else if args.prs.len() == 2 {
        predict::analyze_pair(
            &root,
            &pr_ref(&root, args.prs[0])?,
            &pr_ref(&root, args.prs[1])?,
            policy,
            &|| false,
        )?
    } else {
        let candidate = match args.pr.or(args.prs.first().copied()) {
            Some(number) => pr_ref(&root, number)?,
            None => args.commit,
        };
        predict::analyze(&root, &args.base, &candidate, policy, &|| false)?
    };
    if !pull_requests.is_empty() {
        report.pull_requests = pull_requests;
        report.run_id = uuid::Uuid::new_v4().to_string();
        report.fingerprint = crate::context::graph::hash(&format!(
            "{}:{}",
            report.fingerprint,
            serde_json::to_string(&report.pull_requests)?
        ));
        Store::open(&root)?.save_report(
            &report.run_id,
            &report.candidate,
            &report.fingerprint,
            &serde_json::to_value(&report)?,
        )?;
    }
    let mut value = serde_json::to_value(&report)?;
    if args.verify && !matches!(report.screening.risk, Risk::Skipped | Risk::Disabled) {
        let server = args
            .sandbox
            .ok_or("--verify requires --sandbox <configured MCP bridge>")?;
        ensure_candidate(&root, &report)?;
        let experiment: Experiment = if args.generate_experiment {
            super::reason::generate_experiment(&root, &mut report)?
        } else {
            let path = args
                .experiment
                .ok_or("--verify requires --experiment <JSON> or --generate-experiment")?;
            serde_json::from_str(&read_bounded(&path)?)?
        };
        value = verify_report(&root, &mut report, experiment, &server, &|| false)?;
    }
    print(&value)
}

pub fn verify_report(
    root: &Path,
    report: &mut Report,
    experiment: Experiment,
    server: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<Value> {
    report.policy.validate()?;
    experiment.validate()?;
    if matches!(report.policy.mode, Mode::Off | Mode::Screen) || report.policy.max_experiments == 0
    {
        return Err("policy does not permit experiments".into());
    }
    let hypothesis = report
        .hypotheses
        .iter()
        .find(|h| h.verdict == Verdict::Predicted)
        .ok_or("no eligible hypothesis with an explicit oracle")?
        .clone();
    if experiment.oracle_sources != hypothesis.oracle_sources {
        return Err(
            "experiment oracle sources must match the independently recorded hypothesis".into(),
        );
    }
    ensure_candidate(root, report)?;
    let mut store = Store::open(root)?;
    let job = store.reserve_experiment(
        &report.run_id,
        report.policy.max_experiments,
        report.policy.experiment_timeout_secs + 60,
    )?;
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(report.policy.experiment_timeout_secs);
    let outcome = verify::run(
        &mut McpSandbox { server, deadline },
        root,
        &report.base,
        &report.candidate,
        experiment,
        cancelled,
    );
    let value = match outcome {
        Ok(evidence) => serde_json::to_value(evidence)?,
        Err(error) => json!({"verdict":"execution_failure","error":error.to_string()}),
    };
    let artifact = store.artifact(&redact(&serde_json::to_string(&value)?))?;
    store.finish_experiment(
        job,
        &artifact,
        value["verdict"].as_str().unwrap_or("execution_failure"),
    )?;
    let verdict: Verdict = serde_json::from_value(value["verdict"].clone())?;
    store.remember(&Memory {
        id: uuid::Uuid::new_v4().to_string(),
        revision: report.candidate.clone(),
        resources: vec![hypothesis.resource],
        symbols: hypothesis.components,
        status: if verdict == Verdict::Reproduced {
            EvidenceClass::Verified
        } else {
            EvidenceClass::Declared
        },
        outcome: verdict,
        fingerprint: hypothesis.id,
        environment: format!("sandbox:{server}"),
        trigger: hypothesis.triggering_conditions,
        root_cause: None,
        fix_commit: None,
        invariant: hypothesis.invariant,
        evidence_artifacts: vec![artifact.clone()],
    })?;
    report.usage.sandbox_experiments += 1;
    Ok(
        json!({"report":report,"verification":serde_json::from_str::<Value>(&redact(&serde_json::to_string(&value)?))?,"evidence_artifact":artifact}),
    )
}

pub(crate) fn ensure_candidate(root: &Path, report: &Report) -> Result<()> {
    report.policy.validate()?;
    if matches!(report.policy.mode, Mode::Off | Mode::Screen) || report.policy.max_experiments == 0
    {
        return Err("policy disables experiments".into());
    }
    if git::revision(root, "HEAD")? != report.candidate
        || !git::run(root, &["status", "--porcelain", "--untracked-files=normal"])?.is_empty()
    {
        return Err("sandbox start snapshots the working tree: check out the report candidate in an authorized clean checkout first; evidence for a different tree is stale".into());
    }
    Ok(())
}

// Keep generated source and logs local; redact credentials before persistent or interface output.
pub fn redact(value: &str) -> String {
    fn scrub(value: &mut Value) -> bool {
        let mut changed = false;
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    if crate::is_secret_key(key)
                        && !value.is_null()
                        && !(key.ends_with("_tokens") && value.is_number())
                    {
                        *value = json!("[redacted]");
                        changed = true;
                    } else {
                        changed |= scrub(value);
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    changed |= scrub(value);
                }
            }
            Value::String(text) => {
                let safe = redact_text(text);
                changed = safe != *text;
                *text = safe;
            }
            _ => {}
        }
        changed
    }
    if let Ok(mut data) = serde_json::from_str::<Value>(value) {
        if scrub(&mut data) {
            return data.to_string();
        }
        return value.to_owned();
    }
    redact_text(value)
}

fn redact_text(value: &str) -> String {
    let tokens = regex::Regex::new(
        r"(?:sk-[A-Za-z0-9_-]{12,}|ghp_[A-Za-z0-9]{12,}|github_pat_[A-Za-z0-9_]{12,})",
    )
    .unwrap();
    let fields = regex::Regex::new(
        r#"(?i)((?:password|api[_-]?key|authorization|secret|access[_-]?token|refresh[_-]?token)[\\\"\s:=]+)[^\s,\"\\]+"#,
    )
    .unwrap();
    let bearer = regex::Regex::new(r#"(?i)\bBearer[ \t]+[^\s\"',;\\]+"#).unwrap();
    fields
        .replace_all(
            &bearer.replace_all(
                &tokens.replace_all(value, "[redacted]"),
                "Bearer [redacted]",
            ),
            "${1}[redacted]",
        )
        .into_owned()
}

pub fn read_bounded(path: &Path) -> Result<String> {
    if fs::metadata(path)?.len() > 1024 * 1024 {
        return Err("input exceeds 1 MiB".into());
    }
    Ok(fs::read_to_string(path)?)
}
pub fn print(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
