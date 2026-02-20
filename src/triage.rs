use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use clap::Args;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};

#[derive(Args, Debug, Clone)]
pub struct TriageArgs {
    #[arg(long, help = "GitHub repository in owner/name form")]
    pub repo: String,

    #[arg(long, default_value = "open", help = "Item state: open | closed | all")]
    pub state: String,

    #[arg(
        long,
        default_value_t = 250,
        help = "Max PRs and issues each to analyze"
    )]
    pub limit: usize,

    #[arg(long, default_value_t = 15, help = "Top PRs to deep-review")]
    pub deep_review_top: usize,

    #[arg(
        long,
        default_value_t = false,
        help = "Deep-review every fetched PR (overrides --deep-review-top)"
    )]
    pub deep_review_all: bool,

    #[arg(
        long,
        default_value_t = 0.62,
        help = "Duplicate similarity threshold (0.0-1.0)"
    )]
    pub dedupe_threshold: f64,

    #[arg(
        long,
        default_value_t = 800_000,
        help = "Safety cap for duplicate-pair comparisons"
    )]
    pub max_pair_comparisons: usize,

    #[arg(long, help = "Only analyze items updated after this RFC3339 timestamp")]
    pub since: Option<String>,

    #[arg(
        long,
        default_value_t = false,
        help = "Use saved triage state to run incrementally"
    )]
    pub incremental: bool,

    #[arg(long, help = "Path to incremental triage state JSON")]
    pub state_file: Option<PathBuf>,

    #[arg(long, help = "Path to a vision document for scope alignment")]
    pub vision: Option<PathBuf>,

    #[arg(long, help = "GitHub token (or set GITHUB_TOKEN)")]
    pub token: Option<String>,

    #[arg(
        long,
        default_value_t = false,
        help = "Apply labels/comments to GitHub issues/PRs"
    )]
    pub apply_actions: bool,

    #[arg(
        long,
        default_value_t = false,
        help = "Include duplicate/vision comments in action plan"
    )]
    pub comment_actions: bool,

    #[arg(long, default_value_t = 50, help = "Max suggested/applied actions")]
    pub action_limit: usize,

    #[arg(
        long,
        default_value = "triage:duplicate-candidate",
        help = "Label for probable duplicates"
    )]
    pub label_duplicate: String,

    #[arg(
        long,
        default_value = "triage:priority-review",
        help = "Label for top-ranked PRs"
    )]
    pub label_priority: String,

    #[arg(
        long,
        default_value = "triage:vision-drift",
        help = "Label for vision-misaligned PRs"
    )]
    pub label_reject: String,

    #[arg(long, help = "Write full report JSON to this file")]
    pub out: Option<PathBuf>,

    #[arg(long, default_value_t = false, help = "Only print JSON report")]
    pub json_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
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
    updated_at: String,
    token_set: HashSet<String>,
    semantic_token_set: HashSet<String>,
    title_token_set: HashSet<String>,
    char_trigram_set: HashSet<String>,
}

#[derive(Debug, Serialize, Clone)]
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
    incremental: bool,
    since: Option<String>,
    state_file: String,
    max_seen_updated_at: Option<String>,
    scanned_prs: usize,
    scanned_issues: usize,
    duplicate_pairs: Vec<DuplicatePair>,
    ranked_prs: Vec<PrScoreReport>,
    planned_actions: Vec<TriageAction>,
    applied_action_count: usize,
}

#[derive(Debug, Serialize, Clone)]
struct TriageAction {
    item_kind: ItemKind,
    item_number: u64,
    item_url: String,
    action_type: String,
    value: String,
    reason: String,
    status: String,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct TriageState {
    repo: String,
    state: String,
    last_run_at: Option<String>,
    last_seen_updated_at: Option<String>,
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
    updated_at: Option<String>,
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
    updated_at: Option<String>,
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
    let state_file = resolve_state_file(args.state_file.clone(), &args.repo);

    let persisted = load_state_file(&state_file)?;
    let since = resolve_since(&args, persisted.as_ref())?;

    let gh = GithubClient::new(token.clone())?;
    let pulls = gh.fetch_pulls(&args.repo, state, args.limit, since.as_ref())?;
    let issues = gh.fetch_issues(&args.repo, state, args.limit, since.as_ref())?;

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

    let duplicate_pairs = find_duplicates(&items, args.dedupe_threshold, args.max_pair_comparisons);

    let mut scored: Vec<PrScoreReport> = pulls
        .iter()
        .map(|pr| score_pr(pr, None, vision_model.as_ref()))
        .collect();

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

    let deep_count = if args.deep_review_all {
        scored.len()
    } else {
        args.deep_review_top.min(scored.len())
    };

    for report in scored.iter_mut().take(deep_count) {
        if let Some(pr) = pulls.iter().find(|p| p.number == report.number) {
            let deep = gh.fetch_deep_signals(
                &args.repo,
                pr.number,
                pr.head.as_ref().and_then(|h| h.sha.clone()),
            )?;
            *report = score_pr(pr, Some(deep), vision_model.as_ref());
        }
    }

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

    let mut action_plan = build_action_plan(&scored, &duplicate_pairs, &args);
    if action_plan.len() > args.action_limit {
        action_plan.truncate(args.action_limit);
    }

    let mut applied_action_count = 0usize;
    if args.apply_actions {
        if token.is_none() {
            return Err("--apply-actions requires --token or GITHUB_TOKEN".into());
        }
        applied_action_count = apply_actions(&gh, &args.repo, &mut action_plan, args.action_limit);
    }

    let max_seen_updated_at = latest_seen_updated_at(&pulls, &issues);

    if args.incremental || args.state_file.is_some() {
        let state_value = TriageState {
            repo: args.repo.clone(),
            state: state.to_string(),
            last_run_at: Some(Utc::now().to_rfc3339()),
            last_seen_updated_at: max_seen_updated_at.clone().or_else(|| since.clone()),
        };
        save_state_file(&state_file, &state_value)?;
    }

    let report = TriageReport {
        repo: args.repo.clone(),
        state: state.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        incremental: args.incremental,
        since,
        state_file: state_file.display().to_string(),
        max_seen_updated_at,
        scanned_prs: pulls.len(),
        scanned_issues: issues.iter().filter(|i| i.pull_request.is_none()).count(),
        duplicate_pairs,
        ranked_prs: scored,
        planned_actions: action_plan,
        applied_action_count,
    };

    let json_report = serde_json::to_string_pretty(&report)?;

    if let Some(path) = args.out.as_ref() {
        fs::write(path, &json_report)?;
    }

    if args.json_only {
        println!("{}", json_report);
        return Ok(());
    }

    print_summary(&report, args.out.as_ref(), args.apply_actions);
    println!("\n{}", json_report);

    Ok(())
}

fn print_summary(report: &TriageReport, out: Option<&PathBuf>, apply_actions: bool) {
    println!("repo: {}", report.repo);
    println!("state: {}", report.state);
    println!(
        "scanned: {} PRs, {} Issues",
        report.scanned_prs, report.scanned_issues
    );
    println!("duplicates found: {}", report.duplicate_pairs.len());
    if let Some(since) = report.since.as_ref() {
        println!("since: {}", since);
    }

    if !report.ranked_prs.is_empty() {
        println!("top PR candidates:");
        for pr in report.ranked_prs.iter().take(5) {
            println!(
                "  #{} [{:.1}] {} ({})",
                pr.number, pr.score, pr.title, pr.decision
            );
        }
    }

    if !report.planned_actions.is_empty() {
        println!("planned actions: {}", report.planned_actions.len());
    }

    if apply_actions {
        println!("applied actions: {}", report.applied_action_count);
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
        since: Option<&String>,
    ) -> Result<Vec<GithubPull>, Box<dyn Error>> {
        let since_dt = since.and_then(|s| parse_date(s));

        let mut page = 1;
        let mut out = Vec::new();
        while out.len() < limit {
            let url = format!(
                "https://api.github.com/repos/{repo}/pulls?state={state}&per_page=100&page={page}&sort=updated&direction=desc"
            );
            let chunk: Vec<GithubPull> =
                self.client.get(&url).send()?.error_for_status()?.json()?;
            if chunk.is_empty() {
                break;
            }

            let mut reached_old = false;
            for pr in chunk {
                if since_dt.is_some() {
                    if let Some(updated) = pr.updated_at.as_ref().and_then(|d| parse_date(d)) {
                        if updated <= since_dt.unwrap() {
                            reached_old = true;
                            continue;
                        }
                    }
                }
                out.push(pr);
                if out.len() >= limit {
                    break;
                }
            }

            if reached_old {
                break;
            }

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
        since: Option<&String>,
    ) -> Result<Vec<GithubIssue>, Box<dyn Error>> {
        let mut page = 1;
        let mut out = Vec::new();
        while out.len() < limit {
            let mut url = format!(
                "https://api.github.com/repos/{repo}/issues?state={state}&per_page=100&page={page}&sort=updated&direction=desc"
            );
            if let Some(since_ts) = since {
                url.push_str("&since=");
                url.push_str(since_ts);
            }
            let mut chunk: Vec<GithubIssue> =
                self.client.get(&url).send()?.error_for_status()?.json()?;
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
        let reviews_url =
            format!("https://api.github.com/repos/{repo}/pulls/{pr_number}/reviews?per_page=100");
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

        let files_url =
            format!("https://api.github.com/repos/{repo}/pulls/{pr_number}/files?per_page=100");
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

    fn add_labels(&self, repo: &str, number: u64, labels: &[String]) -> Result<(), Box<dyn Error>> {
        let url = format!("https://api.github.com/repos/{repo}/issues/{number}/labels");
        self.client
            .post(&url)
            .json(&serde_json::json!({ "labels": labels }))
            .send()?
            .error_for_status()?;
        Ok(())
    }

    fn add_comment(&self, repo: &str, number: u64, body: &str) -> Result<(), Box<dyn Error>> {
        let url = format!("https://api.github.com/repos/{repo}/issues/{number}/comments");
        self.client
            .post(&url)
            .json(&serde_json::json!({ "body": body }))
            .send()?
            .error_for_status()?;
        Ok(())
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
    let token_set = tokenize(&text);

    WorkItem {
        kind: ItemKind::PullRequest,
        number: pr.number,
        title: title.clone(),
        body: body.clone(),
        url: pr.html_url.clone().unwrap_or_default(),
        author: pr
            .user
            .as_ref()
            .and_then(|u| u.login.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        created_at: pr.created_at.clone().unwrap_or_default(),
        updated_at: pr
            .updated_at
            .clone()
            .or_else(|| pr.created_at.clone())
            .unwrap_or_default(),
        semantic_token_set: semanticize_tokens(&token_set),
        title_token_set: tokenize(&title),
        char_trigram_set: char_ngrams(&text, 3, 2200),
        token_set,
    }
}

fn work_item_from_issue(issue: &GithubIssue) -> WorkItem {
    let title = issue.title.clone().unwrap_or_default();
    let body = issue.body.clone().unwrap_or_default();
    let text = format!("{}\n{}", title, body);
    let token_set = tokenize(&text);

    WorkItem {
        kind: ItemKind::Issue,
        number: issue.number,
        title: title.clone(),
        body: body.clone(),
        url: issue.html_url.clone().unwrap_or_default(),
        author: issue
            .user
            .as_ref()
            .and_then(|u| u.login.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        created_at: issue.created_at.clone().unwrap_or_default(),
        updated_at: issue
            .updated_at
            .clone()
            .or_else(|| issue.created_at.clone())
            .unwrap_or_default(),
        semantic_token_set: semanticize_tokens(&token_set),
        title_token_set: tokenize(&title),
        char_trigram_set: char_ngrams(&text, 3, 2200),
        token_set,
    }
}

fn find_duplicates(
    items: &[WorkItem],
    threshold: f64,
    max_pair_comparisons: usize,
) -> Vec<DuplicatePair> {
    let mut out = Vec::new();
    if items.len() < 2 {
        return out;
    }

    let candidates = candidate_pairs(items, max_pair_comparisons);

    for (i, j) in candidates {
        let a = &items[i];
        let b = &items[j];

        let shared_title_tokens = a.title_token_set.intersection(&b.title_token_set).count();

        let title_sim = dice_coefficient(&a.title, &b.title);
        let body_a = if a.body.len() > 1400 {
            &a.body[..1400]
        } else {
            &a.body
        };
        let body_b = if b.body.len() > 1400 {
            &b.body[..1400]
        } else {
            &b.body
        };
        let text_sim = jaccard(&a.token_set, &b.token_set);
        let body_sim = dice_coefficient(body_a, body_b);
        let semantic_sim = jaccard(&a.semantic_token_set, &b.semantic_token_set);
        let trigram_sim = jaccard(&a.char_trigram_set, &b.char_trigram_set);

        let mut similarity = (title_sim * 0.35)
            + (text_sim * 0.18)
            + (body_sim * 0.10)
            + (semantic_sim * 0.22)
            + (trigram_sim * 0.15);

        if a.author == b.author {
            similarity += 0.03;
        }
        if is_date_near(&a.created_at, &b.created_at, 14)
            || is_date_near(&a.updated_at, &b.updated_at, 14)
        {
            similarity += 0.02;
        }

        if shared_title_tokens == 0 && similarity < (threshold + 0.07) {
            continue;
        }

        if similarity >= threshold {
            let rationale = format!(
                "title_dice={:.2}, text_jaccard={:.2}, semantic_jaccard={:.2}, body_dice={:.2}, trigram_jaccard={:.2}, shared_title_tokens={}",
                title_sim, text_sim, semantic_sim, body_sim, trigram_sim, shared_title_tokens
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

    out.sort_by(|x, y| {
        y.similarity
            .partial_cmp(&x.similarity)
            .unwrap_or(Ordering::Equal)
    });
    out
}

fn candidate_pairs(items: &[WorkItem], max_pair_comparisons: usize) -> Vec<(usize, usize)> {
    let mut by_title_token: HashMap<&str, Vec<usize>> = HashMap::new();

    for (idx, item) in items.iter().enumerate() {
        for token in &item.title_token_set {
            by_title_token.entry(token.as_str()).or_default().push(idx);
        }
    }

    let mut pair_votes: HashMap<(usize, usize), u16> = HashMap::new();

    for indexes in by_title_token.values() {
        if indexes.len() < 2 || indexes.len() > 300 {
            continue;
        }

        for i in 0..indexes.len() {
            for j in (i + 1)..indexes.len() {
                let a = indexes[i];
                let b = indexes[j];
                let key = if a < b { (a, b) } else { (b, a) };
                let entry = pair_votes.entry(key).or_insert(0);
                *entry = entry.saturating_add(1);
            }
        }
    }

    let mut pairs: Vec<((usize, usize), u16)> = pair_votes.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1));

    if pairs.is_empty() {
        let mut fallback = Vec::new();
        for i in 0..items.len() {
            for j in (i + 1)..items.len() {
                fallback.push((i, j));
                if fallback.len() >= max_pair_comparisons {
                    return fallback;
                }
            }
        }
        return fallback;
    }

    pairs
        .into_iter()
        .take(max_pair_comparisons)
        .map(|(pair, _)| pair)
        .collect()
}

fn score_pr(
    pr: &GithubPull,
    deep: Option<DeepSignals>,
    vision: Option<&VisionModel>,
) -> PrScoreReport {
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

fn build_action_plan(
    scored: &[PrScoreReport],
    duplicates: &[DuplicatePair],
    args: &TriageArgs,
) -> Vec<TriageAction> {
    let mut actions = Vec::new();
    let mut uniq_label_keys: HashSet<(ItemKind, u64, String)> = HashSet::new();
    let mut uniq_comment_keys: HashSet<(ItemKind, u64)> = HashSet::new();
    let pr_scores: HashMap<u64, f64> = scored.iter().map(|p| (p.number, p.score)).collect();

    for pr in scored {
        if pr.decision == "priority_review" {
            push_label_action(
                &mut actions,
                &mut uniq_label_keys,
                ItemKind::PullRequest,
                pr.number,
                pr.url.clone(),
                args.label_priority.clone(),
                "High triage score".to_string(),
            );
        }

        if pr.decision == "reject_candidate" {
            if let Some(v) = pr.signals.vision_alignment {
                if v < 0.25 {
                    push_label_action(
                        &mut actions,
                        &mut uniq_label_keys,
                        ItemKind::PullRequest,
                        pr.number,
                        pr.url.clone(),
                        args.label_reject.clone(),
                        format!("Vision alignment is low ({:.2})", v),
                    );

                    if args.comment_actions
                        && uniq_comment_keys.insert((ItemKind::PullRequest, pr.number))
                    {
                        actions.push(TriageAction {
                            item_kind: ItemKind::PullRequest,
                            item_number: pr.number,
                            item_url: pr.url.clone(),
                            action_type: "comment".to_string(),
                            value: format!(
                                "Triage note: this PR appears to drift from VISION scope (alignment {:.2}). Please re-check goals and acceptance criteria.",
                                v
                            ),
                            reason: "Vision alignment warning".to_string(),
                            status: "planned".to_string(),
                            error: None,
                        });
                    }
                }
            }
        }
    }

    for dup in duplicates {
        if dup.similarity < (args.dedupe_threshold + 0.08) {
            continue;
        }

        let (canonical_kind, canonical_number, canonical_url, dup_kind, dup_number, dup_url) =
            choose_canonical_duplicate(dup, &pr_scores);

        push_label_action(
            &mut actions,
            &mut uniq_label_keys,
            dup_kind.clone(),
            dup_number,
            dup_url.clone(),
            args.label_duplicate.clone(),
            format!(
                "Likely duplicate of {:?} #{} (sim {:.2})",
                canonical_kind, canonical_number, dup.similarity
            ),
        );

        if args.comment_actions && uniq_comment_keys.insert((dup_kind.clone(), dup_number)) {
            actions.push(TriageAction {
                item_kind: dup_kind,
                item_number: dup_number,
                item_url: dup_url,
                action_type: "comment".to_string(),
                value: format!(
                    "Triage note: this looks like a probable duplicate of {:?} #{} ({}), similarity {:.2}. Consider consolidating discussion there.",
                    canonical_kind, canonical_number, canonical_url, dup.similarity
                ),
                reason: "Probable duplicate".to_string(),
                status: "planned".to_string(),
                error: None,
            });
        }
    }

    actions
}

fn choose_canonical_duplicate(
    dup: &DuplicatePair,
    pr_scores: &HashMap<u64, f64>,
) -> (ItemKind, u64, String, ItemKind, u64, String) {
    match (&dup.left_kind, &dup.right_kind) {
        (ItemKind::PullRequest, ItemKind::PullRequest) => {
            let left = pr_scores.get(&dup.left_number).copied().unwrap_or(0.0);
            let right = pr_scores.get(&dup.right_number).copied().unwrap_or(0.0);
            if left >= right {
                (
                    dup.left_kind.clone(),
                    dup.left_number,
                    dup.left_url.clone(),
                    dup.right_kind.clone(),
                    dup.right_number,
                    dup.right_url.clone(),
                )
            } else {
                (
                    dup.right_kind.clone(),
                    dup.right_number,
                    dup.right_url.clone(),
                    dup.left_kind.clone(),
                    dup.left_number,
                    dup.left_url.clone(),
                )
            }
        }
        _ => {
            if dup.left_number <= dup.right_number {
                (
                    dup.left_kind.clone(),
                    dup.left_number,
                    dup.left_url.clone(),
                    dup.right_kind.clone(),
                    dup.right_number,
                    dup.right_url.clone(),
                )
            } else {
                (
                    dup.right_kind.clone(),
                    dup.right_number,
                    dup.right_url.clone(),
                    dup.left_kind.clone(),
                    dup.left_number,
                    dup.left_url.clone(),
                )
            }
        }
    }
}

fn push_label_action(
    actions: &mut Vec<TriageAction>,
    uniq: &mut HashSet<(ItemKind, u64, String)>,
    kind: ItemKind,
    number: u64,
    url: String,
    label: String,
    reason: String,
) {
    if !uniq.insert((kind.clone(), number, label.clone())) {
        return;
    }

    actions.push(TriageAction {
        item_kind: kind,
        item_number: number,
        item_url: url,
        action_type: "label".to_string(),
        value: label,
        reason,
        status: "planned".to_string(),
        error: None,
    });
}

fn apply_actions(
    gh: &GithubClient,
    repo: &str,
    actions: &mut [TriageAction],
    action_limit: usize,
) -> usize {
    let mut applied = 0usize;

    for action in actions.iter_mut().take(action_limit) {
        let result = match action.action_type.as_str() {
            "label" => gh.add_labels(repo, action.item_number, &[action.value.clone()]),
            "comment" => gh.add_comment(repo, action.item_number, &action.value),
            _ => Err("unknown action type".into()),
        };

        match result {
            Ok(_) => {
                action.status = "applied".to_string();
                action.error = None;
                applied += 1;
            }
            Err(e) => {
                action.status = "failed".to_string();
                action.error = Some(e.to_string());
            }
        }
    }

    applied
}

fn resolve_state_file(path: Option<PathBuf>, repo: &str) -> PathBuf {
    if let Some(path) = path {
        return path;
    }

    let name = repo.replace('/', "_");
    PathBuf::from(format!(".context/triage-state-{name}.json"))
}

fn load_state_file(path: &Path) -> Result<Option<TriageState>, Box<dyn Error>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path)?;
    let state: TriageState = serde_json::from_str(&raw)?;
    Ok(Some(state))
}

fn save_state_file(path: &Path, value: &TriageState) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}

fn resolve_since(
    args: &TriageArgs,
    persisted: Option<&TriageState>,
) -> Result<Option<String>, Box<dyn Error>> {
    if let Some(since) = args.since.as_ref() {
        if parse_date(since).is_none() {
            return Err(format!("invalid --since timestamp: {since}").into());
        }
        return Ok(Some(since.clone()));
    }

    if args.incremental {
        if let Some(last_seen) = persisted.and_then(|s| s.last_seen_updated_at.clone()) {
            let dt = parse_date(&last_seen)
                .ok_or_else(|| format!("invalid saved state timestamp: {last_seen}"))?;
            let overlap = dt - Duration::minutes(10);
            return Ok(Some(overlap.to_rfc3339()));
        }
    }

    Ok(None)
}

fn latest_seen_updated_at(pulls: &[GithubPull], issues: &[GithubIssue]) -> Option<String> {
    let mut latest: Option<DateTime<Utc>> = None;

    for pr in pulls {
        if let Some(dt) = pr.updated_at.as_ref().and_then(|d| parse_date(d)) {
            latest = Some(match latest {
                Some(curr) if curr > dt => curr,
                _ => dt,
            });
        }
    }

    for issue in issues {
        if let Some(dt) = issue.updated_at.as_ref().and_then(|d| parse_date(d)) {
            latest = Some(match latest {
                Some(curr) if curr > dt => curr,
                _ => dt,
            });
        }
    }

    latest.map(|d| d.to_rfc3339())
}

fn tokenize(text: &str) -> HashSet<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(stem_token)
        .filter(|t| !is_stopword(t))
        .collect()
}

fn semanticize_tokens(tokens: &HashSet<String>) -> HashSet<String> {
    tokens
        .iter()
        .map(|t| semantic_root(t))
        .collect::<HashSet<_>>()
}

fn semantic_root(token: &str) -> String {
    match token {
        "bug" | "fault" | "error" | "defect" | "panic" | "crash" => "bug".to_string(),
        "fix" | "resolve" | "repair" | "patch" | "hotfix" => "fix".to_string(),
        "performance" | "perf" | "latency" | "throughput" => "performance".to_string(),
        "doc" | "docs" | "readme" | "documentation" => "docs".to_string(),
        "refactor" | "cleanup" | "rework" => "refactor".to_string(),
        "auth" | "authentication" | "oauth" | "login" => "auth".to_string(),
        "api" | "endpoint" | "route" => "api".to_string(),
        _ => token.to_string(),
    }
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
        .map(|c| {
            if c.is_ascii_alphanumeric() || c.is_ascii_whitespace() {
                c
            } else {
                ' '
            }
        })
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

fn char_ngrams(s: &str, n: usize, max_chars: usize) -> HashSet<String> {
    let normalized = normalize_text(s);
    let chars: Vec<char> = normalized.chars().take(max_chars).collect();
    if chars.len() < n || n == 0 {
        return HashSet::new();
    }

    let mut out = HashSet::new();
    for i in 0..=(chars.len() - n) {
        out.insert(chars[i..i + n].iter().collect::<String>());
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
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_root_groups_aliases() {
        assert_eq!(semantic_root("crash"), "bug");
        assert_eq!(semantic_root("documentation"), "docs");
        assert_eq!(semantic_root("route"), "api");
    }

    #[test]
    fn candidate_pairs_returns_items_with_shared_title_tokens() {
        let items = vec![
            WorkItem {
                kind: ItemKind::Issue,
                number: 1,
                title: "fix login bug".to_string(),
                body: String::new(),
                url: String::new(),
                author: "a".to_string(),
                created_at: String::new(),
                updated_at: String::new(),
                token_set: tokenize("fix login bug"),
                semantic_token_set: tokenize("fix login bug"),
                title_token_set: tokenize("fix login bug"),
                char_trigram_set: HashSet::new(),
            },
            WorkItem {
                kind: ItemKind::Issue,
                number: 2,
                title: "repair auth crash".to_string(),
                body: String::new(),
                url: String::new(),
                author: "b".to_string(),
                created_at: String::new(),
                updated_at: String::new(),
                token_set: tokenize("repair auth crash"),
                semantic_token_set: tokenize("repair auth crash"),
                title_token_set: tokenize("repair auth crash"),
                char_trigram_set: HashSet::new(),
            },
            WorkItem {
                kind: ItemKind::Issue,
                number: 3,
                title: "update docs".to_string(),
                body: String::new(),
                url: String::new(),
                author: "c".to_string(),
                created_at: String::new(),
                updated_at: String::new(),
                token_set: tokenize("update docs"),
                semantic_token_set: tokenize("update docs"),
                title_token_set: tokenize("update docs"),
                char_trigram_set: HashSet::new(),
            },
        ];

        let pairs = candidate_pairs(&items, 100);
        assert!(!pairs.is_empty());
    }
}
