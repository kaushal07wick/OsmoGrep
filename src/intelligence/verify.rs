//! Differential orchestration only. Repository code is executed by the configured VM bridge.
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

use super::Result;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Predicted,
    ExperimentReady,
    Reproduced,
    NotReproduced,
    Inconclusive,
    InvalidHypothesis,
    ExecutionFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Experiment {
    pub path: String,
    pub source: String,
    pub command: String,
    /// An independent specification, documented contract, or pre-existing invariant.
    pub oracle: String,
    pub oracle_sources: Vec<String>,
}

impl Experiment {
    pub fn validate(&self) -> Result<()> {
        let path = Path::new(&self.path);
        if path.components().count() != 1
            || self.path.contains(['/', '\\', '\0'])
            || !self.path.starts_with("osmogrep_repro_")
            || self.source.is_empty()
            || self.source.len() > 128 * 1024
            || self.command.trim().is_empty()
            || self.command.len() > 4096
            || self.oracle.trim().is_empty()
            || self.oracle_sources.is_empty()
            || self.oracle_sources.iter().any(|s| s.trim().is_empty())
            || super::cli::redact(&self.source) != self.source
            || super::cli::redact(&self.command) != self.command
        {
            return Err("experiment needs a new root-level osmogrep_repro_* file, bounded source/command, and an independent oracle with sources".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifferentialEvidence {
    pub verdict: Verdict,
    pub session: Option<String>,
    pub experiment: Experiment,
    pub context: Value,
    pub comparison: Value,
    pub original_suites: Value,
    pub export: Value,
    pub cleanup_error: Option<String>,
}

pub trait Sandbox {
    fn call(&mut self, method: &str, args: Value) -> Result<Value>;
}

pub struct McpSandbox<'a> {
    pub server: &'a str,
    pub deadline: Instant,
}

impl Sandbox for McpSandbox<'_> {
    fn call(&mut self, method: &str, args: Value) -> Result<Value> {
        let timeout = if method == "close" {
            Duration::from_secs(30)
        } else {
            self.deadline.saturating_duration_since(Instant::now())
        };
        let result = crate::mcp::call_with_timeout(Some(self.server), method, &args, timeout)?;
        if result["exit_code"].as_i64() != Some(0) {
            return Err(format!("sandbox bridge failed for {method}").into());
        }
        let raw = result["stdout"]
            .as_str()
            .ok_or("bridge returned no stdout")?;
        let value: Value = serde_json::from_str(raw)?;
        if value["isError"].as_bool() == Some(true) || value.get("error").is_some() {
            return Err(format!("sandbox rejected {method}").into());
        }
        Ok(value)
    }
}

/// A failure is evidence only when both clean executions used the same oracle and environment.
/// The bridge must independently attest integrity; agent-provided prose is never an attestation.
pub fn classify(comparison: &Value, check: &Value) -> Verdict {
    if check["integrity_preserved"].as_bool() != Some(true)
        || comparison["environment_equivalent"].as_bool() != Some(true)
        || comparison["oracle_validated"].as_bool() != Some(true)
        || comparison["reproducer_identical"].as_bool() != Some(true)
    {
        return Verdict::Inconclusive;
    }
    let base = &comparison["base"];
    let candidate = &comparison["candidate"];
    if [base, candidate].iter().any(|v| {
        v["timed_out"].as_bool() != Some(false)
            || v["cancelled"].as_bool() != Some(false)
            || v["exit_code"].as_i64().is_none()
    }) {
        return Verdict::ExecutionFailure;
    }
    match (base["exit_code"].as_i64(), candidate["exit_code"].as_i64()) {
        (Some(0), Some(0)) => Verdict::NotReproduced,
        (Some(0), Some(1)) if comparison["candidate"]["failure_kind"] == "assertion" => {
            Verdict::Reproduced
        }
        (Some(0), Some(_)) => Verdict::Inconclusive,
        (Some(_), _) if base["failure_kind"] == "assertion" => Verdict::InvalidHypothesis,
        _ => Verdict::ExecutionFailure,
    }
}

pub fn run(
    sandbox: &mut dyn Sandbox,
    repo: &Path,
    base: &str,
    candidate: &str,
    experiment: Experiment,
    cancelled: &dyn Fn() -> bool,
) -> Result<DifferentialEvidence> {
    experiment.validate()?;
    if cancelled() {
        return Err("cancelled before sandbox allocation".into());
    }
    let started = sandbox.call("start", json!({"repo": repo, "base": base}))?;
    let session = started["session"]
        .as_str()
        .ok_or("sandbox returned no session")?
        .to_owned();
    let mut evidence = DifferentialEvidence {
        verdict: Verdict::ExecutionFailure,
        session: Some(session.clone()),
        experiment,
        context: Value::Null,
        comparison: Value::Null,
        original_suites: Value::Null,
        export: Value::Null,
        cleanup_error: None,
    };
    let outcome = (|| -> Result<()> {
        evidence.context = sandbox.call("context", json!({"session": session}))?;
        if cancelled() {
            return Err("cancelled".into());
        }
        sandbox.call("reproducer", json!({
            "session": session, "path": evidence.experiment.path, "content": evidence.experiment.source
        }))?;
        if cancelled() {
            return Err("cancelled".into());
        }
        evidence.comparison = sandbox.call(
            "compare",
            json!({
                "session": session, "command": evidence.experiment.command
            }),
        )?;
        evidence.original_suites = sandbox.call("check", json!({"session": session}))?;
        evidence.export = sandbox.call("export", json!({"session": session}))?;
        evidence.verdict = classify(&evidence.comparison, &evidence.original_suites);
        if matches!(
            evidence.verdict,
            Verdict::Reproduced | Verdict::NotReproduced | Verdict::InvalidHypothesis
        ) {
            let digest = hex::encode(Sha256::digest(evidence.experiment.source.as_bytes()));
            if evidence.comparison["base"]["revision"] != base
                || evidence.comparison["candidate"]["revision"] != candidate
                || evidence.comparison["reproducer_sha256"] != digest
                || evidence.comparison["command"] != evidence.experiment.command
                || evidence.comparison["oracle_sources"]
                    != json!(evidence.experiment.oracle_sources)
                || evidence.comparison["environment_fingerprint"]
                    .as_str()
                    .map(str::is_empty)
                    .unwrap_or(true)
                || !evidence.comparison["environment_details"].is_object()
                || !evidence.comparison["dependency_versions"].is_object()
                || evidence.comparison["base"]["source_tree_matches_revision"] != true
                || evidence.comparison["candidate"]["source_tree_matches_revision"] != true
            {
                evidence.verdict = Verdict::Inconclusive;
            }
        }
        if evidence.verdict == Verdict::Reproduced
            && evidence.export["artifacts"]
                .as_array()
                .map(|a| {
                    a.is_empty()
                        || a.iter().any(|v| {
                            v["sha256"]
                                .as_str()
                                .map(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
                                .unwrap_or(true)
                                || v["reference"].as_str().map(str::is_empty).unwrap_or(true)
                        })
                })
                .unwrap_or(true)
        {
            evidence.verdict = Verdict::Inconclusive;
        }
        Ok(())
    })();
    // Always attempt cleanup, including after failed comparisons and cancellation.
    if let Err(error) = sandbox.call("close", json!({"session": session})) {
        evidence.cleanup_error = Some(error.to_string());
    }
    if let Err(error) = outcome {
        evidence.export = json!({"execution_error": error.to_string()});
    }
    Ok(evidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mocked_chain_preserves_artifacts_and_rejects_stale_revision_bindings() {
        struct Worker {
            comparison: Value,
            calls: Vec<String>,
        }
        impl Sandbox for Worker {
            fn call(&mut self, method: &str, _: Value) -> Result<Value> {
                self.calls.push(method.into());
                Ok(match method {
                    "start" => json!({"session":"isolated-mock"}),
                    "context" => {
                        json!({"original_suites":{"base":{"exit_code":0},"candidate":{"exit_code":0}}})
                    }
                    "compare" => self.comparison.clone(),
                    "check" => json!({"integrity_preserved":true}),
                    "export" => {
                        json!({"artifacts":[{"sha256":"a".repeat(64),"reference":"private://mock/replay"}]})
                    }
                    _ => json!({}),
                })
            }
        }
        let experiment = Experiment {
            path: "osmogrep_repro_pool.py".into(),
            source: "assert True\n".into(),
            command: "python3 osmogrep_repro_pool.py".into(),
            oracle: "independent contract".into(),
            oracle_sources: vec!["CONTRACT.md:3".into()],
        };
        let comparison = json!({
            "base":{"revision":"base","source_tree_matches_revision":true,"exit_code":0,"timed_out":false,"cancelled":false},
            "candidate":{"revision":"head","source_tree_matches_revision":true,"exit_code":1,"timed_out":false,"cancelled":false,"failure_kind":"assertion"},
            "environment_equivalent":true,"oracle_validated":true,"reproducer_identical":true,
            "reproducer_sha256":hex::encode(Sha256::digest(experiment.source.as_bytes())),
            "command":experiment.command,"oracle_sources":experiment.oracle_sources,
            "environment_fingerprint":"mock-image-v1","environment_details":{"image":"controlled mock"},"dependency_versions":{"python":"mock"}
        });
        let mut worker = Worker {
            comparison,
            calls: vec![],
        };
        let evidence = run(
            &mut worker,
            Path::new("."),
            "base",
            "head",
            experiment.clone(),
            &|| false,
        )
        .unwrap();
        assert_eq!(evidence.verdict, Verdict::Reproduced);
        assert_eq!(
            worker.calls,
            vec![
                "start",
                "context",
                "reproducer",
                "compare",
                "check",
                "export",
                "close"
            ]
        );
        assert_eq!(evidence.context["original_suites"]["base"]["exit_code"], 0);
        worker.comparison["candidate"]["revision"] = json!("different");
        assert_eq!(
            run(
                &mut worker,
                Path::new("."),
                "base",
                "head",
                experiment,
                &|| false
            )
            .unwrap()
            .verdict,
            Verdict::Inconclusive
        );
    }

    #[test]
    fn experiment_paths_are_new_root_files_on_all_platforms() {
        let mut experiment = Experiment {
            path: "osmogrep_repro_x.py".into(),
            source: "assert True".into(),
            command: "python3 osmogrep_repro_x.py".into(),
            oracle: "contract".into(),
            oracle_sources: vec!["spec".into()],
        };
        assert!(experiment.validate().is_ok());
        for path in [
            "../osmogrep_repro_x.py",
            "osmogrep_repro_x/../test.py",
            "osmogrep_repro_x\\..\\test.py",
        ] {
            experiment.path = path.into();
            assert!(experiment.validate().is_err());
        }
    }

    #[test]
    fn differential_verdict_requires_independent_execution_attestations() {
        let mut comparison = json!({
            "base":{"exit_code":0,"timed_out":false,"cancelled":false},
            "candidate":{"exit_code":1,"timed_out":false,"cancelled":false,"failure_kind":"assertion"},
            "environment_equivalent":true,"oracle_validated":true,"reproducer_identical":true
        });
        let check = json!({"integrity_preserved":true});
        assert_eq!(classify(&comparison, &check), Verdict::Reproduced);
        comparison["candidate"]["exit_code"] = json!(0);
        assert_eq!(classify(&comparison, &check), Verdict::NotReproduced);
        comparison["base"]["exit_code"] = json!(1);
        assert_eq!(classify(&comparison, &check), Verdict::ExecutionFailure);
        comparison["base"]["failure_kind"] = json!("assertion");
        assert_eq!(classify(&comparison, &check), Verdict::InvalidHypothesis);
        comparison["oracle_validated"] = Value::Null;
        assert_eq!(classify(&comparison, &check), Verdict::Inconclusive);
    }

    #[test]
    fn failed_worker_still_closes_and_preserves_original_evidence() {
        struct Broken {
            calls: Vec<String>,
        }
        impl Sandbox for Broken {
            fn call(&mut self, method: &str, _: Value) -> Result<Value> {
                self.calls.push(method.into());
                match method {
                    "start" => Ok(json!({"session":"test"})),
                    "context" => Ok(json!({"original_suites":{"base":0,"candidate":0}})),
                    "compare" => Err("worker lost".into()),
                    _ => Ok(json!({})),
                }
            }
        }
        let mut worker = Broken { calls: vec![] };
        let evidence = run(
            &mut worker,
            Path::new("."),
            "abc",
            "def",
            Experiment {
                path: "osmogrep_repro_pool.py".into(),
                source: "assert True".into(),
                command: "python osmogrep_repro_pool.py".into(),
                oracle: "request completes".into(),
                oracle_sources: vec!["contract.md:2".into()],
            },
            &|| false,
        )
        .unwrap();
        assert_eq!(evidence.verdict, Verdict::ExecutionFailure);
        assert_eq!(evidence.context["original_suites"]["base"], 0);
        assert_eq!(worker.calls.last().unwrap(), "close");
    }
}
