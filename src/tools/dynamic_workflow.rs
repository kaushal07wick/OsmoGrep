use std::thread;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{json, Value};

use super::{Tool, ToolResult, ToolSafety};

const DEFAULT_QUERY_COUNT: usize = 4;
const DEFAULT_SOURCES_PER_QUERY: usize = 3;
const MAX_AGENTS: usize = 16;
const MAX_SOURCES_PER_QUERY: usize = 6;
const FETCH_CHAR_LIMIT: usize = 8_000;

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
                    "task": {
                        "type": "string",
                        "description": "The research or audit task the workflow should investigate"
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["deep_research", "research"],
                        "description": "Workflow type. Use deep_research for online research requiring multiple sources."
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
                    }
                },
                "required": ["task"],
                "additionalProperties": false
            }
        })
    }

    fn safety(&self) -> ToolSafety {
        ToolSafety::Safe
    }

    fn call(&self, args: Value) -> ToolResult {
        let task = args
            .get("task")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|task| !task.is_empty())
            .ok_or("missing task")?;
        let kind = args
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("deep_research");
        if !matches!(kind, "deep_research" | "research") {
            return Err("kind must be deep_research or research".to_string());
        }

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
        let queries = workflow_queries(task, args.get("queries"), max_agents);
        let started = Instant::now();
        let agents = run_research_agents(&queries, max_sources_per_query);
        let sources = flatten_sources(&agents);

        Ok(json!({
            "workflow": {
                "id": format!("workflow-{}", uuid::Uuid::new_v4()),
                "kind": kind,
                "task": task,
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
            "agents": agents,
            "sources": sources,
            "report_instructions": "Synthesize the answer from the sources. Cite URLs inline. Label claims unsupported if the fetched excerpts do not verify them."
        }))
    }
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
