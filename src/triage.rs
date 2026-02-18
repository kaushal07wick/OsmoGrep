use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::Args;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};

#[derive(Args, Debug, Clone)]
pub struct TriageArgs {
    #[arg(long, help = "GitHub repository in owner/name form")]
    pub repo: String,

    #[arg(long, default_value = "open", help = "Item state: open | closed | all")]
    pub state: String,

    #[arg(long, default_value_t = 250, help = "Max PRs and issues each to analyze")]
    pub limit: usize,

    #[arg(long, default_value_t = 15, help = "Top PRs to deep-review")]
    pub deep_review_top: usize,

    #[arg(long, default_value_t = 0.62, help = "Duplicate similarity threshold (0.0-1.0)")]
    pub dedupe_threshold: f64,

    #[arg(long, help = "Path to a vision document for scope alignment")]
    pub vision: Option<PathBuf>,

    #[arg(long, help = "GitHub token (or set GITHUB_TOKEN)")]
    pub token: Option<String>,

    #[arg(long, help = "Write full report JSON to this file")]
    pub out: Option<PathBuf>,

    #[arg(long, default_value_t = false, help = "Only print JSON report")]
    pub json_only: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum ItemKind {
    PullRequest,
    Issue,
}

#[derive(Debug, Clone)]
struct WorkItem {
    kind: ItemKind,
    number: u64,
    title: String,
    body: String,
    url: String,
    author: String,
    created_at: String,
    token_set: HashSet<String>,
    title_token_set: HashSet<String>,
}

#[derive(Debug, Serialize)]
struct DuplicatePair {
    left_kind: ItemKind,
    left_number: u64,
    left_title: String,
    left_url: String,
    right_kind: ItemKind,
    right_number: u64,
    right_title: String,
    right_url: String,
    similarity: f64,
    rationale: String,
}

#[derive(Debug, Serialize)]
struct PrScoreReport {
    number: u64,
    title: String,
    url: String,
    author: String,
    score: f64,
    decision: String,
    rationale: Vec<String>,
    signals: PrSignals,
}

#[derive(Debug, Serialize)]
struct PrSignals {
    draft: bool,
    comments: u64,
    review_comments: u64,
    additions: u64,
    deletions: u64,
    changed_files: u64,
    mergeable_state: Option<String>,
    approvals: u64,
    change_requests: u64,
    ci_state: Option<String>,
    vision_alignment: Option<f64>,
}

#[derive(Debug, Serialize)]
struct TriageReport {
    repo: String,
    state: String,
    generated_at: String,
    scanned_prs: usize,
    scanned_issues: usize,
    duplicate_pairs: Vec<DuplicatePair>,
    ranked_prs: Vec<PrScoreReport>,
}

#[derive(Debug, Deserialize, Clone)]
struct GithubUser {
    login: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct PullRef {
    sha: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct GithubPull {
    number: u64,
    title: Option<String>,
    body: Option<String>,
    html_url: Option<String>,
    user: Option<GithubUser>,
    created_at: Option<String>,
    draft: Option<bool>,
    comments: Option<u64>,
    review_comments: Option<u64>,
    additions: Option<u64>,
    deletions: Option<u64>,
    changed_files: Option<u64>,
    mergeable_state: Option<String>,
    head: Option<PullRef>,
}

#[derive(Debug, Deserialize, Clone)]
struct PullDetailFile {
    filename: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct GithubIssue {
    number: u64,
    title: Option<String>,
    body: Option<String>,
    html_url: Option<String>,
    user: Option<GithubUser>,
    created_at: Option<String>,
    pull_request: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GithubReview {
    user: Option<GithubUser>,
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommitStatus {
    state: Option<String>,
}

#[derive(Debug)]
struct DeepSignals {
    approvals: u64,
    change_requests: u64,
    ci_state: Option<String>,
    changed_paths: Vec<String>,
}

#[derive(Debug)]
struct VisionModel {
    token_counts: HashMap<String, f64>,
    norm: f64,
}

pub fn run(args: TriageArgs) -> Result<(), Box<dyn Error>> {
    let state = normalize_state(&args.state)?;
    let token = args.token.clone().or_else(|| env::var("GITHUB_TOKEN").ok());

    let gh = GithubClient::new(token)?;
    let pulls = gh.fetch_pulls(&args.repo, state, args.limit)?;
    let issues = gh.fetch_issues(&args.repo, state, args.limit)?;

    let vision_model = if let Some(path) = args.vision.as_ref() {
        let content = fs::read_to_string(path)?;
        Some(build_vision_model(&content))
    } else {
        None
    };

    let mut items = Vec::new();
    for pr in &pulls {
        items.push(work_item_from_pr(pr));
    }
    for issue in &issues {
        if issue.pull_request.is_none() {
            items.push(work_item_from_issue(issue));
        }
    }

    let duplicate_pairs = find_duplicates(&items, args.dedupe_threshold);

    let mut scored: Vec<PrScoreReport> = pulls
        .iter()
        .map(|pr| score_pr(pr, None, vision_model.as_ref()))
        .collect();

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

    let deep_count = args.deep_review_top.min(scored.len());
    for report in scored.iter_mut().take(deep_count) {
        if let Some(pr) = pulls.iter().find(|p| p.number == report.number) {
            let deep = gh.fetch_deep_signals(&args.repo, pr.number, pr.head.as_ref().and_then(|h| h.sha.clone()))?;
            *report = score_pr(pr, Some(deep), vision_model.as_ref());
        }
    }

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

    let report = TriageReport {
        repo: args.repo.clone(),
        state: state.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        scanned_prs: pulls.len(),
        scanned_issues: issues.iter().filter(|i| i.pull_request.is_none()).count(),
        duplicate_pairs,
        ranked_prs: scored,
    };

    let json_report = serde_json::to_string_pretty(&report)?;

    if let Some(path) = args.out.as_ref() {
        fs::write(path, &json_report)?;
    }

    if args.json_only {
        println!("{}", json_report);
        return Ok(());
    }

    print_summary(&report, args.out.as_ref());
    println!("\n{}", json_report);

    Ok(())
}

fn print_summary(report: &TriageReport, out: Option<&PathBuf>) {
    println!("repo: {}", report.repo);
    println!("state: {}", report.state);
    println!("scanned: {} PRs, {} Issues", report.scanned_prs, report.scanned_issues);
    println!("duplicates found: {}", report.duplicate_pairs.len());

    if !report.ranked_prs.is_empty() {
        println!("top PR candidates:");
        for pr in report.ranked_prs.iter().take(5) {
            println!(
                "  #{} [{:.1}] {} ({})",
                pr.number, pr.score, pr.title, pr.decision
            );
        }
    }

    if let Some(path) = out {
        println!("report written to: {}", path.display());
    }
}

struct GithubClient {
    client: Client,
}

impl GithubClient {
    fn new(token: Option<String>) -> Result<Self, Box<dyn Error>> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("osmogrep-triage"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );

        if let Some(tok) = token {
            let value = format!("Bearer {}", tok);
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&value)?);
        }

        let client = Client::builder().default_headers(headers).build()?;
        Ok(Self { client })
    }

    fn fetch_pulls(
        &self,
        repo: &str,
        state: &str,
        limit: usize,
    ) -> Result<Vec<GithubPull>, Box<dyn Error>> {
        let mut page = 1;
        let mut out = Vec::new();
        while out.len() < limit {
            let url = format!(
                "https://api.github.com/repos/{repo}/pulls?state={state}&per_page=100&page={page}&sort=updated&direction=desc"
            );
            let mut chunk: Vec<GithubPull> = self.client.get(&url).send()?.error_for_status()?.json()?;
            if chunk.is_empty() {
                break;
            }
            out.append(&mut chunk);
            page += 1;
        }
        out.truncate(limit);
        Ok(out)
    }

    fn fetch_issues(
        &self,
        repo: &str,
        state: &str,
        limit: usize,
    ) -> Result<Vec<GithubIssue>, Box<dyn Error>> {
        let mut page = 1;
        let mut out = Vec::new();
        while out.len() < limit {
            let url = format!(
                "https://api.github.com/repos/{repo}/issues?state={state}&per_page=100&page={page}&sort=updated&direction=desc"
            );
            let mut chunk: Vec<GithubIssue> = self.client.get(&url).send()?.error_for_status()?.json()?;
            if chunk.is_empty() {
                break;
            }
            out.append(&mut chunk);
            page += 1;
        }
        out.truncate(limit);
        Ok(out)
    }

    fn fetch_deep_signals(
        &self,
        repo: &str,
        pr_number: u64,
        head_sha: Option<String>,
    ) -> Result<DeepSignals, Box<dyn Error>> {
        let reviews_url = format!(
            "https://api.github.com/repos/{repo}/pulls/{pr_number}/reviews?per_page=100"
        );
        let reviews: Vec<GithubReview> = self
            .client
            .get(&reviews_url)
            .send()?
            .error_for_status()?
            .json()?;

        let mut latest_by_user: HashMap<String, String> = HashMap::new();
        for review in reviews {
            if let (Some(user), Some(state)) = (
                review.user.and_then(|u| u.login),
                review.state.map(|s| s.to_ascii_uppercase()),
            ) {
                latest_by_user.insert(user, state);
            }
        }

        let approvals = latest_by_user
            .values()
            .filter(|s| s.as_str() == "APPROVED")
            .count() as u64;

        let change_requests = latest_by_user
            .values()
            .filter(|s| s.as_str() == "CHANGES_REQUESTED")
            .count() as u64;

        let ci_state = if let Some(sha) = head_sha {
            let status_url = format!("https://api.github.com/repos/{repo}/commits/{sha}/status");
            let status: CommitStatus = self
                .client
                .get(&status_url)
                .send()?
                .error_for_status()?
                .json()?;
            status.state
        } else {
            None
        };

        let files_url = format!(
            "https://api.github.com/repos/{repo}/pulls/{pr_number}/files?per_page=100"
        );
        let files: Vec<PullDetailFile> = self
            .client
            .get(&files_url)
            .send()?
            .error_for_status()?
            .json()?;

        let changed_paths = files
            .into_iter()
            .filter_map(|f| f.filename)
            .collect::<Vec<_>>();

        Ok(DeepSignals {
            approvals,
            change_requests,
            ci_state,
            changed_paths,
        })
    }
}

fn normalize_state(state: &str) -> Result<&'static str, Box<dyn Error>> {
    match state {
        "open" => Ok("open"),
        "closed" => Ok("closed"),
        "all" => Ok("all"),
        _ => Err(format!("invalid --state '{}'; expected open|closed|all", state).into()),
    }
}

fn work_item_from_pr(pr: &GithubPull) -> WorkItem {
    let title = pr.title.clone().unwrap_or_default();
    let body = pr.body.clone().unwrap_or_default();
    let text = format!("{}\n{}", title, body);

    WorkItem {
        kind: ItemKind::PullRequest,
        number: pr.number,
        title: title.clone(),
        body,
        url: pr.html_url.clone().unwrap_or_default(),
        author: pr
            .user
            .as_ref()
            .and_then(|u| u.login.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        created_at: pr.created_at.clone().unwrap_or_default(),
        token_set: tokenize(&text),
        title_token_set: tokenize(&title),
    }
}

fn work_item_from_issue(issue: &GithubIssue) -> WorkItem {
    let title = issue.title.clone().unwrap_or_default();
    let body = issue.body.clone().unwrap_or_default();
    let text = format!("{}\n{}", title, body);

    WorkItem {
        kind: ItemKind::Issue,
        number: issue.number,
        title: title.clone(),
        body,
        url: issue.html_url.clone().unwrap_or_default(),
        author: issue
            .user
            .as_ref()
            .and_then(|u| u.login.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        created_at: issue.created_at.clone().unwrap_or_default(),
        token_set: tokenize(&text),
        title_token_set: tokenize(&title),
    }
}

fn find_duplicates(items: &[WorkItem], threshold: f64) -> Vec<DuplicatePair> {
    let mut out = Vec::new();

    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            let a = &items[i];
            let b = &items[j];

            let shared_title_tokens = a
                .title_token_set
                .intersection(&b.title_token_set)
                .count();

            if shared_title_tokens == 0 {
                continue;
            }

            let title_sim = dice_coefficient(&a.title, &b.title);
            let body_a = if a.body.len() > 1200 { &a.body[..1200] } else { &a.body };
            let body_b = if b.body.len() > 1200 { &b.body[..1200] } else { &b.body };
            let text_sim = jaccard(&a.token_set, &b.token_set);
            let body_sim = dice_coefficient(body_a, body_b);

            let mut similarity = (title_sim * 0.55) + (text_sim * 0.30) + (body_sim * 0.15);
            if a.author == b.author {
                similarity += 0.03;
            }
            if is_date_near(&a.created_at, &b.created_at, 14) {
                similarity += 0.02;
            }

            if similarity >= threshold {
                let rationale = format!(
                    "title_dice={:.2}, text_jaccard={:.2}, body_dice={:.2}, shared_title_tokens={}",
                    title_sim, text_sim, body_sim, shared_title_tokens
                );

                out.push(DuplicatePair {
                    left_kind: a.kind.clone(),
                    left_number: a.number,
                    left_title: a.title.clone(),
                    left_url: a.url.clone(),
                    right_kind: b.kind.clone(),
                    right_number: b.number,
                    right_title: b.title.clone(),
                    right_url: b.url.clone(),
                    similarity,
                    rationale,
                });
            }
        }
    }

    out.sort_by(|x, y| y.similarity.partial_cmp(&x.similarity).unwrap_or(Ordering::Equal));
    out
}

fn score_pr(pr: &GithubPull, deep: Option<DeepSignals>, vision: Option<&VisionModel>) -> PrScoreReport {
    let title = pr.title.clone().unwrap_or_default();
    let body = pr.body.clone().unwrap_or_default();
    let mut score = 50.0;
    let mut rationale = Vec::new();

    let draft = pr.draft.unwrap_or(false);
    if draft {
        score -= 25.0;
        rationale.push("draft PR".to_string());
    } else {
        score += 18.0;
        rationale.push("ready for review".to_string());
    }

    let comments = pr.comments.unwrap_or(0);
    let review_comments = pr.review_comments.unwrap_or(0);
    let additions = pr.additions.unwrap_or(0);
    let deletions = pr.deletions.unwrap_or(0);
    let changed_files = pr.changed_files.unwrap_or(0);

    if comments > 0 {
        let penalty = (comments as f64 * 0.4).min(8.0);
        score -= penalty;
        rationale.push(format!("discussion load -{:.1}", penalty));
    }

    if review_comments > 0 {
        let penalty = (review_comments as f64 * 0.35).min(10.0);
        score -= penalty;
        rationale.push(format!("review friction -{:.1}", penalty));
    }

    let churn = additions + deletions;
    if churn <= 450 {
        score += 8.0;
        rationale.push("small-medium change set".to_string());
    } else if churn <= 1800 {
        score += 2.5;
    } else {
        score -= 10.0;
        rationale.push("very large change set".to_string());
    }

    if changed_files > 0 {
        if changed_files <= 15 {
            score += 5.0;
            rationale.push("focused file footprint".to_string());
        } else if changed_files > 50 {
            score -= 8.0;
            rationale.push("wide file footprint".to_string());
        }
    }

    let mut approvals = 0;
    let mut change_requests = 0;
    let mut ci_state = None;
    let mut mergeable_state = pr.mergeable_state.clone();
    let mut changed_paths = Vec::new();

    if let Some(ds) = deep {
        approvals = ds.approvals;
        change_requests = ds.change_requests;
        ci_state = ds.ci_state;
        changed_paths = ds.changed_paths;
    }

    if approvals > 0 {
        let bonus = (approvals as f64 * 6.0).min(24.0);
        score += bonus;
        rationale.push(format!("approvals +{:.1}", bonus));
    }

    if change_requests > 0 {
        let penalty = (change_requests as f64 * 12.0).min(24.0);
        score -= penalty;
        rationale.push(format!("changes requested -{:.1}", penalty));
    }

    if let Some(ci) = ci_state.clone() {
        match ci.as_str() {
            "success" => {
                score += 10.0;
                rationale.push("CI green".to_string());
            }
            "failure" | "error" => {
                score -= 12.0;
                rationale.push("CI failing".to_string());
            }
            "pending" => {
                score -= 3.0;
                rationale.push("CI pending".to_string());
            }
            _ => {}
        }
    }

    if let Some(ms) = mergeable_state.clone() {
        match ms.as_str() {
            "clean" => {
                score += 8.0;
                rationale.push("mergeable cleanly".to_string());
            }
            "dirty" => {
                score -= 8.0;
                rationale.push("merge conflicts".to_string());
            }
            "blocked" => {
                score -= 6.0;
                rationale.push("merge blocked".to_string());
            }
            _ => {}
        }
    } else {
        mergeable_state = None;
    }

    let vision_alignment = vision.map(|vm| {
        let mut text = format!("{}\n{}", title, body);
        if !changed_paths.is_empty() {
            text.push('\n');
            text.push_str(&changed_paths.join("\n"));
        }
        cosine_against_vision(vm, &text)
    });

    if let Some(v) = vision_alignment {
        if v < 0.25 {
            score -= 18.0;
            rationale.push(format!("low vision alignment ({:.2})", v));
        } else if v >= 0.45 {
            score += 9.0;
            rationale.push(format!("strong vision alignment ({:.2})", v));
        }
    }

    let decision = if score >= 85.0 {
        "priority_review"
    } else if score >= 65.0 {
        "review"
    } else if score >= 45.0 {
        "needs_triage"
    } else {
        "reject_candidate"
    }
    .to_string();

    PrScoreReport {
        number: pr.number,
        title,
        url: pr.html_url.clone().unwrap_or_default(),
        author: pr
            .user
            .as_ref()
            .and_then(|u| u.login.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        score,
        decision,
        rationale,
        signals: PrSignals {
            draft,
            comments,
            review_comments,
            additions,
            deletions,
            changed_files,
            mergeable_state,
            approvals,
            change_requests,
            ci_state,
            vision_alignment,
        },
    }
}

fn tokenize(text: &str) -> HashSet<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(stem_token)
        .filter(|t| !is_stopword(t))
        .collect()
}

fn stem_token(token: &str) -> String {
    if token.ends_with("ing") && token.len() > 5 {
        token[..token.len() - 3].to_string()
    } else if token.ends_with("ed") && token.len() > 4 {
        token[..token.len() - 2].to_string()
    } else if token.ends_with('s') && token.len() > 3 {
        token[..token.len() - 1].to_string()
    } else {
        token.to_string()
    }
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "the"
            | "and"
            | "for"
            | "with"
            | "that"
            | "this"
            | "from"
            | "into"
            | "have"
            | "your"
            | "you"
            | "are"
            | "was"
            | "were"
            | "not"
            | "can"
            | "will"
            | "all"
            | "any"
            | "just"
            | "about"
            | "there"
            | "their"
            | "when"
            | "where"
            | "what"
            | "how"
    )
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }

    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn dice_coefficient(left: &str, right: &str) -> f64 {
    let l = normalize_text(left);
    let r = normalize_text(right);

    if l.is_empty() && r.is_empty() {
        return 1.0;
    }
    if l.len() < 2 || r.len() < 2 {
        return 0.0;
    }

    let lb = bigrams(&l);
    let rb = bigrams(&r);

    if lb.is_empty() || rb.is_empty() {
        return 0.0;
    }

    let mut match_count = 0usize;
    let mut rb_map: HashMap<&str, usize> = HashMap::new();
    for b in &rb {
        *rb_map.entry(b.as_str()).or_insert(0) += 1;
    }

    for b in &lb {
        if let Some(v) = rb_map.get_mut(b.as_str()) {
            if *v > 0 {
                *v -= 1;
                match_count += 1;
            }
        }
    }

    (2.0 * match_count as f64) / (lb.len() as f64 + rb.len() as f64)
}

fn normalize_text(s: &str) -> String {
    s.to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c.is_ascii_whitespace() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn bigrams(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(chars.len() - 1);
    for i in 0..(chars.len() - 1) {
        out.push(format!("{}{}", chars[i], chars[i + 1]));
    }
    out
}

fn is_date_near(a: &str, b: &str, within_days: i64) -> bool {
    let da = parse_date(a);
    let db = parse_date(b);

    match (da, db) {
        (Some(x), Some(y)) => (x - y).num_days().abs() <= within_days,
        _ => false,
    }
}

fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

fn build_vision_model(text: &str) -> VisionModel {
    let mut token_counts: HashMap<String, f64> = HashMap::new();
    for tok in tokenize(text) {
        *token_counts.entry(tok).or_insert(0.0) += 1.0;
    }

    let norm = token_counts.values().map(|v| v * v).sum::<f64>().sqrt();
    VisionModel { token_counts, norm }
}

fn cosine_against_vision(vision: &VisionModel, text: &str) -> f64 {
    if vision.norm == 0.0 {
        return 0.0;
    }

    let mut probe: HashMap<String, f64> = HashMap::new();
    for tok in tokenize(text) {
        *probe.entry(tok).or_insert(0.0) += 1.0;
    }

    let probe_norm = probe.values().map(|v| v * v).sum::<f64>().sqrt();
    if probe_norm == 0.0 {
        return 0.0;
    }

    let dot = probe
        .iter()
        .filter_map(|(k, v)| vision.token_counts.get(k).map(|w| v * w))
        .sum::<f64>();

    dot / (vision.norm * probe_norm)
}
