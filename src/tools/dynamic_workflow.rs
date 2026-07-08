use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

const DEFAULT_QUERY_COUNT: usize = 4;
const DEFAULT_SOURCES_PER_QUERY: usize = 3;
const MAX_AGENTS: usize = 16;
const MAX_SOURCES_PER_QUERY: usize = 6;
const FETCH_CHAR_LIMIT: usize = 8_000;
const WORKFLOW_DIR: &str = ".context/osmogrep-workflows";

pub struct DynamicWorkflow;

impl Tool for DynamicWorkflow {
    fn name(&self) -> &'static str {
        "dynamic_workflow"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "function",
            "name": "dynamic_workflow",
            "description": "Run a bounded Claude Code-style dynamic workflow for broad research or audit tasks. Use for deep online research, cross-source checks, or workflow-scale exploration instead of manually looping web_search/web_fetch calls.",
            "parameters": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["start", "resume", "checkpoint", "complete"],
                        "description": "Workflow action. Start creates a durable ledger; resume loads one; checkpoint updates progress; complete closes it."
                    },
                    "task": {
                        "type": "string",
                        "description": "The research, audit, or coding task the workflow should coordinate. Required for start."
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["coding", "audit", "deep_research", "research"],
                        "description": "Workflow type. If omitted, Osmogrep infers coding, audit, or deep_research from the task."
                    },
                    "workflow_id": {
                        "type": "string",
                        "description": "Workflow id to resume, checkpoint, or complete. If omitted for resume/checkpoint, the latest workflow is used."
                    },
                    "queries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional query angles. If omitted, the workflow derives several angles from the task."
                    },
                    "max_agents": {
                        "type": "integer",
                        "description": "Maximum query agents to fan out, capped at 16"
                    },
                    "max_sources_per_query": {
                        "type": "integer",
                        "description": "Maximum search results to fetch per query, capped at 6"
                    },
                    "notes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional progress notes to append during checkpoint/resume."
                    },
                    "completed_steps": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Step names or 1-based step numbers to mark completed during checkpoint."
                    },
                    "status": {
                        "type": "string",
                        "description": "Optional workflow status for checkpoint, such as running, blocked, or completed."
                    }
                },
                "required": [],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    fn call(&self, args: Value) -> ToolResult {
        let action = args
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("start");
        match action {
            "start" => start_workflow(args),
            "resume" => resume_workflow(args),
            "checkpoint" => checkpoint_workflow(args, false),
            "complete" => checkpoint_workflow(args, true),
            _ => Err("action must be start, resume, checkpoint, or complete".to_string()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WorkflowRecord {
    id: String,
    kind: String,
    task: String,
    status: String,
    steps: Vec<WorkflowStep>,
    notes: Vec<String>,
    metadata: Value,
    created_at: String,
    updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WorkflowStep {
    name: String,
    status: String,
    instruction: String,
}

fn start_workflow(args: Value) -> ToolResult {
    let task = args
        .get("task")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|task| !task.is_empty())
        .ok_or("missing task")?;
    let kind = workflow_kind(task, args.get("kind").and_then(Value::as_str))?;
    let id = args
        .get("workflow_id")
        .and_then(Value::as_str)
        .map(sanitize_workflow_id)
        .filter(|id| !id.is_empty())
        .unwrap_or_else(new_workflow_id);
    let now = now_rfc3339();
    let mut record = WorkflowRecord {
        id,
        kind: kind.to_string(),
        task: task.to_string(),
        status: "running".to_string(),
        steps: default_workflow_steps(kind),
        notes: parse_notes(args.get("notes")),
        metadata: json!({}),
        created_at: now.clone(),
        updated_at: now,
    };

    if matches!(kind, "deep_research" | "research") {
        run_research_workflow(args, &mut record)
    } else {
        save_workflow(&record)?;
        Ok(json!({
            "workflow": workflow_summary(&record),
            "ledger": record.clone(),
            "next_step": next_step_value(&record),
            "resume_instructions": resume_instructions(&record),
            "model_instructions": coding_workflow_instructions(kind),
        }))
    }
}

fn run_research_workflow(args: Value, record: &mut WorkflowRecord) -> ToolResult {
    let task = record.task.clone();
    let kind = record.kind.clone();
    let max_agents = args
        .get("max_agents")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_QUERY_COUNT)
        .clamp(1, MAX_AGENTS);
    let max_sources_per_query = args
        .get("max_sources_per_query")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_SOURCES_PER_QUERY)
        .clamp(1, MAX_SOURCES_PER_QUERY);
    let queries = workflow_queries(&task, args.get("queries"), max_agents);
    let started = Instant::now();
    let agents = run_research_agents(&queries, max_sources_per_query);
    let sources = flatten_sources(&agents);

    set_step_status(&mut record.steps, "plan_queries", "completed");
    set_step_status(&mut record.steps, "search_sources", "completed");
    set_step_status(&mut record.steps, "fetch_evidence", "completed");
    set_step_status(&mut record.steps, "synthesize", "in_progress");
    record.metadata = json!({
        "queries": queries,
        "agent_count": agents.len(),
        "source_count": sources.len(),
        "duration_ms": started.elapsed().as_millis(),
        "limits": {
            "max_agents": max_agents,
            "max_sources_per_query": max_sources_per_query
        }
    });
    record.updated_at = now_rfc3339();
    save_workflow(record)?;

    Ok(json!({
        "workflow": {
            "id": record.id.clone(),
            "kind": kind,
            "task": task,
            "status": record.status.clone(),
            "ledger_path": workflow_path(&record.id).display().to_string(),
            "agent_count": agents.len(),
            "duration_ms": started.elapsed().as_millis(),
            "limits": {
                "max_agents": max_agents,
                "max_sources_per_query": max_sources_per_query
            },
            "phases": [
                { "name": "plan_queries", "status": "completed", "agent_count": 1 },
                { "name": "search_sources", "status": "completed", "agent_count": agents.len() },
                { "name": "fetch_evidence", "status": "completed", "source_count": sources.len() },
                { "name": "synthesize", "status": "ready_for_model", "source_count": sources.len() }
            ]
        },
        "ledger": record.clone(),
        "next_step": next_step_value(record),
        "agents": agents,
        "sources": sources,
        "report_instructions": "Synthesize the answer from the sources. Cite URLs inline. Label claims unsupported if the fetched excerpts do not verify them.",
        "resume_instructions": resume_instructions(record)
    }))
}

fn resume_workflow(args: Value) -> ToolResult {
    let mut record = load_requested_workflow(&args)?;
    record.status = "running".to_string();
    record.notes.extend(parse_notes(args.get("notes")));
    record.updated_at = now_rfc3339();
    save_workflow(&record)?;

    Ok(json!({
        "workflow": workflow_summary(&record),
        "ledger": record.clone(),
        "next_step": next_step_value(&record),
        "resume_instructions": resume_instructions(&record),
    }))
}

fn checkpoint_workflow(args: Value, complete: bool) -> ToolResult {
    let mut record = load_requested_workflow(&args)?;
    record.notes.extend(parse_notes(args.get("notes")));
    mark_completed_steps(&mut record, args.get("completed_steps"));
    if complete {
        for step in &mut record.steps {
            step.status = "completed".to_string();
        }
        record.status = "completed".to_string();
    } else if let Some(status) = args
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|status| !status.is_empty())
    {
        record.status = status.to_string();
    }
    promote_next_step(&mut record);
    record.updated_at = now_rfc3339();
    save_workflow(&record)?;

    Ok(json!({
        "workflow": workflow_summary(&record),
        "ledger": record.clone(),
        "next_step": next_step_value(&record),
        "resume_instructions": resume_instructions(&record),
    }))
}

fn workflow_kind<'a>(task: &str, explicit: Option<&'a str>) -> Result<&'a str, String> {
    if let Some(kind) = explicit {
        if matches!(kind, "coding" | "audit" | "deep_research" | "research") {
            return Ok(kind);
        }
        return Err("kind must be coding, audit, deep_research, or research".to_string());
    }

    let lower = task.to_ascii_lowercase();
    if contains_any(
        &lower,
        &[
            "latest",
            "online",
            "web",
            "internet",
            "research",
            "sources",
            "docs",
            "documentation",
        ],
    ) {
        Ok("deep_research")
    } else if contains_any(
        &lower,
        &["audit", "review", "edge case", "security", "risk"],
    ) {
        Ok("audit")
    } else {
        Ok("coding")
    }
}

fn default_workflow_steps(kind: &str) -> Vec<WorkflowStep> {
    let specs = match kind {
        "deep_research" | "research" => vec![
            (
                "plan_queries",
                "Derive bounded search angles and evidence targets.",
            ),
            (
                "search_sources",
                "Fan out source search across query angles.",
            ),
            (
                "fetch_evidence",
                "Fetch high-signal excerpts and capture URLs.",
            ),
            (
                "synthesize",
                "Synthesize answer with citations and unsupported-claim labels.",
            ),
        ],
        "audit" => vec![
            (
                "scope",
                "Clarify the audit target and relevant constraints.",
            ),
            (
                "inventory",
                "Map files, entry points, configs, and existing tests.",
            ),
            (
                "inspect_risks",
                "Inspect failure modes, regressions, security risks, and edge cases.",
            ),
            (
                "verify_findings",
                "Run deterministic checks or targeted reproductions.",
            ),
            (
                "propose_fixes",
                "Apply or propose scoped fixes with rollback awareness.",
            ),
            (
                "summarize",
                "Summarize findings, verification, and residual risk.",
            ),
        ],
        _ => vec![
            (
                "orient",
                "Read project instructions, recent changes, and relevant context.",
            ),
            (
                "map_code",
                "Search and read the code paths involved in the task.",
            ),
            ("plan", "Create or update a concise implementation plan."),
            ("implement", "Make scoped edits following local patterns."),
            ("verify", "Run targeted checks and fix regressions."),
            ("summarize", "Report changes, tests, and follow-up risk."),
        ],
    };

    specs
        .into_iter()
        .enumerate()
        .map(|(idx, (name, instruction))| WorkflowStep {
            name: name.to_string(),
            status: if idx == 0 { "in_progress" } else { "pending" }.to_string(),
            instruction: instruction.to_string(),
        })
        .collect()
}

fn workflow_summary(record: &WorkflowRecord) -> Value {
    let completed = record
        .steps
        .iter()
        .filter(|step| step.status == "completed")
        .count();
    json!({
        "id": record.id.clone(),
        "kind": record.kind.clone(),
        "task": record.task.clone(),
        "status": record.status.clone(),
        "ledger_path": workflow_path(&record.id).display().to_string(),
        "step_count": record.steps.len(),
        "completed_steps": completed,
        "agent_count": record.metadata.get("agent_count").and_then(Value::as_u64).unwrap_or(0),
        "source_count": record.metadata.get("source_count").and_then(Value::as_u64).unwrap_or(0),
        "next_step": next_step_value(record),
        "steps": record.steps.iter().map(|step| json!({
            "name": step.name.clone(),
            "status": step.status.clone(),
            "instruction": step.instruction.clone(),
        })).collect::<Vec<_>>(),
    })
}

fn workflow_dir() -> PathBuf {
    PathBuf::from(WORKFLOW_DIR)
}

fn workflow_path(id: &str) -> PathBuf {
    workflow_dir().join(format!("{}.json", sanitize_workflow_id(id)))
}

fn save_workflow(record: &WorkflowRecord) -> Result<(), String> {
    fs::create_dir_all(workflow_dir()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    fs::write(workflow_path(&record.id), text).map_err(|e| e.to_string())
}

fn load_requested_workflow(args: &Value) -> Result<WorkflowRecord, String> {
    if let Some(id) = args
        .get("workflow_id")
        .and_then(Value::as_str)
        .map(sanitize_workflow_id)
        .filter(|id| !id.is_empty())
    {
        load_workflow(&id)
    } else {
        load_latest_workflow()
    }
}

fn load_workflow(id: &str) -> Result<WorkflowRecord, String> {
    let path = workflow_path(id);
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("failed to read workflow {}: {}", path.display(), e))?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn load_latest_workflow() -> Result<WorkflowRecord, String> {
    let mut records = Vec::new();
    let entries = fs::read_dir(workflow_dir()).map_err(|_| {
        "no workflow ledger found; start one with dynamic_workflow action=start".to_string()
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if let Ok(text) = fs::read_to_string(entry.path()) {
            if let Ok(record) = serde_json::from_str::<WorkflowRecord>(&text) {
                records.push(record);
            }
        }
    }
    records
        .into_iter()
        .max_by(|a, b| a.updated_at.cmp(&b.updated_at))
        .ok_or_else(|| {
            "no workflow ledger found; start one with dynamic_workflow action=start".to_string()
        })
}

fn next_step_value(record: &WorkflowRecord) -> Value {
    record
        .steps
        .iter()
        .find(|step| step.status != "completed")
        .map(|step| {
            json!({
                "name": step.name.clone(),
                "status": step.status.clone(),
                "instruction": step.instruction.clone(),
            })
        })
        .unwrap_or(Value::Null)
}

fn parse_notes(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|note| !note.is_empty())
            .map(ToString::to_string)
            .collect(),
        Some(Value::String(note)) if !note.trim().is_empty() => vec![note.trim().to_string()],
        _ => Vec::new(),
    }
}

fn mark_completed_steps(record: &mut WorkflowRecord, value: Option<&Value>) {
    let Some(items) = value.and_then(Value::as_array) else {
        return;
    };
    for item in items {
        let Some(raw) = item.as_str().map(str::trim).filter(|raw| !raw.is_empty()) else {
            continue;
        };
        for (idx, step) in record.steps.iter_mut().enumerate() {
            if step.name == raw || (idx + 1).to_string() == raw {
                step.status = "completed".to_string();
            }
        }
    }
}

fn promote_next_step(record: &mut WorkflowRecord) {
    if record.status == "completed" || record.steps.iter().any(|step| step.status == "in_progress")
    {
        return;
    }
    if let Some(step) = record
        .steps
        .iter_mut()
        .find(|step| step.status != "completed")
    {
        step.status = "in_progress".to_string();
    }
}

fn set_step_status(steps: &mut [WorkflowStep], name: &str, status: &str) {
    if let Some(step) = steps.iter_mut().find(|step| step.name == name) {
        step.status = status.to_string();
    }
}

fn resume_instructions(record: &WorkflowRecord) -> String {
    format!(
        "Workflow {} is persisted at {}. Resume with dynamic_workflow action=resume workflow_id={} and checkpoint progress with completed_steps plus concise notes.",
        record.id,
        workflow_path(&record.id).display(),
        record.id
    )
}

fn coding_workflow_instructions(kind: &str) -> Vec<&'static str> {
    if kind == "audit" {
        vec![
            "Use search/read tools to map the audit surface before editing.",
            "Checkpoint completed audit steps with dynamic_workflow after each phase.",
            "Verify findings with tests, diagnostics, or concrete reproduction where possible.",
        ]
    } else {
        vec![
            "Use read/search tools to orient before editing.",
            "Use update_plan for the live UI checklist, then checkpoint durable workflow progress.",
            "Run targeted verification before summarizing or committing.",
        ]
    }
}

fn sanitize_workflow_id(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(96)
        .collect()
}

fn new_workflow_id() -> String {
    format!("workflow-{}", uuid::Uuid::new_v4())
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn workflow_queries(task: &str, provided: Option<&Value>, max_agents: usize) -> Vec<String> {
    let mut queries = Vec::new();
    if let Some(values) = provided.and_then(Value::as_array) {
        for value in values {
            if let Some(query) = value
                .as_str()
                .map(str::trim)
                .filter(|query| !query.is_empty())
            {
                push_unique(&mut queries, query.to_string());
            }
            if queries.len() >= max_agents {
                break;
            }
        }
    }

    if queries.is_empty() {
        for query in default_query_angles(task) {
            push_unique(&mut queries, query);
            if queries.len() >= max_agents {
                break;
            }
        }
    }

    queries.truncate(max_agents);
    queries
}

fn default_query_angles(task: &str) -> Vec<String> {
    let task = task.trim();
    vec![
        task.to_string(),
        format!("{task} official documentation"),
        format!("{task} changelog release notes"),
        format!("{task} GitHub issue discussion"),
    ]
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn run_research_agents(queries: &[String], max_sources_per_query: usize) -> Vec<Value> {
    thread::scope(|scope| {
        let handles = queries
            .iter()
            .map(|query| {
                let query = query.clone();
                scope.spawn(move || research_agent(&query, max_sources_per_query))
            })
            .collect::<Vec<_>>();

        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| failed_agent("agent panicked"))
            })
            .collect()
    })
}

fn research_agent(query: &str, max_sources_per_query: usize) -> Value {
    let started = Instant::now();
    match search_web(query, max_sources_per_query) {
        Ok(results) => {
            let sources = results.into_iter().map(fetch_source).collect::<Vec<_>>();
            json!({
                "query": query,
                "status": "completed",
                "duration_ms": started.elapsed().as_millis(),
                "source_count": sources.len(),
                "sources": sources,
            })
        }
        Err(error) => json!({
            "query": query,
            "status": "failed",
            "duration_ms": started.elapsed().as_millis(),
            "error": error,
            "sources": [],
        }),
    }
}

fn failed_agent(message: &str) -> Value {
    json!({
        "query": "",
        "status": "failed",
        "duration_ms": 0,
        "error": message,
        "sources": [],
    })
}

fn flatten_sources(agents: &[Value]) -> Vec<Value> {
    let mut sources = Vec::new();
    for agent in agents {
        let query = agent.get("query").and_then(Value::as_str).unwrap_or("");
        if let Some(items) = agent.get("sources").and_then(Value::as_array) {
            for item in items {
                let mut source = item.clone();
                if let Some(map) = source.as_object_mut() {
                    map.insert("query".to_string(), Value::String(query.to_string()));
                }
                sources.push(source);
            }
        }
    }
    sources
}

#[derive(Clone, Debug)]
struct SearchResult {
    title: String,
    url: String,
}

fn search_web(query: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
    let url = format!("https://duckduckgo.com/html/?q={}", encode_query(query));
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let html = client
        .get(url)
        .header("User-Agent", "osmogrep/0.3")
        .send()
        .map_err(|e| e.to_string())?
        .text()
        .map_err(|e| e.to_string())?;

    parse_search_results(&html, limit)
}

fn parse_search_results(html: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
    let re = Regex::new(r#"<a[^>]*class=\"result__a\"[^>]*href=\"([^\"]+)\"[^>]*>(.*?)</a>"#)
        .map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    for cap in re.captures_iter(html).take(limit) {
        let href = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let title = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        if href.is_empty() {
            continue;
        }
        results.push(SearchResult {
            title: html_decode(&strip_tags(title)),
            url: normalize_result_url(href),
        });
    }
    Ok(results)
}

fn fetch_source(result: SearchResult) -> Value {
    match fetch_text(&result.url, FETCH_CHAR_LIMIT) {
        Ok((status, content_type, text)) => json!({
            "title": result.title,
            "url": result.url,
            "status": status,
            "content_type": content_type,
            "excerpt": excerpt_text(&text),
        }),
        Err(error) => json!({
            "title": result.title,
            "url": result.url,
            "status": null,
            "content_type": "",
            "error": error,
            "excerpt": "",
        }),
    }
}

fn fetch_text(url: &str, max_chars: usize) -> Result<(u16, String, String), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body = resp.text().map_err(|e| e.to_string())?;
    let text = if body.chars().count() > max_chars {
        body.chars().take(max_chars).collect::<String>()
    } else {
        body
    };
    Ok((
        status,
        headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string(),
        strip_tags(&text),
    ))
}

fn excerpt_text(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= 1_500 {
        compact
    } else {
        let head: String = compact.chars().take(1_500).collect();
        format!("{head} ...")
    }
}

fn normalize_result_url(href: &str) -> String {
    let href = html_decode(href);
    if let Some(idx) = href.find("uddg=") {
        let encoded = href[idx + 5..].split('&').next().unwrap_or("").to_string();
        if let Some(decoded) = percent_decode(&encoded) {
            return decoded;
        }
    }
    href
}

fn strip_tags(s: &str) -> String {
    let re = Regex::new(r"<[^>]+>").unwrap();
    re.replace_all(s, "").to_string()
}

fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        match bytes[idx] {
            b'%' if idx + 2 < bytes.len() => {
                let hi = hex_value(bytes[idx + 1])?;
                let lo = hex_value(bytes[idx + 2])?;
                out.push((hi << 4) | lo);
                idx += 3;
            }
            b'+' => {
                out.push(b' ');
                idx += 1;
            }
            byte => {
                out.push(byte);
                idx += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn encode_query(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_bounded_unique_workflow_queries() {
        let queries = workflow_queries(
            "node permissions",
            Some(&json!([
                "node permissions",
                "node permissions",
                "node changelog"
            ])),
            2,
        );

        assert_eq!(queries, vec!["node permissions", "node changelog"]);
    }

    #[test]
    fn infers_workflow_kind_from_task_shape() {
        assert_eq!(workflow_kind("fix the parser", None).unwrap(), "coding");
        assert_eq!(
            workflow_kind("audit edge cases in the parser", None).unwrap(),
            "audit"
        );
        assert_eq!(
            workflow_kind("search latest docs for the parser", None).unwrap(),
            "deep_research"
        );
    }

    #[test]
    fn sanitizes_workflow_ids_for_repo_local_paths() {
        assert_eq!(sanitize_workflow_id("../parser/work"), "parser-work");
        assert_eq!(sanitize_workflow_id("parser_work-1"), "parser_work-1");
    }

    #[test]
    fn decodes_duckduckgo_redirect_urls() {
        let url =
            normalize_result_url("/l/?uddg=https%3A%2F%2Fexample.com%2Fa%3Fx%3D1&amp;rut=abc");

        assert_eq!(url, "https://example.com/a?x=1");
    }

    #[test]
    fn parses_search_result_cards() {
        let html = r#"
            <a rel="nofollow" class="result__a" href="/l/?uddg=https%3A%2F%2Fexample.com">Example &amp; Docs</a>
        "#;
        let results = parse_search_results(html, 5).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example & Docs");
        assert_eq!(results[0].url, "https://example.com");
    }
}
