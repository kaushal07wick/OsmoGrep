use std::{fs, path::PathBuf};

use super::{
    git,
    predict::{self, Policy, Risk},
    verify::Verdict,
};
use crate::context::{
    graph::{EvidenceClass, Memory, Store},
    runtime::Manifest,
};

struct Repo(PathBuf);
impl Repo {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("osmogrep-intelligence-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        git::run(&root, &["init", "-q"]).unwrap();
        git::run(&root, &["config", "user.name", "Test"]).unwrap();
        git::run(&root, &["config", "user.email", "test@localhost"]).unwrap();
        fs::write(root.join(".gitignore"), ".context/\n").unwrap();
        Self(root)
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    fn commit(&self) -> String {
        git::run(&self.0, &["add", "."]).unwrap();
        git::run(&self.0, &["commit", "-qm", "fixture"]).unwrap();
        git::revision(&self.0, "HEAD").unwrap()
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manifest(capacity: u32, concurrency: u32) -> String {
    format!(
        r#"schema_version = 1
[[resources]]
id = "db"
kind = "connection_pool"
capacity = {capacity}
[[services]]
id = "worker"
resources = ["db"]
files = ["worker.py"]
concurrency = {concurrency}
[[invariants]]
id = "all-workers-acquire"
kind = "capacity_covers_concurrency"
resource = "db"
services = ["worker"]
assumption = "All workers hold one connection until every worker reaches the barrier."
oracle = "Every worker acquires within the workload deadline."
source = "CONTRACT.md:1"
"#
    )
}

#[test]
fn docs_skip_without_indexing_or_using_execution() {
    let repo = Repo::new();
    repo.write("README.md", "before");
    let base = repo.commit();
    repo.write("README.md", "after");
    let candidate = repo.commit();
    let report =
        predict::analyze(&repo.0, &base, &candidate, Policy::default(), &|| false).unwrap();
    assert_eq!(report.screening.risk, Risk::Skipped);
    assert_eq!(report.usage.model_calls, 0);
    assert_eq!(report.usage.sandbox_experiments, 0);
    let store = Store::read(&repo.0).unwrap();
    assert_eq!(store.status().unwrap()["analyses"], 0);
    assert_eq!(
        store.report(&report.run_id).unwrap()["screening"]["risk"],
        "skipped"
    );
}

#[test]
fn graph_survives_restart_reuses_unchanged_analysis_and_isolates_branches() {
    let repo = Repo::new();
    repo.write("a.rs", "fn first() { second(); }\n");
    repo.write("b.rs", "fn second() {}\n");
    let base = repo.commit();
    let mut store = Store::open(&repo.0).unwrap();
    assert_eq!(
        store.index(&repo.0, &base, &|| false).unwrap().parsed_files,
        2
    );
    drop(store);
    repo.write("a.rs", "fn replaced() { second(); }\n");
    let candidate = repo.commit();
    let mut store = Store::open(&repo.0).unwrap();
    let stats = store.index(&repo.0, &candidate, &|| false).unwrap();
    assert_eq!(stats.parsed_files, 1);
    assert_eq!(stats.reused_files, 1);
    assert_eq!(
        store
            .index(&repo.0, &candidate, &|| false)
            .unwrap()
            .parsed_files,
        0
    );
    let old = store.neighborhood(&base, &["first".into()], 2, 20).unwrap();
    assert!(old.nodes.iter().any(|n| n.label == "first"));
    let new = store
        .neighborhood(&candidate, &["first".into()], 2, 20)
        .unwrap();
    assert!(new.nodes.is_empty());
    assert!(!store
        .neighborhood(&base, &["replaced".into()], 2, 20)
        .unwrap()
        .nodes
        .iter()
        .any(|n| n.label == "replaced"));
    assert!(store
        .neighborhood(&candidate, &["replaced".into()], 2, 20)
        .unwrap()
        .edges
        .iter()
        .any(|e| e.evidence == EvidenceClass::StaticAnalysis));
}

#[test]
fn interrupted_index_rolls_back_and_recovers() {
    let repo = Repo::new();
    repo.write("a.rs", "fn first() {}\n");
    let base = repo.commit();
    let mut store = Store::open(&repo.0).unwrap();
    assert!(store.index(&repo.0, &base, &|| true).is_err());
    assert_eq!(store.status().unwrap()["revisions"], 0);
    assert_eq!(
        store.index(&repo.0, &base, &|| false).unwrap().parsed_files,
        1
    );
}

#[test]
fn high_risk_configuration_produces_explicit_hypothesis_and_history() {
    let repo = Repo::new();
    repo.write("osmogrep.runtime.toml", &manifest(16, 8));
    let base = repo.commit();
    let mut store = Store::open(&repo.0).unwrap();
    store.index(&repo.0, &base, &|| false).unwrap();
    let artifact = store
        .artifact("independently checked earlier experiment")
        .unwrap();
    store
        .remember(&Memory {
            id: "old".into(),
            revision: base.clone(),
            resources: vec!["db".into()],
            symbols: vec!["worker".into()],
            status: EvidenceClass::Verified,
            outcome: Verdict::Reproduced,
            fingerprint: "earlier".into(),
            environment: "isolated-test".into(),
            trigger: "contention".into(),
            root_cause: None,
            fix_commit: None,
            invariant: "all-workers-acquire".into(),
            evidence_artifacts: vec![artifact],
        })
        .unwrap();
    repo.write("osmogrep.runtime.toml", &manifest(4, 8));
    let candidate = repo.commit();
    let report =
        predict::analyze(&repo.0, &base, &candidate, Policy::default(), &|| false).unwrap();
    assert_eq!(report.screening.risk, Risk::High);
    assert_eq!(report.hypotheses.len(), 1);
    let hypothesis = &report.hypotheses[0];
    assert!(hypothesis
        .causal_chain
        .contains("8 simultaneous acquisitions"));
    assert_eq!(hypothesis.verdict, Verdict::Predicted);
    assert_eq!(hypothesis.history.len(), 1);
    assert!(!report.impact.nodes.is_empty());
    assert_eq!(
        report.run_id,
        predict::analyze(&repo.0, &base, &candidate, Policy::default(), &|| false)
            .unwrap()
            .run_id
    );
    let reopened = Store::read(&repo.0).unwrap();
    assert_eq!(reopened.history("db", 10).unwrap().len(), 1);
    assert_eq!(
        reopened.report(&report.run_id).unwrap()["candidate"],
        candidate
    );
}

#[test]
fn related_pairs_merge_without_changing_worktree_and_irrelevant_pairs_defer() {
    let repo = Repo::new();
    repo.write("osmogrep.runtime.toml", &manifest(16, 8));
    let base = repo.commit();
    repo.write("osmogrep.runtime.toml", &manifest(8, 8));
    let left = repo.commit();
    git::run(&repo.0, &["checkout", "--detach", &base]).unwrap();
    repo.write("osmogrep.runtime.toml", &manifest(16, 16));
    let right = repo.commit();
    let current = git::revision(&repo.0, "HEAD").unwrap();
    for revision in [&left, &right] {
        assert!(
            predict::analyze(&repo.0, &base, revision, Policy::default(), &|| false)
                .unwrap()
                .hypotheses
                .is_empty()
        );
    }
    let report =
        predict::analyze_pair(&repo.0, &left, &right, Policy::default(), &|| false).unwrap();
    assert_eq!(report.hypotheses.len(), 1);
    assert_eq!(report.related_revisions.len(), 2);
    assert_eq!(report.common_base, Some(base.clone()));
    assert_eq!(git::revision(&repo.0, "HEAD").unwrap(), current);
    git::run(&repo.0, &["checkout", "--detach", &base]).unwrap();
    repo.write("README.md", "irrelevant docs");
    let docs = repo.commit();
    let unrelated =
        predict::analyze_pair(&repo.0, &left, &docs, Policy::default(), &|| false).unwrap();
    assert!(unrelated.hypotheses.is_empty());
    assert_eq!(unrelated.usage.sandbox_experiments, 0);
}

#[test]
fn budgets_and_worker_failure_do_not_corrupt_graph() {
    let repo = Repo::new();
    repo.write("osmogrep.runtime.toml", &manifest(16, 8));
    let base = repo.commit();
    repo.write("osmogrep.runtime.toml", &manifest(4, 8));
    let candidate = repo.commit();
    let policy = Policy {
        max_hypotheses: 0,
        ..Policy::default()
    };
    assert!(
        predict::analyze(&repo.0, &base, &candidate, policy, &|| false)
            .unwrap()
            .hypotheses
            .is_empty()
    );
    let report =
        predict::analyze(&repo.0, &base, &candidate, Policy::default(), &|| false).unwrap();
    let mut store = Store::open(&repo.0).unwrap();
    let job = store.reserve_experiment(&report.run_id, 1, 60).unwrap();
    assert!(store.reserve_experiment(&report.run_id, 1, 60).is_err());
    let artifact = store.artifact("worker crashed").unwrap();
    store
        .finish_experiment(job, &artifact, "execution_failure")
        .unwrap();
    assert!(store.reserve_experiment(&report.run_id, 1, 60).is_err());
    assert!(store.require_revision(&candidate).is_ok());
}

#[test]
fn runtime_metadata_validation_rejects_unknown_resources_and_secrets_fields() {
    assert!(Manifest::parse(&manifest(16, 8)).is_ok());
    assert!(Manifest::parse(
        &manifest(16, 8).replace("resources = [\"db\"]", "resources = [\"missing\"]")
    )
    .is_err());
    assert!(Manifest::parse(&(manifest(16, 8) + "\npassword = \"secret\"\n")).is_err());
    assert_eq!(super::cli::redact("sk-1234567890123456"), "[redacted]");
    let raw=serde_json::json!({"access_token":"opaque-value","headers":{"Authorization":"Bearer hidden"},"model_tokens":42,"source":"assert True\n"}).to_string();
    let safe: serde_json::Value = serde_json::from_str(&super::cli::redact(&raw)).unwrap();
    assert_eq!(safe["access_token"], "[redacted]");
    assert_eq!(safe["model_tokens"], 42);
    assert_eq!(safe["source"], "assert True\n");
    assert!(!super::cli::redact("Authorization: Bearer opaque-value").contains("opaque-value"));
    assert!(Policy {
        max_experiments: 99,
        ..Policy::default()
    }
    .validate()
    .is_err());
}

#[test]
fn stale_candidate_is_rejected_before_model_or_sandbox_work() {
    let repo = Repo::new();
    repo.write("osmogrep.runtime.toml", &manifest(16, 8));
    let base = repo.commit();
    repo.write("osmogrep.runtime.toml", &manifest(4, 8));
    let candidate = repo.commit();
    let mut report =
        predict::analyze(&repo.0, &base, &candidate, Policy::default(), &|| false).unwrap();
    assert!(super::cli::ensure_candidate(&repo.0, &report).is_ok());
    repo.write("worker.py", "# concurrent edit\n");
    assert!(super::cli::ensure_candidate(&repo.0, &report).is_err());
    report.policy.max_model_calls = 0;
    assert!(super::reason::generate_experiment(&repo.0, &mut report).is_err());
    assert_eq!(report.usage.model_calls, 0);
}

#[test]
fn failed_migration_and_corrupt_database_are_visible_and_leave_legacy_context_alone() {
    let repo = Repo::new();
    repo.write(".context/context.json", "legacy survives");
    let dir = repo.0.join(".context/intelligence");
    fs::create_dir_all(&dir).unwrap();
    let db = rusqlite::Connection::open(dir.join("graph.sqlite3")).unwrap();
    db.execute_batch("CREATE TABLE nodes(block_migration TEXT);")
        .unwrap();
    drop(db);
    assert!(Store::open(&repo.0).is_err());
    let db = rusqlite::Connection::open(dir.join("graph.sqlite3")).unwrap();
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 0);
    let partial: i64 = db
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='revisions'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(partial, 0);
    drop(db);
    fs::write(dir.join("graph.sqlite3"), b"not a database").unwrap();
    assert!(Store::open(&repo.0).is_err());
    assert_eq!(
        fs::read_to_string(repo.0.join(".context/context.json")).unwrap(),
        "legacy survives"
    );
}

#[test]
fn graph_failure_does_not_disable_normal_agent_tools() {
    let repo = Repo::new();
    repo.write("a.rs", "fn unchanged() {}\n");
    repo.commit();
    let dir = repo.0.join(".context/intelligence");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("graph.sqlite3"), b"broken").unwrap();
    let registry = crate::tools::ToolRegistry::with_root(repo.0.clone());
    assert!(registry
        .call_parallel_safe_cancellable(
            "query_code_graph",
            serde_json::json!({"revision":"HEAD","seeds":["unchanged"]}),
            &|| false
        )
        .is_err());
    let file = registry
        .call_parallel_safe_cancellable("read_file", serde_json::json!({"path":"a.rs"}), &|| false)
        .unwrap();
    assert!(file.to_string().contains("unchanged"));
    let scope = crate::tools::ToolScope::for_prompt("fix the function");
    assert!(!registry
        .scoped_schema(&scope)
        .iter()
        .any(|s| s["name"] == "query_code_graph"));
    let scope =
        crate::tools::ToolScope::for_prompt("inspect runtime risk and code graph").read_only();
    assert!(registry
        .scoped_schema(&scope)
        .iter()
        .any(|s| s["name"] == "query_code_graph"));
}

#[test]
fn runtime_observations_stay_bound_to_their_deployment_revision() {
    use crate::context::runtime::Observation;
    let repo = Repo::new();
    repo.write("osmogrep.runtime.toml", &manifest(16, 8));
    let base = repo.commit();
    let mut store = Store::open(&repo.0).unwrap();
    store.index(&repo.0, &base, &|| false).unwrap();
    let observation = Observation {
        service: "worker".into(),
        resource: "db".into(),
        deployment_revision: base.clone(),
        observed_at: "2026-11-13T12:00:00Z".into(),
        source: "authorized-staging:otel/deploy-1".into(),
        capacity: Some(8),
        concurrency: Some(8),
    };
    store.observe(&observation).unwrap();
    store.observe(&observation).unwrap();
    assert_eq!(store.observations(&base, "db").unwrap().len(), 1);
    repo.write("osmogrep.runtime.toml", &manifest(32, 8));
    let candidate = repo.commit();
    store.index(&repo.0, &candidate, &|| false).unwrap();
    assert!(store.observations(&candidate, "db").unwrap().is_empty());
    assert_eq!(store.observations(&base, "db").unwrap()[0]["capacity"], 8);
    let graph = store.neighborhood(&base, &["db".into()], 2, 30).unwrap();
    assert!(graph.nodes.iter().any(|n| n.data["capacity"] == 16));
}

#[test]
fn conflicting_prs_fail_without_touching_the_checkout() {
    let repo = Repo::new();
    repo.write("osmogrep.runtime.toml", &manifest(16, 8));
    let base = repo.commit();
    repo.write("osmogrep.runtime.toml", &manifest(8, 8));
    let left = repo.commit();
    git::run(&repo.0, &["checkout", "--detach", &base]).unwrap();
    repo.write("osmogrep.runtime.toml", &manifest(4, 8));
    let right = repo.commit();
    assert!(predict::analyze_pair(&repo.0, &left, &right, Policy::default(), &|| false).is_err());
    assert_eq!(git::revision(&repo.0, "HEAD").unwrap(), right);
    assert!(git::run(&repo.0, &["status", "--porcelain"])
        .unwrap()
        .is_empty());
}

#[test]
fn retention_keeps_pinned_reports_and_unreferenced_analysis_is_compacted() {
    let repo = Repo::new();
    repo.write("a.rs", "fn old() {}\n");
    let base = repo.commit();
    let mut store = Store::open(&repo.0).unwrap();
    store.index(&repo.0, &base, &|| false).unwrap();
    repo.write("a.rs", "fn new() {}\n");
    let candidate = repo.commit();
    store.index(&repo.0, &candidate, &|| false).unwrap();
    assert_eq!(store.compact(1).unwrap(), 1);
    assert!(store.require_revision(&base).is_err());
    assert!(store.require_revision(&candidate).is_ok());
    assert_eq!(store.status().unwrap()["analyses"], 1);
    assert!(store.compact(0).is_err());
}

#[cfg(unix)]
#[test]
fn untrusted_storage_symlinks_are_rejected() {
    let repo = Repo::new();
    let other = Repo::new();
    std::os::unix::fs::symlink(&other.0, repo.0.join(".context")).unwrap();
    assert!(Store::open(&repo.0).is_err());
    assert!(!other.0.join("intelligence").exists());
}

#[test]
fn model_and_experiment_budget_cannot_be_reset_by_a_new_report_id() {
    let repo = Repo::new();
    repo.write("a.rs", "fn first() {}\n");
    let revision = repo.commit();
    let mut store = Store::open(&repo.0).unwrap();
    store.index(&repo.0, &revision, &|| false).unwrap();
    for id in ["first", "second"] {
        store
            .save_report(id, &revision, id, &serde_json::json!({"revision":revision}))
            .unwrap();
    }
    store.reserve_experiment("first", 1, 60).unwrap();
    assert!(store.reserve_experiment("second", 1, 60).is_err());
    store.reserve_model("first", 1).unwrap();
    assert!(store.reserve_model("second", 1).is_err());
}

#[test]
fn custom_merge_drivers_are_rejected_without_executing_repository_commands() {
    let repo = Repo::new();
    repo.write("a.rs", "fn original() {}\n");
    let base = repo.commit();
    git::run(
        &repo.0,
        &["config", "merge.untrusted.driver", "touch SHOULD_NOT_EXIST"],
    )
    .unwrap();
    assert!(git::combined_tree(&repo.0, &base, &base).is_err());
    assert!(!repo.0.join("SHOULD_NOT_EXIST").exists());
}
