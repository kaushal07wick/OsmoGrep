use std::{collections::BTreeSet, path::Path, time::Instant};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{git, verify::Verdict, Result};
use crate::context::{
    graph::{hash, Graph, Memory, Store},
    runtime::Manifest,
};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Off,
    Screen,
    #[default]
    Balanced,
    Deep,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub mode: Mode,
    pub max_hypotheses: usize,
    pub max_experiments: usize,
    pub experiment_timeout_secs: u64,
    pub max_model_calls: usize,
    pub max_model_tokens: usize,
    pub max_changed_files: usize,
    pub max_graph_nodes: usize,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            mode: Mode::Balanced,
            max_hypotheses: 3,
            max_experiments: 1,
            experiment_timeout_secs: 120,
            max_model_calls: 1,
            max_model_tokens: 6000,
            max_changed_files: 200,
            max_graph_nodes: 200,
        }
    }
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        if self.max_hypotheses > 20
            || self.max_experiments > 3
            || self.experiment_timeout_secs == 0
            || self.experiment_timeout_secs > 3600
            || self.max_changed_files == 0
            || self.max_graph_nodes == 0
            || self.max_graph_nodes > 500
            || self.max_model_calls > 3
            || self.max_model_tokens > 24000
        {
            return Err("policy exceeds supported bounded analysis limits".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub model_calls: usize,
    pub model_tokens: Option<usize>,
    pub sandbox_experiments: usize,
    pub elapsed_ms: u128,
}
impl Default for Usage {
    fn default() -> Self {
        Self {
            model_calls: 0,
            model_tokens: Some(0),
            sandbox_experiments: 0,
            elapsed_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Skipped,
    Review,
    High,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screening {
    pub risk: Risk,
    pub rationale: Vec<String>,
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    pub resource: String,
    pub invariant: String,
    pub changes: Vec<String>,
    pub causal_chain: String,
    pub triggering_conditions: String,
    pub expected_observation: String,
    pub assumptions: Vec<String>,
    pub oracle_sources: Vec<String>,
    pub components: Vec<String>,
    pub verification_plan: String,
    pub verdict: Verdict,
    pub experiment_value: String,
    pub history: Vec<Memory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub run_id: String,
    pub repository: String,
    pub base: String,
    pub candidate: String,
    pub related_revisions: Vec<String>,
    #[serde(default)]
    pub pull_requests: Vec<u64>,
    pub common_base: Option<String>,
    pub merge_order: Vec<String>,
    pub created_at: String,
    pub fingerprint: String,
    pub policy: Policy,
    pub screening: Screening,
    pub impact: Graph,
    pub hypotheses: Vec<Hypothesis>,
    pub limitations: Vec<String>,
    pub usage: Usage,
}

pub fn screen(root: &Path, base: &str, candidate: &str, policy: &Policy) -> Result<Screening> {
    if policy.mode == Mode::Off {
        return Ok(Screening {
            risk: Risk::Disabled,
            rationale: vec!["predictive analysis disabled".into()],
            changed_paths: vec![],
        });
    }
    let paths = git::changed(root, base, candidate)?;
    if paths.len() > policy.max_changed_files {
        return Err("changed-file analysis budget exceeded".into());
    }
    let docs_only = !paths.is_empty()
        && paths.iter().all(|p| {
            (p.starts_with("docs/") || p.starts_with("README"))
                && p.ends_with(".md")
                && !p.contains("/.github/")
        });
    if paths.is_empty() || docs_only {
        return Ok(Screening {
            risk: Risk::Skipped,
            rationale: vec![if docs_only {
                "documentation-only change"
            } else {
                "no tree changes"
            }
            .into()],
            changed_paths: paths,
        });
    }
    let mut reasons = BTreeSet::new();
    for path in &paths {
        let lower = path.to_ascii_lowercase();
        if [".toml", ".yaml", ".yml", ".json", ".lock", "dockerfile"]
            .iter()
            .any(|ext| lower.ends_with(ext))
        {
            reasons.insert("configuration, dependency or deployment change".to_owned());
        }
        let before = git::source(root, base, path)?.unwrap_or_default();
        let after = git::source(root, candidate, path)?.unwrap_or_default();
        let diff = similar::TextDiff::from_lines(&before, &after);
        let changed = diff
            .iter_all_changes()
            .filter(|c| c.tag() != similar::ChangeTag::Equal)
            .map(|c| c.to_string())
            .collect::<String>()
            .to_ascii_lowercase();
        for (tokens, reason) in [
            (
                &[
                    "mutex",
                    "semaphore",
                    "concurrency",
                    "thread",
                    "atomic",
                    "lock(",
                ][..],
                "concurrency or shared state",
            ),
            (
                &["pool", "capacity", "max_connections", "worker"][..],
                "runtime resource capacity",
            ),
            (
                &["retry", "timeout", "backoff"][..],
                "retry or timeout behavior",
            ),
            (
                &["queue", "enqueue", "dequeue"][..],
                "queue production or consumption",
            ),
            (
                &["select ", "insert ", "transaction", "database"][..],
                "database interaction",
            ),
            (
                &["route(", "endpoint", "schema", "serialize"][..],
                "API or data contract",
            ),
        ] {
            if tokens.iter().any(|t| changed.contains(t)) {
                reasons.insert(reason.into());
            }
        }
    }
    let risk = if reasons.is_empty() {
        Risk::Review
    } else {
        Risk::High
    };
    if reasons.is_empty() {
        reasons.insert(
            "executable change; runtime relevance not established by cheap screening".into(),
        );
    }
    Ok(Screening {
        risk,
        rationale: reasons.into_iter().collect(),
        changed_paths: paths,
    })
}

pub fn analyze(
    root: &Path,
    base_ref: &str,
    candidate_ref: &str,
    policy: Policy,
    cancelled: &dyn Fn() -> bool,
) -> Result<Report> {
    policy.validate()?;
    let started = Instant::now();
    let base = git::revision(root, base_ref)?;
    let candidate = git::revision(root, candidate_ref)?;
    let screening = screen(root, &base, &candidate, &policy)?;
    let fingerprint = hash(&serde_json::to_string(
        &json!({"schema":1,"base":base,"candidate":candidate,"policy":policy}),
    )?);
    let mut report = Report {
        schema_version: 1,
        run_id: Uuid::new_v4().to_string(),
        repository: root.display().to_string(),
        base,
        candidate,
        related_revisions: vec![],
        pull_requests: vec![],
        common_base: None,
        merge_order: vec![],
        created_at: chrono::Utc::now().to_rfc3339(),
        fingerprint,
        policy: policy.clone(),
        screening,
        impact: Graph::default(),
        hypotheses: vec![],
        limitations: vec![],
        usage: Usage::default(),
    };
    // Clear skip cases touch neither model nor sandbox nor source index.
    if matches!(report.screening.risk, Risk::Skipped | Risk::Disabled)
        || policy.mode == Mode::Screen
    {
        report.usage.elapsed_ms = started.elapsed().as_millis();
        Store::open(root)?.save_report(
            &report.run_id,
            &report.candidate,
            &report.fingerprint,
            &serde_json::to_value(&report)?,
        )?;
        return Ok(report);
    }
    if cancelled() {
        return Err("analysis cancelled".into());
    }
    let mut store = Store::open(root)?;
    report.fingerprint = hash(&format!(
        "{}:{}",
        report.fingerprint,
        store.knowledge_version()?
    ));
    if let Some(value) = store.cached(&report.fingerprint)? {
        return Ok(serde_json::from_value(value)?);
    }
    store.index(root, &report.candidate, cancelled)?;
    report.impact = store.neighborhood(
        &report.candidate,
        &report.screening.changed_paths,
        2,
        policy.max_graph_nodes,
    )?;
    let before = manifest(root, &report.base)?;
    let after = manifest(root, &report.candidate)?;
    if let (Some(before), Some(after)) = (before, after) {
        report.hypotheses =
            hypotheses(&before, &after, &report.related_revisions, &store, &policy)?;
    } else {
        report.limitations.push("No explicit runtime manifest/invariant at both revisions; heuristic risk is advisory and no causal capacity hypothesis was invented.".into());
    }
    report.limitations.push("Declarations describe intended configuration. No production deployment or runtime behavior has been observed by this analysis.".into());
    report.usage.elapsed_ms = started.elapsed().as_millis();
    store.save_report(
        &report.run_id,
        &report.candidate,
        &report.fingerprint,
        &serde_json::to_value(&report)?,
    )?;
    Ok(report)
}

pub fn manifest(root: &Path, revision: &str) -> Result<Option<Manifest>> {
    git::source(root, revision, "osmogrep.runtime.toml")?
        .map(|s| Manifest::parse(&s))
        .transpose()
}

fn hypotheses(
    before: &Manifest,
    after: &Manifest,
    changes: &[String],
    store: &Store,
    policy: &Policy,
) -> Result<Vec<Hypothesis>> {
    let mut result = Vec::new();
    for invariant in &after.invariants {
        if result.len() >= policy.max_hypotheses {
            break;
        }
        let resource = after
            .resources
            .iter()
            .find(|r| r.id == invariant.resource)
            .ok_or("unknown invariant resource")?;
        let old_resource = before.resources.iter().find(|r| r.id == resource.id);
        let load = |manifest: &Manifest| -> u64 {
            manifest
                .services
                .iter()
                .filter(|s| invariant.services.contains(&s.id))
                .map(|s| u64::from(s.concurrency))
                .sum()
        };
        let demand = load(after);
        let old_demand = load(before);
        let old_capacity = old_resource.map(|r| u64::from(r.capacity)).unwrap_or(0);
        if old_resource.is_none()
            || old_demand > old_capacity
            || demand <= u64::from(resource.capacity)
        {
            continue;
        }
        let fingerprint = hash(&serde_json::to_string(
            &json!({"invariant":invariant,"runtime":after}),
        )?);
        let history = store.history(&resource.id, 5)?;
        result.push(Hypothesis {
            id:fingerprint,resource:resource.id.clone(),invariant:invariant.id.clone(),changes:changes.to_vec(),
            causal_chain:format!("{} capacity changes {old_capacity} → {}; declared concurrent demand changes {old_demand} → {demand}. Under the invariant's assumptions, {demand} simultaneous acquisitions exceed {} available slots.",resource.id,resource.capacity,resource.capacity),
            triggering_conditions:invariant.assumption.clone(),
            expected_observation:format!("At least {} simultaneous acquisitions cannot complete before the workload deadline if all holders retain their slots.",demand-u64::from(resource.capacity)),
            assumptions:vec![invariant.assumption.clone(),"Declarations match the tested configuration; each listed worker holds one slot during the specified overlap.".into()],
            oracle_sources:vec![invariant.source.clone()],components:invariant.services.clone(),
            verification_plan:format!("Generate a separate bounded contention test for `{}` using the contract `{}`. Run identical bytes and setup on the known-good base and candidate; preserve original tests and classify infrastructure errors separately.",resource.id,invariant.oracle),
            verdict:Verdict::Predicted,
            experiment_value:"eligible: explicit oracle, numeric capacity violation, bounded contention experiment; runtime validation still required".into(),
            history,
        });
    }
    Ok(result)
}

pub fn analyze_pair(
    root: &Path,
    left_ref: &str,
    right_ref: &str,
    policy: Policy,
    cancelled: &dyn Fn() -> bool,
) -> Result<Report> {
    policy.validate()?;
    let left = git::revision(root, left_ref)?;
    let right = git::revision(root, right_ref)?;
    if left == right {
        return Err("pair analysis requires two distinct revisions".into());
    }
    let base = git::common_base(root, &left, &right)?;
    let store = Store::open(root)?;
    let pair_fingerprint = hash(&serde_json::to_string(
        &json!({"pair":[left,right],"policy":policy,"schema":1,"knowledge":store.knowledge_version()?}),
    )?);
    if let Some(value) = store.cached(&pair_fingerprint)? {
        return Ok(serde_json::from_value(value)?);
    }
    let left_manifest = manifest(root, &left)?;
    let right_manifest = manifest(root, &right)?;
    let base_manifest = manifest(root, &base)?;
    let left_paths = git::changed(root, &base, &left)?;
    let right_paths = git::changed(root, &base, &right)?;
    let affected = |manifest: Option<&Manifest>, paths: &[String]| -> BTreeSet<String> {
        let mut affected = BTreeSet::new();
        if let (Some(base), Some(current)) = (base_manifest.as_ref(), manifest) {
            for resource in &current.resources {
                if base
                    .resources
                    .iter()
                    .find(|r| r.id == resource.id)
                    .map(|r| r.capacity)
                    != Some(resource.capacity)
                {
                    affected.insert(resource.id.clone());
                }
            }
            for service in &current.services {
                let old = base.services.iter().find(|s| s.id == service.id);
                if old.map(|s| (s.concurrency, s.retry_attempts, &s.resources))
                    != Some((
                        service.concurrency,
                        service.retry_attempts,
                        &service.resources,
                    ))
                    || service.files.iter().any(|f| paths.contains(f))
                {
                    affected.extend(service.resources.clone());
                }
            }
        }
        affected
    };
    let shared: Vec<_> = affected(left_manifest.as_ref(), &left_paths)
        .intersection(&affected(right_manifest.as_ref(), &right_paths))
        .cloned()
        .collect();
    if shared.is_empty() || matches!(policy.mode, Mode::Off | Mode::Screen) {
        let mut report = analyze(
            root,
            &base,
            &left,
            Policy {
                mode: Mode::Screen,
                ..policy
            },
            cancelled,
        )?;
        report.related_revisions = vec![left, right];
        report.limitations.push("No shared changed resource with explicit metadata; pair experiment deferred without constructing combinations.".into());
        report.run_id = Uuid::new_v4().to_string();
        report.fingerprint = pair_fingerprint;
        store.save_report(
            &report.run_id,
            &report.candidate,
            &report.fingerprint,
            &serde_json::to_value(&report)?,
        )?;
        return Ok(report);
    }
    let tree = git::combined_tree(root, &left, &right)?;
    let candidate = git::text(
        root,
        &[
            "-c",
            "user.name=OsmoGrep",
            "-c",
            "user.email=analysis@localhost",
            "commit-tree",
            &tree,
            "-p",
            &left,
            "-p",
            &right,
            "-m",
            "OsmoGrep isolated pair analysis (no branch updated)",
        ],
    )?;
    let mut report = analyze(root, &base, &candidate, policy, cancelled)?;
    report.related_revisions = vec![left.clone(), right.clone()];
    report.common_base = Some(base);
    report.merge_order = vec![left, right];
    for hypothesis in &mut report.hypotheses {
        hypothesis.changes = report.related_revisions.clone();
    }
    // Persist the completed pair metadata as a new immutable report.
    report.run_id = Uuid::new_v4().to_string();
    report.fingerprint = pair_fingerprint;
    Store::open(root)?.save_report(
        &report.run_id,
        &report.candidate,
        &report.fingerprint,
        &serde_json::to_value(&report)?,
    )?;
    Ok(report)
}
