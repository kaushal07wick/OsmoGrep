//! Optional revision-aware intelligence. The legacy working-tree index is independent.
use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::Path,
    time::Duration,
};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::intelligence::{git, Result};

const PARSER_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Repository,
    Commit,
    Branch,
    PullRequest,
    File,
    Symbol,
    Function,
    Type,
    Module,
    Package,
    Service,
    Api,
    Database,
    Queue,
    Cache,
    Worker,
    Lock,
    Resource,
    Configuration,
    Deployment,
    Invariant,
    Incident,
    Hypothesis,
    Experiment,
    Finding,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Relation {
    Defines,
    Calls,
    Imports,
    DependsOn,
    Reads,
    Writes,
    UsesResource,
    SharesResource,
    ConfiguredBy,
    DeployedAs,
    ModifiedBy,
    Affects,
    ObservedIn,
    Violates,
    ReproducedBy,
    FixedBy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    Declared,
    StaticAnalysis,
    RuntimeObservation,
    ModelInference,
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub relation: Relation,
    pub evidence: EvidenceClass,
    pub provenance: String,
    pub schema_version: u32,
    pub created_at: String,
    pub source_blob: Option<String>,
    pub revision: Option<String>,
    pub freshness: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub unsupported: Vec<String>,
}

impl Graph {
    pub(crate) fn node(&mut self, id: String, kind: NodeKind, label: &str, data: Value) -> String {
        if !self.nodes.iter().any(|n| n.id == id) {
            self.nodes.push(Node {
                id: id.clone(),
                kind,
                label: label.into(),
                data,
            });
        }
        id
    }
    pub(crate) fn edge(
        &mut self,
        source: &str,
        target: &str,
        relation: Relation,
        provenance: &str,
    ) {
        let id = hash(&format!("{source}:{relation:?}:{target}"));
        if !self.edges.iter().any(|e| e.id == id) {
            self.edges.push(Edge {
                id,
                source: source.into(),
                target: target.into(),
                relation,
                evidence: EvidenceClass::StaticAnalysis,
                provenance: provenance.into(),
                schema_version: 1,
                created_at: Utc::now().to_rfc3339(),
                source_blob: None,
                revision: None,
                freshness: "unpersisted".into(),
            });
        }
    }
}

pub fn hash(raw: &str) -> String {
    blake3::hash(raw.as_bytes()).to_hex().to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexStats {
    pub revision: String,
    pub parsed_files: usize,
    pub reused_files: usize,
    pub unsupported_files: usize,
}

pub struct Store {
    pub(crate) db: Connection,
}

impl Store {
    pub fn read(root: &Path) -> Result<Self> {
        reject_storage_symlinks(root)?;
        let db = Connection::open_with_flags(
            root.join(".context/intelligence/graph.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        db.busy_timeout(Duration::from_secs(5))?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != 1 {
            return Err("unsupported intelligence schema".into());
        }
        Ok(Self { db })
    }
    pub fn open(root: &Path) -> Result<Self> {
        reject_storage_symlinks(root)?;
        let dir = root.join(".context/intelligence");
        fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        }
        let mut db = Connection::open(dir.join("graph.sqlite3"))?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 1 {
            return Err("intelligence schema is newer than this binary".into());
        }
        if version == 0 {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("graph_schema.sql"))?;
            tx.commit()?;
        }
        Ok(Self { db })
    }

    pub fn index(
        &mut self,
        root: &Path,
        reference: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<IndexStats> {
        let revision = git::revision(root, reference)?;
        let entries = git::files(root, &revision)?;
        let tx = self.db.transaction()?;
        let mut stats = IndexStats {
            revision: revision.clone(),
            parsed_files: 0,
            reused_files: 0,
            unsupported_files: 0,
        };
        tx.execute(
            "INSERT OR IGNORE INTO revisions(id,created_at) VALUES(?1,?2)",
            params![revision, Utc::now().to_rfc3339()],
        )?;
        for entry in entries.iter().filter(|e| indexable(&e.path)) {
            if cancelled() {
                return Err("indexing cancelled; transaction rolled back".into());
            }
            let key = hash(&format!("{PARSER_VERSION}:{}:{}", entry.path, entry.blob));
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM analyses WHERE id=?1)",
                [&key],
                |r| r.get(0),
            )?;
            if exists {
                stats.reused_files += 1;
            } else {
                let mut graph = match git::blob(root, &entry.blob)? {
                    Some(source) => extract(&entry.path, &source)?,
                    None => Graph {
                        unsupported: vec!["binary or file larger than 1 MiB".into()],
                        ..Graph::default()
                    },
                };
                for edge in &mut graph.edges {
                    edge.source_blob = Some(entry.blob.clone());
                    edge.freshness = "committed_blob".into();
                }
                tx.execute("INSERT INTO analyses(id,path,blob,parser_version,created_at,unsupported) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![key, entry.path, entry.blob, PARSER_VERSION, Utc::now().to_rfc3339(), serde_json::to_string(&graph.unsupported)?])?;
                for node in &graph.nodes {
                    tx.execute(
                        "INSERT INTO nodes(analysis,id,label,kind,payload) VALUES(?1,?2,?3,?4,?5)",
                        params![
                            key,
                            node.id,
                            node.label,
                            serde_json::to_string(&node.kind)?,
                            serde_json::to_string(node)?
                        ],
                    )?;
                }
                for edge in &graph.edges {
                    tx.execute("INSERT INTO edges(analysis,id,source,target,payload) VALUES(?1,?2,?3,?4,?5)",
                        params![key, edge.id, edge.source, edge.target, serde_json::to_string(edge)?])?;
                }
                stats.parsed_files += 1;
            }
            let unsupported: String = tx.query_row(
                "SELECT unsupported FROM analyses WHERE id=?1",
                [&key],
                |r| r.get(0),
            )?;
            if unsupported != "[]" {
                stats.unsupported_files += 1;
            }
            tx.execute(
                "INSERT OR IGNORE INTO revision_files(revision,path,analysis) VALUES(?1,?2,?3)",
                params![revision, entry.path, key],
            )?;
        }
        if cancelled() {
            return Err("indexing cancelled; transaction rolled back".into());
        }
        tx.commit()?;
        Ok(stats)
    }

    pub fn neighborhood(
        &self,
        revision: &str,
        seeds: &[String],
        depth: usize,
        limit: usize,
    ) -> Result<Graph> {
        self.require_revision(revision)?;
        let limit = limit.clamp(1, 500);
        let mut graph = Graph::default();
        let mut queue: VecDeque<(String, usize)> =
            seeds.iter().take(100).map(|s| (s.clone(), 0)).collect();
        let mut visited = BTreeSet::new();
        while let Some((seed, level)) = queue.pop_front() {
            if !visited.insert(seed.clone()) || graph.nodes.len() >= limit {
                continue;
            }
            let mut stmt = self.db.prepare("SELECT DISTINCT n.payload FROM nodes n JOIN revision_files f ON f.analysis=n.analysis WHERE f.revision=?1 AND (n.id=?2 OR n.label=?2 OR f.path=?2) ORDER BY n.id LIMIT ?3")?;
            let rows = stmt.query_map(params![revision, seed, limit as i64], |r| {
                r.get::<_, String>(0)
            })?;
            for row in rows {
                let node: Node = serde_json::from_str(&row?)?;
                if !graph.nodes.iter().any(|n| n.id == node.id) && graph.nodes.len() < limit {
                    if level < depth.min(4) {
                        queue.push_back((node.id.clone(), level + 1));
                    }
                    graph.nodes.push(node);
                }
            }
            let mut stmt = self.db.prepare("SELECT DISTINCT e.payload FROM edges e JOIN revision_files f ON f.analysis=e.analysis WHERE f.revision=?1 AND (e.source=?2 OR e.target=?2) ORDER BY e.id LIMIT ?3")?;
            let rows = stmt.query_map(params![revision, seed, limit as i64], |r| {
                r.get::<_, String>(0)
            })?;
            for row in rows {
                let mut edge: Edge = serde_json::from_str(&row?)?;
                edge.revision = Some(revision.into());
                if level < depth.min(4) {
                    queue.push_back((edge.source.clone(), level + 1));
                    queue.push_back((edge.target.clone(), level + 1));
                }
                if !graph.edges.iter().any(|e| e.id == edge.id) && graph.edges.len() < limit * 2 {
                    graph.edges.push(edge);
                }
            }
        }
        graph.unsupported.push("Rust/Python top-level definitions and unresolved call names only; other languages and exact dispatch resolution are unsupported.".into());
        Ok(graph)
    }

    pub fn require_revision(&self, revision: &str) -> Result<()> {
        let exists: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM revisions WHERE id=?1)",
            [revision],
            |r| r.get(0),
        )?;
        if !exists {
            return Err("revision is not indexed; run osmogrep index --commit <revision>".into());
        }
        Ok(())
    }

    pub fn save_report(
        &self,
        id: &str,
        revision: &str,
        fingerprint: &str,
        value: &Value,
    ) -> Result<()> {
        self.db.execute("INSERT INTO reports(id,revision,fingerprint,created_at,payload) VALUES(?1,?2,?3,?4,?5)",
            params![id,revision,fingerprint,Utc::now().to_rfc3339(),serde_json::to_string(value)?])?;
        Ok(())
    }

    pub fn report(&self, id: &str) -> Result<Value> {
        let value: String =
            self.db
                .query_row("SELECT payload FROM reports WHERE id=?1", [id], |r| {
                    r.get(0)
                })?;
        let mut value: Value = serde_json::from_str(&value)?;
        let mut stmt=self.db.prepare("SELECT j.id,j.status,j.evidence,a.content FROM experiment_jobs j LEFT JOIN artifacts a ON a.id=j.evidence WHERE j.report=?1 ORDER BY j.id")?;
        let jobs = stmt.query_map([id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut evidence = Vec::new();
        for job in jobs {
            let (job, status, artifact, content) = job?;
            evidence.push(json!({"job_id":job,"status":status,"artifact":artifact,"result":content.map(|s|serde_json::from_str::<Value>(&s)).transpose()?}));
        }
        value["verification_jobs"] = json!(evidence);
        let mut stmt = self
            .db
            .prepare("SELECT status,tokens,evidence FROM model_jobs WHERE report=?1 ORDER BY id")?;
        let jobs = stmt.query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut models = Vec::new();
        for job in jobs {
            let (status, tokens, artifact) = job?;
            models.push(json!({"status":status,"tokens":tokens,"artifact":artifact}));
        }
        value["model_jobs"] = json!(models);
        Ok(value)
    }

    pub fn remember(&self, record: &Memory) -> Result<()> {
        if record.status == EvidenceClass::Verified && record.evidence_artifacts.is_empty() {
            return Err("verified memory requires immutable execution artifacts".into());
        }
        if record.resources.is_empty() || record.environment.is_empty() || record.trigger.is_empty()
        {
            return Err("memory requires resources, environment and triggering conditions".into());
        }
        for artifact in &record.evidence_artifacts {
            let exists: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM artifacts WHERE id=?1)",
                [artifact],
                |r| r.get(0),
            )?;
            if !exists {
                return Err("memory references missing execution artifact".into());
            }
        }
        let tx = self.db.unchecked_transaction()?;
        tx.execute("INSERT INTO memories(id,revision,environment,fingerprint,status,payload) VALUES(?1,?2,?3,?4,?5,?6)",
            params![record.id,record.revision,record.environment,record.fingerprint,serde_json::to_string(&record.status)?,serde_json::to_string(record)?])?;
        for resource in &record.resources {
            tx.execute(
                "INSERT INTO memory_resources(memory,resource) VALUES(?1,?2)",
                params![record.id, resource],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn history(&self, resource: &str, limit: usize) -> Result<Vec<Memory>> {
        let mut stmt = self.db.prepare("SELECT m.payload FROM memories m JOIN memory_resources r ON r.memory=m.id WHERE r.resource=?1 ORDER BY m.rowid DESC LIMIT ?2")?;
        let rows = stmt.query_map(params![resource, limit.min(20) as i64], |r| {
            r.get::<_, String>(0)
        })?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }

    pub fn cached(&self, fingerprint: &str) -> Result<Option<Value>> {
        let raw: Option<String> = self
            .db
            .query_row(
                "SELECT payload FROM reports WHERE fingerprint=?1 ORDER BY rowid DESC LIMIT 1",
                [fingerprint],
                |r| r.get(0),
            )
            .optional()?;
        raw.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
    }

    pub fn knowledge_version(&self) -> Result<String> {
        let memories: i64 = self
            .db
            .query_row("SELECT count(*) FROM memories", [], |r| r.get(0))?;
        let observations: i64 =
            self.db
                .query_row("SELECT count(*) FROM observations", [], |r| r.get(0))?;
        Ok(format!("{memories}:{observations}"))
    }

    pub fn artifact(&self, content: &str) -> Result<String> {
        if content.len() > 4 * 1024 * 1024 {
            return Err("artifact exceeds 4 MiB".into());
        }
        let id = hash(content);
        self.db.execute(
            "INSERT OR IGNORE INTO artifacts(id,content) VALUES(?1,?2)",
            params![id, content],
        )?;
        Ok(id)
    }

    pub fn compact(&mut self, keep_revisions: usize) -> Result<usize> {
        if keep_revisions == 0 {
            return Err("retention must keep at least one revision".into());
        }
        let keep_revisions = i64::try_from(keep_revisions)?;
        let tx = self.db.transaction()?;
        let removed = tx.execute("DELETE FROM revisions WHERE id IN (SELECT id FROM revisions ORDER BY rowid DESC LIMIT -1 OFFSET ?1) AND id NOT IN (SELECT revision FROM reports UNION SELECT revision FROM memories)", [keep_revisions])?;
        tx.execute(
            "DELETE FROM analyses WHERE id NOT IN (SELECT analysis FROM revision_files)",
            [],
        )?;
        tx.commit()?;
        self.db
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?;
        Ok(removed)
    }

    pub fn status(&self) -> Result<Value> {
        let count = |table: &str| -> Result<i64> {
            Ok(self
                .db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?)
        };
        Ok(
            json!({"schema_version":1,"revisions":count("revisions")?,"analyses":count("analyses")?,"reports":count("reports")?,"memories":count("memories")?}),
        )
    }

    pub fn observe(&self, observation: &super::runtime::Observation) -> Result<()> {
        observation.validate()?;
        self.require_revision(&observation.deployment_revision)?;
        let raw = serde_json::to_string(observation)?;
        self.db.execute("INSERT OR IGNORE INTO observations(id,revision,service,resource,observed_at,payload) VALUES(?1,?2,?3,?4,?5,?6)",
            params![hash(&raw),observation.deployment_revision,observation.service,observation.resource,observation.observed_at,raw])?;
        Ok(())
    }

    pub fn observations(&self, revision: &str, resource: &str) -> Result<Vec<Value>> {
        let mut stmt = self.db.prepare("SELECT payload FROM observations WHERE revision=?1 AND resource=?2 ORDER BY observed_at DESC LIMIT 20")?;
        let rows = stmt.query_map(params![revision, resource], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }

    /// Reservation is durable before remote work; a crashed worker consumes its budget.
    pub fn reserve_experiment(
        &mut self,
        report: &str,
        max: usize,
        timeout_secs: u64,
    ) -> Result<i64> {
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM experiment_jobs j JOIN reports r ON r.id=j.report WHERE r.revision=(SELECT revision FROM reports WHERE id=?1)",
            [report],
            |r| r.get(0),
        )?;
        if count >= max as i64 {
            return Err("experiment budget exhausted".into());
        }
        let running: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM experiment_jobs WHERE status='running' AND expires_at>unixepoch())",[],|r|r.get(0))?;
        if running {
            return Err("repository experiment concurrency limit reached".into());
        }
        tx.execute("UPDATE experiment_jobs SET status='interrupted' WHERE status='running' AND expires_at<=unixepoch()",[])?;
        tx.execute("INSERT INTO experiment_jobs(report,status,expires_at) VALUES(?1,'running',unixepoch()+?2)",params![report,timeout_secs as i64])?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    pub fn finish_experiment(&self, id: i64, evidence: &str, status: &str) -> Result<()> {
        self.db.execute(
            "UPDATE experiment_jobs SET status=?1,evidence=?2 WHERE id=?3 AND status='running'",
            params![status, evidence, id],
        )?;
        Ok(())
    }

    pub fn reserve_model(&mut self, report: &str, max: usize) -> Result<i64> {
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM model_jobs j JOIN reports r ON r.id=j.report WHERE r.revision=(SELECT revision FROM reports WHERE id=?1)",
            [report],
            |r| r.get(0),
        )?;
        if count >= max as i64 {
            return Err("model call budget exhausted".into());
        }
        tx.execute(
            "INSERT INTO model_jobs(report,status) VALUES(?1,'started')",
            [report],
        )?;
        let job = tx.last_insert_rowid();
        tx.commit()?;
        Ok(job)
    }
    pub fn finish_model(&self, job: i64, tokens: Option<usize>, artifact: &str) -> Result<()> {
        self.db.execute(
            "UPDATE model_jobs SET status='finished',tokens=?1,evidence=?2 WHERE id=?3",
            params![tokens.map(|n| n as i64), artifact, job],
        )?;
        Ok(())
    }
}

fn reject_storage_symlinks(root: &Path) -> Result<()> {
    for relative in [
        ".context",
        ".context/intelligence",
        ".context/intelligence/graph.sqlite3",
        ".context/intelligence/graph.sqlite3-wal",
        ".context/intelligence/graph.sqlite3-shm",
    ] {
        match fs::symlink_metadata(root.join(relative)) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("intelligence storage must not traverse symlinks".into())
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub id: String,
    pub revision: String,
    pub resources: Vec<String>,
    pub symbols: Vec<String>,
    pub status: EvidenceClass,
    pub outcome: crate::intelligence::verify::Verdict,
    pub fingerprint: String,
    pub environment: String,
    pub trigger: String,
    pub root_cause: Option<String>,
    pub fix_commit: Option<String>,
    pub invariant: String,
    pub evidence_artifacts: Vec<String>,
}

fn indexable(path: &str) -> bool {
    !path.split('/').any(|p| {
        matches!(
            p,
            ".git" | ".context" | ".env" | "node_modules" | "target" | "vendor"
        )
    }) && (path.ends_with(".rs") || path.ends_with(".py") || path == "osmogrep.runtime.toml")
}

fn extract(path: &str, source: &str) -> Result<Graph> {
    let mut graph = Graph::default();
    let file = graph.node(
        format!("file:{path}"),
        NodeKind::File,
        path,
        json!({"path":path}),
    );
    if path == "osmogrep.runtime.toml" {
        let manifest = super::runtime::Manifest::parse(source)?;
        manifest.add_graph(&mut graph, &file)?;
        return Ok(graph);
    }
    let symbols = super::indexer::extract_symbols(source, path);
    for symbol in symbols {
        let id = graph.node(
            format!("symbol:{path}:{}:{}", symbol.kind, symbol.name),
            if symbol.kind == "class" {
                NodeKind::Type
            } else {
                NodeKind::Function
            },
            &symbol.name,
            json!({"file":path,"line_start":symbol.line_start,"line_end":symbol.line_end}),
        );
        graph.edge(&file, &id, Relation::Defines, path);
        for call in &symbol.calls {
            // shortcut: unqualified call names are candidates; use language name resolution before treating them as exact targets.
            let target = graph.node(
                format!("call:{call}"),
                NodeKind::Symbol,
                call,
                json!({"resolution":"unresolved_name"}),
            );
            graph.edge(&id, &target, Relation::Calls, path);
        }
    }
    Ok(graph)
}
