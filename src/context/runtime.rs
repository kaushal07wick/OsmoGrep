//! Explicit repository metadata is declared intent, never proof of deployed configuration.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::graph::{EvidenceClass, Graph, NodeKind, Relation};
use crate::intelligence::Result;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    #[serde(default)]
    pub resources: Vec<Resource>,
    #[serde(default)]
    pub services: Vec<Service>,
    #[serde(default)]
    pub invariants: Vec<Invariant>,
    pub recipe: Option<Recipe>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub id: String,
    pub kind: String,
    pub capacity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub id: String,
    pub resources: Vec<String>,
    pub files: Vec<String>,
    pub concurrency: u32,
    #[serde(default = "one")]
    pub retry_attempts: u32,
}
fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invariant {
    pub id: String,
    pub kind: InvariantKind,
    pub resource: String,
    pub services: Vec<String>,
    pub assumption: String,
    pub oracle: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvariantKind {
    CapacityCoversConcurrency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    #[serde(default)]
    pub install: Vec<String>,
    #[serde(default)]
    pub build: Vec<String>,
    #[serde(default)]
    pub services: Vec<LocalService>,
    #[serde(default)]
    pub environment_names: Vec<String>,
    pub test: String,
    #[serde(default)]
    pub cleanup: Vec<String>,
    pub timeout_secs: u64,
    pub cpu_limit: u32,
    pub memory_mb: u32,
    #[serde(default)]
    pub network_allowlist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalService {
    pub name: String,
    pub start: String,
    pub ready: String,
    pub stop: String,
}

impl Manifest {
    pub fn parse(source: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(source)?;
        if manifest.schema_version != 1 {
            return Err("unsupported runtime manifest schema".into());
        }
        let mut ids = BTreeSet::new();
        for (id, capacity) in manifest.resources.iter().map(|r| (&r.id, r.capacity)) {
            if !valid_id(id) || !ids.insert(id) || capacity == 0 {
                return Err("resources need unique IDs and positive capacities".into());
            }
        }
        let mut services = BTreeSet::new();
        for service in &manifest.services {
            if !valid_id(&service.id)
                || !services.insert(&service.id)
                || service.concurrency == 0
                || service.retry_attempts == 0
                || service.resources.iter().any(|r| !ids.contains(r))
                || service.files.iter().any(|f| !safe_path(f))
            {
                return Err("invalid service or unknown resource reference".into());
            }
        }
        let mut invariants = BTreeSet::new();
        for invariant in &manifest.invariants {
            if !valid_id(&invariant.id)
                || !invariants.insert(&invariant.id)
                || !ids.contains(&invariant.resource)
                || invariant.services.is_empty()
                || invariant.services.iter().any(|s| !services.contains(s))
                || invariant.assumption.trim().is_empty()
                || invariant.oracle.trim().is_empty()
                || invariant.source.trim().is_empty()
            {
                return Err(
                    "invariant requires known components, assumptions, oracle and provenance"
                        .into(),
                );
            }
        }
        if let Some(recipe) = &manifest.recipe {
            if recipe.timeout_secs == 0
                || recipe.timeout_secs > 3600
                || recipe.cpu_limit == 0
                || recipe.memory_mb < 64
                || recipe.test.is_empty()
                || recipe
                    .environment_names
                    .iter()
                    .any(|v| !v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
                || recipe
                    .services
                    .iter()
                    .any(|s| s.start.is_empty() || s.ready.is_empty() || s.stop.is_empty())
            {
                return Err(
                    "recipe requires bounded resources, test, and service lifecycle commands"
                        .into(),
                );
            }
        }
        Ok(manifest)
    }

    pub fn add_graph(&self, graph: &mut Graph, file: &str) -> Result<()> {
        for resource in &self.resources {
            let id = graph.node(format!("resource:{}",resource.id), NodeKind::Resource, &resource.id,
                json!({"capacity":resource.capacity,"kind":resource.kind,"freshness":"declared_configuration"}));
            graph.edge(file, &id, Relation::Defines, "osmogrep.runtime.toml");
        }
        for service in &self.services {
            let id = graph.node(
                format!("service:{}", service.id),
                NodeKind::Service,
                &service.id,
                json!({"concurrency":service.concurrency,"retry_attempts":service.retry_attempts}),
            );
            graph.edge(&id, file, Relation::ConfiguredBy, "osmogrep.runtime.toml");
            for resource in &service.resources {
                graph.edge(
                    &id,
                    &format!("resource:{resource}"),
                    Relation::UsesResource,
                    "osmogrep.runtime.toml",
                );
            }
            for path in &service.files {
                let source = graph.node(
                    format!("file:{path}"),
                    NodeKind::File,
                    path,
                    json!({"path":path}),
                );
                graph.edge(&source, &id, Relation::DeployedAs, "osmogrep.runtime.toml");
            }
        }
        for invariant in &self.invariants {
            let id = graph.node(
                format!("invariant:{}", invariant.id),
                NodeKind::Invariant,
                &invariant.id,
                serde_json::to_value(invariant)?,
            );
            graph.edge(
                &id,
                &format!("resource:{}", invariant.resource),
                Relation::Affects,
                &invariant.source,
            );
        }
        for edge in &mut graph.edges {
            edge.evidence = EvidenceClass::Declared;
        }
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !std::path::Path::new(path).is_absolute()
        && !path
            .split(['/', '\\'])
            .any(|p| matches!(p, ".." | ".git" | ".env"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub service: String,
    pub resource: String,
    pub deployment_revision: String,
    pub observed_at: String,
    pub source: String,
    pub capacity: Option<u32>,
    pub concurrency: Option<u32>,
}

impl Observation {
    pub fn validate(&self) -> Result<()> {
        if !valid_id(&self.service)
            || !valid_id(&self.resource)
            || self.source.trim().is_empty()
            || !matches!(self.deployment_revision.len(), 40 | 64)
            || !self
                .deployment_revision
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
            || self
                .observed_at
                .parse::<chrono::DateTime<chrono::Utc>>()
                .is_err()
        {
            return Err(
                "observation requires exact deployment revision, timestamp and provenance".into(),
            );
        }
        Ok(())
    }
}
