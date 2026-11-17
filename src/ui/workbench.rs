//! Rust owns panel state and actions; both renderers consume the same safe view.
use crate::{
    agent::ModelConfig,
    commands, persistence,
    preferences::STATUS_FIELDS,
    state::{AgentState, PermissionProfile, UiAccent, UiDensity, UiTheme},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Serialize;

const SECTIONS: &[&str] = &[
    "Commands",
    "Appearance",
    "Model",
    "Permissions",
    "Status line",
    "Keymap",
    "Skills",
    "Usage",
    "Stats",
    "Status",
    "Sessions",
    "Team",
    "Settings",
    "Workspace",
    "Doctor",
];

pub struct Panel {
    health: Option<crate::diagnostics::Report>,
    health_scan: Option<std::sync::mpsc::Receiver<crate::diagnostics::Event>>,
    health_checks: Vec<crate::diagnostics::Check>,
    health_started: std::time::Instant,
    health_deep: bool,
    load_health: bool,
    team_history: Vec<serde_json::Value>,
    team_before: Option<i64>,
    load_team: bool,
    message_preview: bool,
    settings: bool,
    filter: usize,
    preview_session: Option<String>,
    preview_lines: Vec<String>,
    preview_offset: usize,
    preview_refreshed: std::time::Instant,
    selected_session: Option<String>,
    message_to: Option<(String, String)>,
    pending_receipt: Option<(i64, std::time::Instant)>,
    import_sessions: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    load_sessions: bool,
    pub section: usize,
    pub query: String,
    pub selected: usize,
    pub notice: String,
    model: ModelConfig,
    edit: Option<(usize, String)>,
    capture: Option<String>,
    skills: Vec<crate::skills::Skill>,
    days: Vec<crate::usage::Day>,
    demo: bool,
    opened: std::time::Instant,
    picker: Option<usize>,
    models: Vec<String>,
    catalog: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
    load_models: bool,
}

#[derive(Clone)]
enum Action {
    HealthScan(bool),
    HealthRepair,
    HealthDetails,
    HealthAsk,
    TeamMessage(i64),
    TeamPage(Option<i64>),
    Section(usize),
    Session {
        id: String,
        root: std::path::PathBuf,
        online: bool,
    },
    Command(String),
    Theme(&'static str),
    Accent(&'static str),
    Density(&'static str),
    Profile(PermissionProfile),
    Approval,
    Motion,
    Art,
    Status(String),
    EditModel(usize),
    CustomModel(usize),
    ChooseProvider(String),
    ChooseModel(String),
    ApplyModel,
    CycleApi,
    Reasoning,
    Sandbox,
    KeyBinding(String),
    Skill(String),
    None,
}
#[derive(Serialize)]
pub struct Row {
    pub id: Option<String>,
    pub group: Option<String>,
    pub tag: Option<String>,
    pub label: String,
    pub description: String,
    pub value: String,
    #[serde(skip)]
    action: Action,
}
fn row(
    label: impl Into<String>,
    description: impl Into<String>,
    value: impl Into<String>,
    action: Action,
) -> Row {
    Row {
        id: None,
        group: None,
        tag: None,
        label: label.into(),
        description: description.into(),
        value: value.into(),
        action,
    }
}
#[derive(Serialize)]
pub struct PanelView {
    pub settings: bool,
    pub previewing: bool,
    pub preview_offset: usize,
    pub center_filter: usize,
    pub counts: Vec<usize>,
    pub title: String,
    pub selecting: bool,
    pub elapsed: u64,
    pub health_running: bool,
    pub sections: Vec<&'static str>,
    pub section: usize,
    pub query: String,
    pub rows: Vec<Row>,
    pub selected: usize,
    pub notice: String,
    pub footer: String,
    pub editing: bool,
    pub preview: Vec<String>,
    pub history: Vec<crate::usage::Day>,
    pub demo: bool,
}

pub fn section_for(command: &str) -> Option<usize> {
    Some(match command {
        "/help" => 0,
        "/settings" | "/config" => 12,
        "/theme" | "/color" | "/type" => 1,
        "/model" | "/providers" | "/account" => 2,
        "/permissions" | "/profile" => 3,
        "/statusline" => 4,
        "/keymap" => 5,
        "/skills" => 6,
        "/usage" => 7,
        "/stats" => 8,
        "/status" | "/metrics" => 9,
        "/sessions" => 10,
        "/team" => 11,
        "/workspace" => 13,
        "/doctor" => 14,
        _ => return None,
    })
}
pub fn open(state: &mut AgentState, section: usize, model: Option<&ModelConfig>) {
    state.ui.command_items.clear();
    let (days, notice) = load_usage(state, section);
    state.ui.panel = Some(Panel {
        health: None,
        health_scan: None,
        health_checks: vec![],
        health_started: std::time::Instant::now(),
        health_deep: false,
        load_health: section == 14,
        team_history: Vec::new(),
        team_before: None,
        load_team: section == 11,
        message_preview: false,
        settings: section == 12,
        filter: 0,
        preview_session: None,
        preview_lines: Vec::new(),
        preview_offset: 0,
        preview_refreshed: std::time::Instant::now(),
        selected_session: None,
        message_to: None,
        pending_receipt: None,
        import_sessions: None,
        load_sessions: section == 10,
        section,
        query: String::new(),
        selected: 0,
        notice,
        model: model.cloned().unwrap_or_default(),
        edit: None,
        demo: section == 8,
        opened: std::time::Instant::now(),
        picker: None,
        models: Vec::new(),
        catalog: None,
        load_models: false,
        days,
        capture: None,
        skills: if section == 6 {
            crate::skills::discover(&state.repo_root)
        } else {
            Vec::new()
        },
    });
}

pub fn open_selector(state: &mut AgentState, provider: bool) {
    let model = state.ui.presentation_model.clone().unwrap_or_default();
    open(state, 2, Some(&model));
    let panel = state.ui.panel.as_mut().unwrap();
    panel.picker = Some(if provider { 0 } else { 1 });
    panel.load_models = !provider;
    if !provider {
        panel.models.push(model.model);
        panel.notice = "Loading models from your endpoint…".into();
    }
}
pub fn open_team_message(state: &mut AgentState, id: i64) {
    let entry = state
        .logs
        .iter()
        .filter(|line| line.kind == crate::state::LogKind::Peer)
        .filter_map(|log| crate::team::transcript_message(&log.text))
        .find(|entry| entry.message.id == id);
    open(state, 11, None);
    let width = state.ui.input_wrap_width.saturating_sub(12).clamp(1, 96);
    if let (Some(entry), Some(panel)) = (entry, state.ui.panel.as_mut()) {
        let saved = crate::team::for_root(&state.repo_root)
            .and_then(|peer| peer.history_page(None, id.checked_add(1)).ok())
            .and_then(|history| history.into_iter().find(|item| item["message"]["id"] == id));
        let entry = saved.unwrap_or_else(|| serde_json::json!({"message":entry.message}));
        team_preview(panel, &entry, &state.repo_root, width);
    }
}
fn team_preview(
    panel: &mut Panel,
    entry: &serde_json::Value,
    workspace: &std::path::Path,
    width: usize,
) {
    let message = &entry["message"];
    let from = message["sender_title"].as_str().unwrap_or("Agent");
    let sender = message["sender"].as_str().unwrap_or("unknown");
    let recipient = message["recipient"].as_str().unwrap_or("All agents");
    let to = entry["recipient_title"].as_str().unwrap_or(recipient);
    let sent = team_time(message["at"].as_i64());
    let mut detail = format!("From  {from}\nSender session  {sender}\nTo  {to}\nRecipient session  {recipient}\nSent  {sent}\nWorkspace  {}\nMessage  #{}\nStatus  {}", workspace.display(), message["id"], team_status(entry));
    if let Some(receipts) = entry["receipts"].as_array() {
        for receipt in receipts {
            detail.push_str(&format!(
                "\nRead by  {} · {}",
                receipt[0].as_str().unwrap_or("unknown"),
                team_time(receipt[1].as_i64())
            ));
        }
    }
    detail.push_str("\n\n");
    detail.push_str(message["body"].as_str().unwrap_or(""));
    show_detail(panel, &detail, width);
}
fn show_detail(panel: &mut Panel, detail: &str, width: usize) {
    let body = detail
        .replace('\t', "    ")
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect::<String>();
    panel.preview_lines = super::tui::wrap_visual_lines(&body, width);
    panel.preview_offset = 0;
    panel.message_preview = true;
}
fn health_scan(
    root: std::path::PathBuf,
    deep: bool,
    fix: bool,
) -> std::sync::mpsc::Receiver<crate::diagnostics::Event> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let report = crate::diagnostics::scan_with_progress(&root, deep, fix, Some(tx.clone()));
        let _ = tx.send(crate::diagnostics::Event::Finished(report));
    });
    rx
}
fn team_time(at: Option<i64>) -> String {
    at.and_then(chrono::DateTime::from_timestamp_millis)
        .map(|time| time.format("%d %b %Y %H:%M:%S UTC").to_string())
        .unwrap_or_default()
}
fn team_status(entry: &serde_json::Value) -> &'static str {
    if entry["receipts"]
        .as_array()
        .is_some_and(|receipts| !receipts.is_empty())
    {
        "Read"
    } else {
        "Sent"
    }
}

pub fn choose_at(state: &mut AgentState, index: usize, label: &str) -> bool {
    if index == 0
        && label == "Show less"
        && state
            .ui
            .panel
            .as_ref()
            .is_some_and(|panel| panel.message_preview)
    {
        state.ui.panel.as_mut().unwrap().message_preview = false;
        return true;
    }
    let Some(panel) = state.ui.panel.as_ref() else {
        return false;
    };
    if (panel.picker.is_none() && panel.section != 10 && !panel.settings)
        || panel.edit.is_some()
        || panel.capture.is_some()
        || entries(state, panel)
            .get(index)
            .is_none_or(|row| row.id.as_deref().unwrap_or(&row.label) != label)
    {
        return false;
    }
    let center = panel.section == 10;
    state.ui.panel.as_mut().unwrap().selected = index;
    state.ui.panel.as_mut().unwrap().selected_session =
        if center { Some(label.into()) } else { None };
    handle_key(state, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    true
}

pub fn poll(state: &mut AgentState, agent: &crate::agent::Agent) -> bool {
    let Some(panel) = state.ui.panel.as_mut() else {
        return false;
    };
    if panel.load_health {
        panel.load_health = false;
        panel.health_started = std::time::Instant::now();
        panel.notice = "Checking local installation…".into();
        panel.health_scan = Some(health_scan(state.repo_root.clone(), false, false));
        return true;
    }
    if let Some(receiver) = &panel.health_scan {
        let mut changed = false;
        for _ in 0..64 {
            match receiver.try_recv() {
                Ok(crate::diagnostics::Event::Checking(title)) => {
                    panel.notice = format!("Checking {title}…");
                    changed = true;
                }
                Ok(crate::diagnostics::Event::Checked(check)) => {
                    panel.health_checks.push(check);
                    changed = true;
                }
                Ok(crate::diagnostics::Event::Finished(report)) => {
                    panel.health = Some(report);
                    panel.health_checks.clear();
                    panel.health_scan = None;
                    panel.notice.clear();
                    return true;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panel.health_scan = None;
                    panel.notice = "Health check stopped unexpectedly. Choose Scan again.".into();
                    return true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
            }
        }
        if changed {
            return true;
        }
    }
    if panel.section == 11
        && (panel.load_team
            || (!panel.message_preview
                && panel.team_before.is_none()
                && panel.preview_refreshed.elapsed() >= std::time::Duration::from_secs(1)))
    {
        panel.load_team = false;
        panel.preview_refreshed = std::time::Instant::now();
        match crate::team::for_root(&state.repo_root)
            .ok_or("Team runtime unavailable".to_string())
            .and_then(|peer| peer.history_page(None, panel.team_before))
        {
            Ok(history) => {
                panel.team_history = history;
                panel.notice.clear();
            }
            Err(error) => panel.notice = error,
        }
        return true;
    }
    if let Some(id) = panel.preview_session.as_deref() {
        if panel.preview_refreshed.elapsed() >= std::time::Duration::from_secs(1) {
            panel.preview_refreshed = std::time::Instant::now();
            match persistence::preview_session(id) {
                Ok(lines) => panel.preview_lines = lines,
                Err(e) => panel.notice = e,
            }
            return true;
        }
    }
    if let Some((id, last)) = panel.pending_receipt.as_mut() {
        if last.elapsed() >= std::time::Duration::from_secs(1) {
            *last = std::time::Instant::now();
            if let Some(peer) = crate::team::for_root(&state.repo_root) {
                if let Ok(receipts) = peer.receipts(*id) {
                    if !receipts.is_empty() {
                        panel.notice=format!("Message #{id} acknowledged by the recipient · accepted into durable context");
                        panel.pending_receipt = None;
                        return true;
                    }
                }
            }
        }
    }
    if panel.load_sessions {
        panel.load_sessions = false;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::team::Store::open().and_then(|s| s.import_sessions()));
        });
        panel.import_sessions = Some(rx);
        panel.notice = "Loading saved sessions…".into();
    }
    if let Some(result) = panel
        .import_sessions
        .as_ref()
        .and_then(|rx| rx.try_recv().ok())
    {
        panel.import_sessions = None;
        match result {
            Ok(()) => {
                panel.notice="Live sessions keep their owner. Open an inactive session, or fork its history.".into();
                match crate::team::Store::open().and_then(|s| s.peers(None)) {
                    Ok(peers) => state.ui.team.peers = peers,
                    Err(e) => panel.notice = e,
                }
            }
            Err(e) => panel.notice = e,
        }
        return true;
    }
    if panel.load_models {
        panel.load_models = false;
        panel.catalog = Some(agent.model_catalog(panel.model.clone()));
    }
    let result = panel.catalog.as_ref().and_then(|rx| rx.try_recv().ok());
    if let Some(result) = result {
        panel.catalog = None;
        match result {
            Ok(models) => {
                panel.models = models;
                panel.notice = "Endpoint catalog · choose a model, or enter a custom ID".into();
            }
            Err(error) => panel.notice = format!("{error} · custom model IDs remain available"),
        }
        return true;
    }
    false
}

fn session_rows(state: &AgentState, panel: &Panel, filter: bool) -> Vec<Row> {
    let mut peers = state
        .ui
        .team
        .peers
        .iter()
        .filter(|p| {
            !filter
                || match panel.filter {
                    1 => p.online && matches!(p.status.as_str(), "needs_you" | "error"),
                    2 => p.online && p.status == "working",
                    3 => p.online && p.status == "ready",
                    4 => !p.online,
                    _ => true,
                }
        })
        .collect::<Vec<_>>();
    peers.sort_by(|a, b| {
        b.updated
            .cmp(&a.updated)
            .then(a.workspace.cmp(&b.workspace))
            .then(b.pinned.cmp(&a.pinned))
            .then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            .then(a.session.cmp(&b.session))
    });
    peers
        .into_iter()
        .map(|p| {
            let tag = if !p.online {
                "inactive"
            } else {
                p.status.as_str()
            };
            let age = (chrono::Utc::now().timestamp_millis() - p.updated).max(0) / 1000;
            let ago = if age < 2 {
                "now".into()
            } else if age < 60 {
                format!("{age}s")
            } else if age < 3600 {
                format!("{}m", age / 60)
            } else if age < 86400 {
                format!("{}h", age / 3600)
            } else {
                format!("{}d", age / 86400)
            };
            let mut r = row(
                format!("{}{}", if p.pinned { "● " } else { "" }, p.title),
                format!("{} · {} · {}", p.workspace, p.model, p.session),
                format!("{} · {ago}", tag.replace('_', " ")),
                Action::Session {
                    id: p.session.clone(),
                    root: p.workspace.clone().into(),
                    online: p.online,
                },
            );
            r.id = Some(p.session.clone());
            r.group = Some(p.workspace.clone());
            r.tag = Some(tag.into());
            r
        })
        .collect()
}
fn selected_index(panel: &Panel, rows: &[Row]) -> usize {
    panel
        .selected_session
        .as_ref()
        .and_then(|id| rows.iter().position(|r| r.id.as_ref() == Some(id)))
        .unwrap_or(panel.selected.min(rows.len().saturating_sub(1)))
}
fn request_session(
    state: &mut AgentState,
    panel: &mut Panel,
    id: Option<String>,
    root: std::path::PathBuf,
    fork: bool,
) -> bool {
    if state.ui.agent_running
        || state
            .jobs
            .iter()
            .any(|j| matches!(j.status, crate::state::JobStatus::Running))
        || state.ui.editor_session.is_some()
    {
        panel.notice =
            "Finish or cancel active work and close Neovim before switching sessions.".into();
        return false;
    }
    state.ui.open_session = Some(crate::state::SessionTarget { id, root, fork });
    true
}

fn entries(state: &AgentState, panel: &Panel) -> Vec<Row> {
    let check = |b| if b { "on" } else { "off" };
    if let Some(field) = panel.picker {
        let mut choices = if field == 0 {
            let mut providers = vec![
                "openai".to_string(),
                "groq".into(),
                "mistral".into(),
                "ollama".into(),
                "anthropic".into(),
                "openrouter".into(),
                "deepseek".into(),
                "xai".into(),
                "lmstudio".into(),
            ];
            if !providers.contains(&panel.model.provider) {
                providers.insert(0, panel.model.provider.clone());
            }
            providers
                .into_iter()
                .map(|provider| {
                    row(
                        &provider,
                        "Choose provider, then choose its model",
                        if provider == panel.model.provider {
                            "●"
                        } else {
                            ""
                        },
                        Action::ChooseProvider(provider.clone()),
                    )
                })
                .collect::<Vec<_>>()
        } else {
            panel
                .models
                .iter()
                .map(|model| {
                    row(
                        model,
                        "Listed by this endpoint",
                        if model == &panel.model.model {
                            "●"
                        } else {
                            ""
                        },
                        Action::ChooseModel(model.clone()),
                    )
                })
                .collect::<Vec<_>>()
        };
        let query = panel.query.to_lowercase();
        choices.retain(|r| r.label.to_lowercase().contains(&query));
        choices.push(row(
            if field == 0 {
                "Custom provider…"
            } else {
                "Custom model…"
            },
            if panel.query.is_empty() {
                "Enter an identifier".into()
            } else {
                format!("Enter {}", panel.query)
            },
            "↵",
            Action::CustomModel(field),
        ));
        return choices;
    }
    let mut rows = match panel.section {
        12 => [
            (1, "Appearance", "Theme, paper, accent and motion"),
            (2, "Model & provider", "Models, reasoning and connection"),
            (3, "Permissions", "Choose what tools may do"),
            (4, "Status line", "Choose and arrange footer items"),
            (5, "Keyboard", "Find and change shortcuts"),
            (6, "Skills", "Browse available agent skills"),
            (7, "Usage", "Tokens and context for this session"),
            (8, "Activity", "Weekly usage and recorded history"),
            (13, "Workspace", "Editor, virtual Mac and iPhone panes"),
            (11, "Team", "Saved messages, sessions and read receipts"),
            (
                14,
                "Doctor",
                "Health checks, diagnostics and focused repairs",
            ),
        ]
        .into_iter()
        .map(|(section, label, description)| row(label, description, "→", Action::Section(section)))
        .collect(),
        14 => {
            let mut rows = vec![
                row(
                    "Scan again",
                    "Check local installation and storage",
                    "Run",
                    Action::HealthScan(false),
                ),
                row(
                    "Deep diagnostics",
                    "Check saved snapshots, mailbox integrity and provider connectivity",
                    "Run",
                    Action::HealthScan(true),
                ),
            ];
            if panel.health_scan.is_some() || panel.load_health {
                rows.push(row(
                    &panel.notice,
                    &format!("{} checks finished", panel.health_checks.len()),
                    "Checking",
                    Action::None,
                ));
                rows.last_mut().unwrap().tag = Some("checking".into());
            } else if let Some(report) = &panel.health {
                if report.checks.iter().any(|check| check.repairable) {
                    rows.push(row(
                        "Repair private storage",
                        "Create missing directories and secure private file permissions",
                        "Repair",
                        Action::HealthRepair,
                    ));
                }
                rows.push(row(
                    "Ask agent to investigate",
                    "Send this report into the current conversation under your permission policy",
                    "Ask",
                    Action::HealthAsk,
                ));
                rows.push(row(
                    "Full diagnostic report",
                    "Read all details and suggested next steps",
                    "Show more",
                    Action::HealthDetails,
                ));
            }
            let checks = panel
                .health
                .as_ref()
                .map(|report| &report.checks)
                .unwrap_or(&panel.health_checks);
            for check in checks
                .iter()
                .filter(|check| !check.id.starts_with("snapshot-"))
            {
                let mut item = row(
                    &check.title,
                    &check.detail,
                    check.status.label(),
                    Action::HealthDetails,
                );
                item.tag = Some(check.status.label().to_ascii_lowercase());
                rows.push(item);
            }
            rows
        }
        13 => vec![
            row(
                "Code editor",
                "Follow the agent’s active file and saved edits",
                "Open",
                Action::Command("/nv".into()),
            ),
            row(
                "Virtual Mac",
                "Create or reuse a Mac automatically; downloads macOS if needed",
                "Open",
                Action::Command("/workspace mac".into()),
            ),
            row(
                "iPhone",
                "Create or reuse a simulator automatically; downloads iOS if needed",
                "Open",
                Action::Command("/workspace phone".into()),
            ),
            row(
                "Download Mac companion",
                "Optional small download; does not install a macOS image",
                "Download",
                Action::Command("/workspace install mac".into()),
            ),
            row(
                "Download iPhone companion",
                "Optional small download; does not install Xcode or an iOS runtime",
                "Download",
                Action::Command("/workspace install phone".into()),
            ),
        ],
        10 => session_rows(state, panel, true),
        11 => {
            let mut rows = vec![];
            if panel.team_before.is_some() {
                rows.push(row(
                    "Newest messages",
                    "Return to live history",
                    "↵",
                    Action::TeamPage(None),
                ));
            }
            for entry in panel.team_history.iter().rev() {
                let message = &entry["message"];
                let outgoing = message["sender"].as_str() == Some(&state.session.id);
                let name = if outgoing {
                    message["recipient"]
                        .as_str()
                        .map(|id| {
                            let label = state
                                .ui
                                .team
                                .peers
                                .iter()
                                .find(|peer| peer.session == id)
                                .map(crate::team::presence_name)
                                .or_else(|| entry["recipient_title"].as_str().map(str::to_owned))
                                .unwrap_or_else(|| id.chars().take(8).collect());
                            format!("to {label}")
                        })
                        .unwrap_or_else(|| "to all agents".into())
                } else {
                    let title = message["sender_title"].as_str().unwrap_or("Agent");
                    if title.starts_with('/') {
                        "Agent".into()
                    } else {
                        title.into()
                    }
                };
                let time = message["at"]
                    .as_i64()
                    .and_then(chrono::DateTime::from_timestamp_millis)
                    .map(|time| time.format("%d %b %H:%M").to_string())
                    .unwrap_or_default();
                rows.push(row(
                    format!("{} {}", if outgoing { "↗" } else { "↙" }, name),
                    message["body"]
                        .as_str()
                        .unwrap_or("")
                        .lines()
                        .next()
                        .unwrap_or(""),
                    format!("{time} · {}", team_status(entry)),
                    Action::TeamMessage(message["id"].as_i64().unwrap_or_default()),
                ));
            }
            if panel.team_history.len() == 50 {
                let before = panel
                    .team_history
                    .first()
                    .and_then(|entry| entry["message"]["id"].as_i64());
                rows.push(row(
                    "Older messages",
                    "History stays in the local mailbox",
                    "↵",
                    Action::TeamPage(before),
                ));
            }
            if rows.is_empty() {
                rows.push(row(
                    "No messages yet",
                    "Ask your agent to contact another open session",
                    "",
                    Action::None,
                ));
            }
            rows
        }
        0 => commands::command_palette_items()
            .iter()
            .map(|c| row(c.cmd, c.desc, "↵", Action::Command(c.cmd.into())))
            .collect(),
        1 => {
            let mut rows = vec![
                row(
                    "Dark",
                    "Black canvas · Graphite paper composer",
                    if state.theme == UiTheme::Dark {
                        "●"
                    } else {
                        ""
                    },
                    Action::Theme("dark"),
                ),
                row(
                    "Light",
                    "Light canvas · Cotton paper composer",
                    if state.theme == UiTheme::Light {
                        "●"
                    } else {
                        ""
                    },
                    Action::Theme("light"),
                ),
                row(
                    "Reduce motion",
                    "Settle artwork immediately",
                    check(state.preferences.reduce_motion),
                    Action::Motion,
                ),
                row(
                    "Ambient artwork",
                    "Quiet central artwork on an empty conversation",
                    check(state.preferences.ambient_art),
                    Action::Art,
                ),
            ];
            for name in ["orange", "blue", "green", "violet", "rose"] {
                rows.push(row(
                    format!("Accent · {name}"),
                    "Workspace highlight color",
                    if state.accent.as_str() == name {
                        "●"
                    } else {
                        ""
                    },
                    Action::Accent(name),
                ));
            }
            for name in ["compact", "standard", "spacious"] {
                rows.push(row(
                    format!("Density · {name}"),
                    "Header and transcript spacing",
                    if state.density.as_str() == name {
                        "●"
                    } else {
                        ""
                    },
                    Action::Density(name),
                ));
            }
            rows
        }
        2 => vec![
            row(
                "Provider",
                "Provider-specific APIs; credentials stay in Rust",
                &panel.model.provider,
                Action::EditModel(0),
            ),
            row(
                "Model",
                "Enter the model identifier supported by your endpoint",
                &panel.model.model,
                Action::EditModel(1),
            ),
            row(
                "Endpoint",
                "Optional http(s) URL; empty uses the provider default",
                panel.model.base_url.as_deref().unwrap_or("default"),
                Action::EditModel(2),
            ),
            row(
                "Apply model",
                "Save through the existing model configuration path",
                "↵",
                Action::ApplyModel,
            ),
            row(
                "API key",
                "Open the masked credential input",
                "↵",
                Action::Command("/key".into()),
            ),
            row(
                "Migrate saved keys",
                "Move legacy plaintext credentials into the OS credential store",
                "↵",
                Action::Command("/key migrate".into()),
            ),
            row(
                "API protocol",
                "Auto follows provider; Enter cycles for custom endpoints",
                panel.model.api.map(|a| a.name()).unwrap_or("auto"),
                Action::CycleApi,
            ),
            row(
                "Reasoning effort",
                "Per-model override; choose an effort your provider supports",
                crate::model_options::get(&panel.model)["reasoning"]
                    .as_str()
                    .unwrap_or("auto"),
                Action::Reasoning,
            ),
            row(
                "Context window",
                "Configured estimate; set per-model context_window in config.toml",
                format!(
                    "{} tokens",
                    crate::model_options::limits(&panel.model).window
                ),
                Action::None,
            ),
            row(
                "Output reserve",
                "Reserved before context compaction",
                format!(
                    "{} tokens",
                    crate::model_options::limits(&panel.model).output
                ),
                Action::None,
            ),
        ],
        3 => vec![
            row(
                "Read only",
                "Block tools classified as dangerous",
                selected_profile(state, PermissionProfile::ReadOnly),
                Action::Profile(PermissionProfile::ReadOnly),
            ),
            row(
                "Ask for approval",
                "Prompt before dangerous tools unless auto-approval is enabled",
                selected_profile(state, PermissionProfile::WorkspaceAuto),
                Action::Profile(PermissionProfile::WorkspaceAuto),
            ),
            row(
                "Full access",
                "Allow dangerous tools without prompting",
                selected_profile(state, PermissionProfile::FullAccess),
                Action::Profile(PermissionProfile::FullAccess),
            ),
            row(
                "Auto-approve dangerous tools",
                "Session only · explicitly bypass dangerous-tool prompts",
                check(state.ui.auto_approve),
                Action::Approval,
            ),
            row(
                "OS command sandbox",
                "Separate from approvals · network denied unless explicitly configured",
                crate::sandbox::mode(),
                Action::Sandbox,
            ),
            row(
                "Scoped grants",
                "User config supports literal command prefixes and workspace write paths",
                "config.toml · permissions",
                Action::None,
            ),
        ],
        4 => {
            let mut ids = state.preferences.status_fields.clone();
            ids.extend(
                STATUS_FIELDS
                    .iter()
                    .filter(|(id, _, _)| !state.preferences.status_fields.iter().any(|s| s == id))
                    .map(|(id, _, _)| id.to_string()),
            );
            ids.iter()
                .filter_map(|id| STATUS_FIELDS.iter().find(|(key, _, _)| key == id))
                .map(|(id, label, desc)| {
                    row(
                        *label,
                        *desc,
                        if state.preferences.status_fields.iter().any(|s| s == id) {
                            "[×]"
                        } else {
                            "[ ]"
                        },
                        Action::Status(id.to_string()),
                    )
                })
                .collect()
        }
        5 => super::keymap::BINDINGS
            .iter()
            .map(|b| {
                row(
                    b.label,
                    format!(
                        "{} · default {}{}",
                        b.group,
                        b.default,
                        if b.id == "copy" { " · protected" } else { "" }
                    ),
                    format!(
                        "{}{}",
                        super::keymap::effective(&state.preferences, b),
                        if state.preferences.key_bindings.contains_key(b.id) {
                            " *"
                        } else {
                            ""
                        }
                    ),
                    Action::KeyBinding(b.id.into()),
                )
            })
            .collect(),
        6 => panel
            .skills
            .iter()
            .map(|skill| {
                row(
                    format!("${}", skill.name),
                    format!(
                        "{} · {} · {}",
                        skill.description,
                        skill.source,
                        skill.path.display()
                    ),
                    if state.preferences.disabled_skills.contains(&skill.name) {
                        "disabled"
                    } else {
                        "enabled"
                    },
                    Action::Skill(skill.name.clone()),
                )
            })
            .collect(),
        7 => {
            let sum = |f: fn(&crate::usage::Day) -> u64| panel.days.iter().map(f).sum::<u64>();
            vec![
                row(
                    "Reported input",
                    "Provider counts · last 365 UTC days · this workspace",
                    sum(|d| d.input).to_string(),
                    Action::None,
                ),
                row(
                    "Reported output",
                    "Includes reasoning tokens when reported by the provider",
                    sum(|d| d.output).to_string(),
                    Action::None,
                ),
                row(
                    "Cached input",
                    "Subset of reported input; never added twice",
                    sum(|d| d.cached).to_string(),
                    Action::None,
                ),
                row(
                    "Model calls",
                    "Successful responses that included valid usage metadata",
                    sum(|d| d.calls).to_string(),
                    Action::None,
                ),
                row(
                    "Provider wait",
                    "Includes retries; not total agent runtime",
                    super::helper::elapsed_text(sum(|d| d.duration_ms), true),
                    Action::None,
                ),
                row(
                    "Legacy text estimate",
                    "Separate from provider counts; not billing data",
                    format!(
                        "~{} tokens",
                        state.usage.prompt_tokens + state.usage.completion_tokens
                    ),
                    Action::None,
                ),
                row(
                    "Context estimate",
                    "Current conversation estimate",
                    format!("~{} tokens", state.conversation.token_estimate),
                    Action::None,
                ),
                row(
                    "Pricing and quotas",
                    "Not reported by the provider; no invented prices or account limits",
                    "unavailable",
                    Action::None,
                ),
            ]
        }
        8 => {
            let mut models = std::collections::BTreeMap::<String, (u64, u64)>::new();
            for day in &panel.days {
                let total = models
                    .entry(format!("{} · {}", day.model, day.provider))
                    .or_default();
                total.0 += day.input + day.output;
                total.1 += day.calls;
            }
            let mut models: Vec<_> = models.into_iter().collect();
            models.sort_by_key(|(_, v)| std::cmp::Reverse(v.0));
            models.into_iter().map(|(model,(tokens,calls))|row(model,format!("{calls} reported model calls · input + output; cached input is a subset"),format!("{tokens} tokens"),Action::None)).collect()
        }
        9 => vec![
            row(
                "Workspace",
                state.repo_root.display().to_string(),
                state.ui.repo_branch.as_deref().unwrap_or("no branch"),
                Action::None,
            ),
            row(
                "Run",
                state.ui.current_tool.as_deref().unwrap_or("No active tool"),
                &state.ui.run_phase,
                Action::None,
            ),
            row(
                "Session",
                state
                    .session_name
                    .as_deref()
                    .unwrap_or("Unnamed conversation"),
                format!("{} messages", state.conversation.messages.len()),
                Action::None,
            ),
            row(
                "Background work",
                "Existing queued and running jobs",
                state.jobs.len().to_string(),
                Action::None,
            ),
            row(
                "Verification",
                "Inspect the execution evidence ledger",
                "↵",
                Action::Command("/verify".into()),
            ),
            row(
                "Connected tools",
                "Inspect MCP server configuration",
                "↵",
                Action::Command("/mcp".into()),
            ),
        ],
        _ => Vec::new(),
    };
    let query = panel.query.to_lowercase();
    rows.retain(|r| {
        format!("{} {} {}", r.label, r.description, r.value)
            .to_lowercase()
            .contains(&query)
    });
    rows
}
fn selected_profile(state: &AgentState, profile: PermissionProfile) -> &'static str {
    if state.permission_profile == profile {
        "●"
    } else {
        ""
    }
}

pub fn view(state: &AgentState) -> Option<PanelView> {
    let panel = state.ui.panel.as_ref()?;
    let rows = entries(state, panel);
    Some(PanelView {
        settings: panel.settings,
        previewing: panel.preview_session.is_some() || panel.message_preview,
        preview_offset: panel.preview_offset,
        center_filter: panel.filter,
        counts: (0..5)
            .map(|filter| {
                state
                    .ui
                    .team
                    .peers
                    .iter()
                    .filter(|p| match filter {
                        1 => p.online && matches!(p.status.as_str(), "needs_you" | "error"),
                        2 => p.online && p.status == "working",
                        3 => p.online && p.status == "ready",
                        4 => !p.online,
                        _ => true,
                    })
                    .count()
            })
            .collect(),
        selecting: (panel.picker.is_some()
            || matches!(panel.section, 10 | 11 | 14)
            || panel.settings)
            && panel.edit.is_none()
            && panel.capture.is_none(),
        title: match panel.picker {
            Some(0) => "Choose provider".into(),
            Some(_) => format!("Choose model · {}", panel.model.provider),
            None => SECTIONS[panel.section].into(),
        },
        elapsed: panel.opened.elapsed().as_millis() as u64,
        health_running: panel.section == 14 && (panel.load_health || panel.health_scan.is_some()),
        sections: SECTIONS.to_vec(),
        section: panel.section,
        selected: selected_index(panel, &rows),
        rows,
        query: if panel.capture.is_some() {
            "Press shortcut…".into()
        } else {
            panel
                .edit
                .as_ref()
                .map(|(_, text)| text.clone())
                .unwrap_or_else(|| panel.query.clone())
        },
        notice: if panel.section == 14 && !panel.message_preview {
            if panel.health_scan.is_some() || panel.load_health {
                format!(
                    "{} · {} · {} checks finished",
                    panel.notice,
                    super::helper::elapsed_text(
                        panel.health_started.elapsed().as_millis() as u64,
                        false
                    ),
                    panel.health_checks.len()
                )
            } else if let Some(report) = &panel.health {
                let warnings = report
                    .checks
                    .iter()
                    .filter(|check| check.status == crate::diagnostics::Status::Warn)
                    .count();
                let failures = report
                    .checks
                    .iter()
                    .filter(|check| check.status == crate::diagnostics::Status::Fail)
                    .count();
                format!(
                    "Scan complete · {} checks · {warnings} warnings · {failures} failures · {}",
                    report.checks.len(),
                    super::helper::elapsed_text(report.duration_ms, true)
                )
            } else {
                panel.notice.clone()
            }
        } else if panel.notice.is_empty() {
            "Workspace settings · saved locally · draft preserved".into()
        } else {
            panel.notice.clone()
        },
        editing: panel.edit.is_some() || panel.capture.is_some(),
        footer: if panel.message_preview && panel.section == 14 {
            "enter / esc Show less · ↑↓ read report"
        } else if panel.message_preview {
            "enter / esc Show less · ↑↓ read message"
        } else if panel.section == 12 {
            "↑↓ choose category · enter open · click to open · esc close"
        } else if panel.settings
            && panel.edit.is_none()
            && panel.capture.is_none()
            && panel.picker.is_none()
        {
            "↑↓ navigate · enter select · esc categories"
        } else if panel.section == 10 {
            "←/→ filter · enter open · ctrl+n new · F3 fork · F4 message · F5 pin · esc back"
        } else if panel.capture.is_some() {
            "press new chord · backspace default · delete unbind · esc cancel"
        } else if panel.picker.is_some() && panel.edit.is_none() {
            "type to filter · ↑↓ navigate · enter choose · esc back"
        } else if panel.section == 5 {
            "enter rebind · * customized · tab section · esc close"
        } else if panel.section == 8 {
            "d sample / recorded · tab section · esc close"
        } else if panel.section == 6 {
            "enter insert · space enable/disable · tab section · esc close"
        } else if panel.edit.is_some() {
            "enter confirm · esc cancel edit"
        } else if panel.section == 4 {
            "space toggle · ←/→ reorder · tab section · esc close"
        } else {
            "↑↓ navigate · enter select · tab section · esc close"
        }
        .into(),
        preview: if panel.preview_session.is_some() || panel.message_preview {
            panel.preview_lines.clone()
        } else {
            statusline(state)
        },
        history: panel.days.clone(),
        demo: panel.demo,
    })
}

pub fn handle_key(state: &mut AgentState, key: KeyEvent) -> bool {
    let Some(mut panel) = state.ui.panel.take() else {
        return false;
    };
    if (panel.preview_session.is_some() || panel.message_preview) && panel.edit.is_none() {
        match key.code {
            KeyCode::Enter if panel.message_preview => {
                panel.message_preview = false;
                state.ui.panel = Some(panel);
                return true;
            }
            KeyCode::Esc => {
                panel.preview_session = None;
                panel.message_preview = false;
                state.ui.panel = Some(panel);
                return true;
            }
            KeyCode::Up | KeyCode::PageUp => {
                if panel.message_preview {
                    panel.preview_offset = panel
                        .preview_offset
                        .saturating_sub(if key.code == KeyCode::Up { 1 } else { 8 });
                    state.ui.panel = Some(panel);
                    return true;
                }
                panel.preview_offset = panel
                    .preview_offset
                    .saturating_add(if key.code == KeyCode::Up { 1 } else { 8 })
                    .min(panel.preview_lines.len());
                state.ui.panel = Some(panel);
                return true;
            }
            KeyCode::Down | KeyCode::PageDown => {
                if panel.message_preview {
                    panel.preview_offset = panel
                        .preview_offset
                        .saturating_add(if key.code == KeyCode::Down { 1 } else { 8 })
                        .min(panel.preview_lines.len());
                    state.ui.panel = Some(panel);
                    return true;
                }
                panel.preview_offset = panel
                    .preview_offset
                    .min(panel.preview_lines.len())
                    .saturating_sub(if key.code == KeyCode::Down { 1 } else { 8 });
                state.ui.panel = Some(panel);
                return true;
            }
            KeyCode::Home => {
                panel.preview_offset = if panel.message_preview { 0 } else { usize::MAX };
                state.ui.panel = Some(panel);
                return true;
            }
            KeyCode::End => {
                panel.preview_offset = if panel.message_preview { usize::MAX } else { 0 };
                state.ui.panel = Some(panel);
                return true;
            }
            KeyCode::F(3) | KeyCode::F(4) => {}
            _ => {
                state.ui.panel = Some(panel);
                return true;
            }
        }
    }
    if let Some(id) = panel.capture.take() {
        let binding = super::keymap::BINDINGS.iter().find(|b| b.id == id).unwrap();
        let value = match key.code {
            KeyCode::Esc => None,
            KeyCode::Backspace => Some(binding.default.to_string()),
            KeyCode::Delete => Some("unbound".into()),
            _ => super::keymap::chord(key),
        };
        if let Some(value) = value {
            panel.notice = match super::keymap::assign(&mut state.preferences, &id, &value) {
                Ok(()) => match persistence::save(state) {
                    Ok(()) => "Shortcut saved".into(),
                    Err(e) => format!("Not saved: {e}"),
                },
                Err(e) => e,
            };
        } else if key.code != KeyCode::Esc {
            panel.capture = Some(id);
            panel.notice = "Use a function key or Ctrl/Alt/Command chord".into();
        }
        state.ui.panel = Some(panel);
        return true;
    }
    if let Some((field, text)) = panel.edit.as_mut() {
        match key.code {
            KeyCode::Esc => {
                panel.edit = None;
                panel.message_to = None;
            }
            KeyCode::Enter => {
                let text = text.trim().to_string();
                if let Some((recipient, nonce)) = panel.message_to.clone() {
                    let result = crate::team::for_root(&state.repo_root)
                        .ok_or("Team runtime is unavailable".to_string())
                        .and_then(|mut peer| {
                            peer.enabled = true;
                            peer.send(Some(&recipient), &text, &nonce)
                        });
                    match result {
                        Ok(id) => {
                            panel.pending_receipt = Some((id, std::time::Instant::now()));
                            panel.edit = None;
                            panel.message_to = None;
                            panel.notice = format!(
                                "Message #{id} queued durably · waiting for acknowledgement"
                            );
                        }
                        Err(e) => panel.notice = e,
                    }
                    state.ui.panel = Some(panel);
                    return true;
                }
                let field = *field;
                if field < 2 && (text.is_empty() || text.chars().any(char::is_whitespace)) {
                    panel.notice = "Use a nonempty provider/model identifier without spaces".into();
                } else if field == 2
                    && !text.is_empty()
                    && (!(text.starts_with("https://") || text.starts_with("http://"))
                        || text.contains('@')
                        || text.chars().any(char::is_whitespace))
                {
                    panel.notice =
                        "Use an http(s) endpoint without embedded credentials or whitespace".into();
                } else {
                    match field {
                        0 => panel.model.provider = text,
                        1 => panel.model.model = text,
                        _ => panel.model.base_url = if text.is_empty() { None } else { Some(text) },
                    };
                    panel.edit = None;
                    panel.notice = "Model draft updated · choose Apply model to save".into();
                }
            }
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Char('u' | 'a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                text.clear()
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                text.push(c)
            }
            _ => {}
        }
        state.ui.panel = Some(panel);
        return true;
    }
    let rows = entries(state, &panel);
    let index = selected_index(&panel, &rows);
    panel.selected = index;
    let selected_before = rows.get(index).and_then(|row| row.id.clone());
    let mut close = false;
    let mut changed = false;
    match key.code {
        KeyCode::Esc => {
            if panel.picker.take().is_some() {
                panel.query.clear();
                panel.selected = 0;
            } else if panel.settings && panel.section != 12 {
                open(state, 12, Some(&panel.model));
                return true;
            } else {
                close = true
            }
        }
        KeyCode::Left | KeyCode::Right if panel.section == 10 => {
            panel.filter = (panel.filter + 5 + if key.code == KeyCode::Left { 4 } else { 1 }) % 5;
            panel.selected = 0;
            panel.selected_session = None;
        }
        KeyCode::F(1) if panel.section == 10 => {
            if let Some(id) = rows.get(index).and_then(|r| r.id.as_ref()) {
                panel.selected_session = Some(id.clone());
                panel.preview_session = Some(id.clone());
                panel.preview_offset = 0;
                match persistence::preview_session(id) {
                    Ok(lines) => panel.preview_lines = lines,
                    Err(e) => panel.notice = e,
                };
            }
        }
        KeyCode::Char('n' | 'N')
            if panel.section == 10 && key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            let root = rows
                .get(index)
                .and_then(|r| {
                    if let Action::Session { root, .. } = &r.action {
                        Some(root.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| state.repo_root.clone());
            close = request_session(state, &mut panel, None, root, false);
        }
        KeyCode::F(3) if panel.section == 10 => {
            if let Some(Row {
                action: Action::Session { id, root, .. },
                ..
            }) = rows.get(index)
            {
                close = request_session(state, &mut panel, Some(id.clone()), root.clone(), true);
            }
        }
        KeyCode::F(4) if panel.section == 10 => {
            if let Some(Row {
                action: Action::Session { id, .. },
                ..
            }) = rows.get(index)
            {
                panel.message_to = Some((id.clone(), uuid::Uuid::new_v4().to_string()));
                panel.edit = Some((100, String::new()));
                panel.notice =
                    "Message this collaborator. Enter sends; Escape preserves your original draft."
                        .into();
            }
        }
        KeyCode::F(5) if panel.section == 10 => {
            if let Some(id) = rows.get(index).and_then(|r| r.id.as_ref()) {
                panel.selected_session = Some(id.clone());
                let pinned = state
                    .ui
                    .team
                    .peers
                    .iter()
                    .find(|p| &p.session == id)
                    .is_some_and(|p| p.pinned);
                panel.notice = match crate::team::Store::open().and_then(|s| {
                    s.pin(id, !pinned)?;
                    state.ui.team.peers = s.peers(None)?;
                    Ok(())
                }) {
                    Ok(()) => "Pin saved".into(),
                    Err(e) => e,
                };
            }
        }
        KeyCode::Char('d' | 'D') if panel.section == 8 => {
            panel.demo = !panel.demo;
            panel.opened = std::time::Instant::now();
        }
        KeyCode::Tab | KeyCode::BackTab if !panel.settings => {
            let back = key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
            panel.selected_session = None;
            panel.load_sessions = panel.section == 10;
            panel.section =
                (panel.section + SECTIONS.len() + if back { SECTIONS.len() - 1 } else { 1 })
                    % SECTIONS.len();
            panel.load_sessions = panel.section == 10;
            panel.query.clear();
            panel.selected = 0;
            panel.notice.clear();
            panel.picker = None;
            panel.catalog = None;
            panel.load_models = false;
            panel.opened = std::time::Instant::now();
            if matches!(panel.section, 7 | 8) {
                let (days, notice) = load_usage(state, panel.section);
                panel.demo = panel.section == 8;
                panel.days = days;
                panel.notice = notice;
            }
            if panel.section == 6 {
                panel.skills = crate::skills::discover(&state.repo_root);
            }
        }
        KeyCode::Up => panel.selected = index.saturating_sub(1),
        KeyCode::Down => panel.selected = (index + 1).min(rows.len().saturating_sub(1)),
        KeyCode::Home => panel.selected = 0,
        KeyCode::End => panel.selected = rows.len().saturating_sub(1),
        KeyCode::PageDown => panel.selected = (index + 8).min(rows.len().saturating_sub(1)),
        KeyCode::PageUp => panel.selected = index.saturating_sub(8),
        KeyCode::Left | KeyCode::Right if panel.section == 4 => {
            if let Some(Row {
                action: Action::Status(id),
                ..
            }) = rows.get(index)
            {
                if let Some(at) = state.preferences.status_fields.iter().position(|s| s == id) {
                    let to = if key.code == KeyCode::Left {
                        at.saturating_sub(1)
                    } else {
                        (at + 1).min(state.preferences.status_fields.len() - 1)
                    };
                    state.preferences.status_fields.swap(at, to);
                    panel.selected = to;
                    changed = true;
                }
            }
        }
        KeyCode::Enter | KeyCode::Char(' ')
            if key.code == KeyCode::Enter || matches!(panel.section, 4 | 6) =>
        {
            if let Some(row) = rows.get(index) {
                match &row.action {
                    Action::HealthScan(deep) => {
                        if panel.health_scan.is_none() {
                            panel.load_health = false;
                            panel.health_deep = *deep;
                            panel.health = None;
                            panel.health_checks.clear();
                            panel.health_started = std::time::Instant::now();
                            panel.health_scan =
                                Some(health_scan(state.repo_root.clone(), *deep, false));
                            panel.notice = "Checking health…".into();
                        }
                    }
                    Action::HealthRepair => {
                        panel.health = None;
                        panel.health_checks.clear();
                        panel.health_started = std::time::Instant::now();
                        panel.health_scan = Some(health_scan(
                            state.repo_root.clone(),
                            panel.health_deep,
                            true,
                        ));
                        panel.notice =
                            "Repairing private application storage, then checking again…".into();
                    }
                    Action::HealthDetails => {
                        if let Some(report) = &panel.health {
                            let text = report.text();
                            let width = state.ui.input_wrap_width.saturating_sub(12).clamp(1, 96);
                            show_detail(&mut panel, &text, width);
                        }
                    }
                    Action::HealthAsk => {
                        if let Some(report) = &panel.health {
                            state
                                .ui
                                .queued_agent_prompts
                                .push_back(report.agent_prompt());
                            state.ui.queue_paused = false;
                            close = true;
                        }
                    }
                    Action::TeamPage(before) => {
                        panel.team_before = *before;
                        panel.load_team = true;
                        panel.selected = 0;
                    }
                    Action::TeamMessage(id) => {
                        if let Some(entry) = panel
                            .team_history
                            .iter()
                            .find(|entry| entry["message"]["id"].as_i64() == Some(*id))
                        {
                            let entry = entry.clone();
                            let width = state.ui.input_wrap_width.saturating_sub(12).clamp(1, 96);
                            team_preview(&mut panel, &entry, &state.repo_root, width);
                        }
                    }
                    Action::Section(section) => {
                        open(state, *section, Some(&panel.model));
                        state.ui.panel.as_mut().unwrap().settings = true;
                        return true;
                    }
                    Action::Session { id, root, online } => {
                        if id == &state.session.id {
                            close = true;
                        } else if *online {
                            panel.selected_session = Some(id.clone());
                            panel.preview_session = Some(id.clone());
                            panel.preview_offset = 0;
                            panel.notice="Read-only live snapshot · refreshed every second · session ownership preserved".into();
                            match persistence::preview_session(id) {
                                Ok(lines) => panel.preview_lines = lines,
                                Err(e) => panel.notice = e,
                            };
                        } else {
                            close = request_session(
                                state,
                                &mut panel,
                                Some(id.clone()),
                                root.clone(),
                                false,
                            );
                        }
                    }
                    Action::Command(cmd) => {
                        state.ui.requested_command = Some(cmd.clone());
                        close = true;
                    }
                    Action::Theme(value) => {
                        state.theme = UiTheme::parse(value).unwrap();
                        changed = true;
                    }
                    Action::Accent(value) => {
                        state.accent = UiAccent::parse(value).unwrap();
                        changed = true;
                    }
                    Action::Density(value) => {
                        state.density = UiDensity::parse(value).unwrap();
                        changed = true;
                    }
                    Action::Profile(value) => {
                        state.permission_profile = *value;
                        state.ui.auto_approve = false;
                        changed = true;
                    }
                    Action::Approval => {
                        state.ui.auto_approve = !state.ui.auto_approve;
                        changed = true;
                    }
                    Action::Motion => {
                        state.preferences.reduce_motion = !state.preferences.reduce_motion;
                        changed = true;
                    }
                    Action::Art => {
                        state.preferences.ambient_art = !state.preferences.ambient_art;
                        changed = true;
                    }
                    Action::Status(id) => {
                        if let Some(at) =
                            state.preferences.status_fields.iter().position(|s| s == id)
                        {
                            state.preferences.status_fields.remove(at);
                        } else {
                            state.preferences.status_fields.push(id.clone());
                        }
                        changed = true;
                    }
                    Action::ChooseProvider(provider) => {
                        if &panel.model.provider != provider {
                            panel.model.provider = provider.clone();
                            panel.model.base_url = None;
                            panel.model.api = None;
                            panel.models.clear();
                        }
                        panel.picker = Some(1);
                        panel.load_models = true;
                        panel.query.clear();
                        panel.selected = 0;
                        panel.notice = "Loading models from your endpoint…".into();
                    }
                    Action::ChooseModel(model) => {
                        state.ui.requested_command = Some(format!(
                            "/model {} {} {} --api={}",
                            panel.model.provider,
                            model,
                            panel.model.base_url.as_deref().unwrap_or(""),
                            panel.model.api.map(|a| a.name()).unwrap_or("auto")
                        ));
                        close = true;
                    }
                    Action::EditModel(field) if *field < 2 => {
                        panel.picker = Some(*field);
                        panel.query.clear();
                        panel.selected = 0;
                        panel.load_models = *field == 1;
                        if *field == 1 {
                            if panel.models.is_empty() {
                                panel.models.push(panel.model.model.clone());
                            }
                            panel.notice = "Loading models from your endpoint…".into();
                        }
                    }
                    Action::EditModel(field) | Action::CustomModel(field) => {
                        let text = match field {
                            0 => panel.model.provider.clone(),
                            1 => panel.model.model.clone(),
                            _ => panel.model.base_url.clone().unwrap_or_default(),
                        };
                        panel.edit = Some((
                            *field,
                            if panel.query.is_empty() {
                                text
                            } else {
                                panel.query.clone()
                            },
                        ));
                        panel.picker = None;
                        panel.query.clear();
                        panel.selected = *field;
                    }
                    Action::CycleApi => {
                        use crate::agent::provider::Api;
                        panel.model.api = match panel.model.api {
                            None => Some(Api::Responses),
                            Some(Api::Responses) => Some(Api::ChatCompletions),
                            Some(Api::ChatCompletions) => Some(Api::AnthropicMessages),
                            Some(Api::AnthropicMessages) => None,
                        };
                        panel.notice = "Protocol draft updated · choose Apply model to save".into();
                    }
                    Action::Reasoning => {
                        let options = crate::model_options::get(&panel.model);
                        let current = options["reasoning"].as_str().unwrap_or("auto");
                        let next = match current {
                            "auto" => "low",
                            "low" => "medium",
                            "medium" => "high",
                            _ => "auto",
                        };
                        panel.notice = match crate::model_options::set_reasoning(&panel.model, next)
                        {
                            Ok(()) => format!(
                                "Reasoning override saved for {} · {}",
                                panel.model.model, next
                            ),
                            Err(error) => error,
                        };
                    }
                    Action::Sandbox => {
                        if state.ui.agent_running {
                            panel.notice =
                                "Interrupt the active run before changing OS enforcement".into();
                        } else {
                            let next = match crate::sandbox::mode().as_str() {
                                "off" => "workspace",
                                "workspace" => "read-only",
                                _ => "off",
                            };
                            panel.notice = match crate::sandbox::set(next) {
                                Ok(()) => format!("OS command sandbox: {next}"),
                                Err(error) => error,
                            };
                        }
                    }
                    Action::ApplyModel => {
                        state.ui.requested_command = Some(format!(
                            "/model {} {} {} --api={}",
                            panel.model.provider,
                            panel.model.model,
                            panel.model.base_url.as_deref().unwrap_or(""),
                            panel.model.api.map(|a| a.name()).unwrap_or("auto")
                        ));
                        close = true;
                    }
                    Action::KeyBinding(id) => {
                        panel.capture = Some(id.clone());
                        panel.notice = "Press the new shortcut; Escape cancels".into();
                    }
                    Action::Skill(name) => {
                        if key.code == KeyCode::Char(' ') {
                            if !state.preferences.disabled_skills.remove(name) {
                                state.preferences.disabled_skills.insert(name.clone());
                            }
                            changed = true;
                        } else if state.preferences.disabled_skills.contains(name) {
                            panel.notice =
                                "Enable this skill with Space before inserting it".into();
                        } else {
                            let already_dollar = state.ui.input
                                [..state.ui.input_cursor.min(state.ui.input.len())]
                                .ends_with('$');
                            state.insert_text(&format!(
                                "{}{} ",
                                if already_dollar { "" } else { "$" },
                                name
                            ));
                            close = true;
                        }
                    }
                    Action::None => {}
                }
            }
        }
        KeyCode::Backspace => {
            panel.query.pop();
            panel.selected = 0;
        }
        KeyCode::Char('u' | 'a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            panel.query.clear();
            panel.selected = 0;
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            panel.query.push(c);
            panel.selected = 0;
        }
        _ => {}
    }
    if panel.section == 10 {
        if matches!(
            key.code,
            KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
        ) {
            panel.selected_session = entries(state, &panel)
                .get(panel.selected)
                .and_then(|r| r.id.clone());
        }
        if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace) {
            panel.selected_session = None;
        }
        let next_rows = entries(state, &panel);
        if next_rows
            .get(selected_index(&panel, &next_rows))
            .and_then(|row| row.id.as_ref())
            != selected_before.as_ref()
        {
            panel.opened = std::time::Instant::now();
        }
    }
    if changed {
        panel.notice = match persistence::save(state) {
            Ok(()) => "Saved to this workspace".into(),
            Err(e) => format!("Not saved: {e}"),
        };
    }
    if !close {
        state.ui.panel = Some(panel);
    }
    true
}

pub fn paste(state: &mut AgentState, text: &str) {
    if let Some(panel) = state.ui.panel.as_mut() {
        let target = if let Some((_, text)) = panel.edit.as_mut() {
            text
        } else {
            &mut panel.query
        };
        target.extend(
            text.chars()
                .filter(|c| !c.is_control())
                .take(4096usize.saturating_sub(target.len())),
        );
        panel.selected = 0;
    }
}

pub fn status_entries(state: &AgentState) -> Vec<(String, String)> {
    state
        .preferences
        .status_fields
        .iter()
        .filter_map(|id| {
            let value = match id.as_str() {
                "model" => state
                    .ui
                    .presentation_model
                    .as_ref()
                    .map(|m| format!("{} · {}", m.model, m.provider))?,
                "directory" => {
                    let path = state.repo_root.display().to_string();
                    if let Some(home) = dirs::home_dir() {
                        path.replacen(&home.display().to_string(), "~", 1)
                    } else {
                        path
                    }
                }
                "branch" => state.ui.repo_branch.clone()?,
                "context" => format!("~{} ctx", state.conversation.token_estimate),
                "permissions" => format!(
                    "{} · {}",
                    if state.plan_mode {
                        "plan read-only"
                    } else {
                        state.permission_profile.as_str()
                    },
                    if state.ui.auto_approve && !state.plan_mode {
                        "auto-approve"
                    } else {
                        "ask approvals"
                    }
                ),
                "tokens" => format!(
                    "~{} tokens",
                    state.usage.prompt_tokens + state.usage.completion_tokens
                ),
                "session" => state.session_name.clone()?,
                "activity" => state
                    .ui
                    .current_tool
                    .clone()
                    .unwrap_or_else(|| state.ui.run_phase.clone()),
                "elapsed" => super::helper::elapsed_text(
                    state.started_at.elapsed().as_millis().min(u64::MAX as u128) as u64,
                    false,
                ),
                "version" => format!("v{}", env!("CARGO_PKG_VERSION")),
                _ => return None,
            };
            Some((id.clone(), value))
        })
        .collect()
}

pub fn statusline(state: &AgentState) -> Vec<String> {
    status_entries(state)
        .into_iter()
        .map(|(_, value)| value)
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn doctor_opens_from_settings_and_queues_only_an_explicit_investigation() {
        let mut state = crate::init_state();
        state.ui.input_wrap_width = 100;
        state.insert_text("keep my draft");
        open(&mut state, 12, None);
        let doctor = view(&state)
            .unwrap()
            .rows
            .iter()
            .position(|row| row.label == "Doctor")
            .unwrap();
        assert!(choose_at(&mut state, doctor, "Doctor"));
        assert_eq!(view(&state).unwrap().section, 14);
        assert!(state.ui.panel.as_ref().unwrap().health_scan.is_none());
        let report = crate::diagnostics::Report {
            duration_ms: 1200,
            version: "test",
            at: "now".into(),
            workspace: state.repo_root.clone(),
            deep: true,
            repairs: vec![],
            checks: vec![crate::diagnostics::Check {
                id: "fixture".into(),
                title: "Fixture storage".into(),
                status: crate::diagnostics::Status::Fail,
                detail: "needs attention".into(),
                remedy: Some("inspect configuration".into()),
                repairable: false,
            }],
        };
        let panel = state.ui.panel.as_mut().unwrap();
        panel.health = Some(report);
        panel.load_health = false;
        let rows = view(&state).unwrap().rows;
        assert!(state.ui.queued_agent_prompts.is_empty());
        let full = rows
            .iter()
            .position(|row| row.label == "Full diagnostic report")
            .unwrap();
        assert!(choose_at(&mut state, full, "Full diagnostic report"));
        assert!(view(&state)
            .unwrap()
            .preview
            .join("\n")
            .contains("needs attention"));
        assert!(choose_at(&mut state, 0, "Show less"));
        let ask = view(&state)
            .unwrap()
            .rows
            .iter()
            .position(|row| row.label == "Ask agent to investigate")
            .unwrap();
        assert!(choose_at(&mut state, ask, "Ask agent to investigate"));
        assert_eq!(state.ui.queued_agent_prompts.len(), 1);
        assert!(state.ui.queued_agent_prompts[0].contains("current permission policy"));
        assert_eq!(state.ui.input, "keep my draft");
        assert!(state.ui.panel.is_none());
    }
    #[test]
    fn team_notice_opens_full_safe_text_and_show_less_preserves_the_draft() {
        let mut state = crate::init_state();
        state.ui.input = "keep my draft".into();
        state.ui.input_wrap_width = 80;
        let body = format!("\u{1b}[2J{}END OF MESSAGE", "\tcontext line\n".repeat(100));
        state.logs.push_peer(format!(
            "TEAM|{}",
            serde_json::json!({
                "outgoing":false,"message":{"id":77,"sender":uuid::Uuid::new_v4().to_string(),
                "sender_title":"Agent B · API","recipient":state.session.id,"body":body,"at":0}
            })
        ));
        open_team_message(&mut state, 77);
        let panel = view(&state).unwrap();
        assert!(panel.previewing);
        assert!(panel.preview.len() > 100);
        assert!(panel
            .preview
            .iter()
            .any(|line| line.contains("END OF MESSAGE")));
        assert!(!panel.preview.iter().any(|line| line.contains('\u{1b}')));
        assert!(choose_at(&mut state, 0, "Show less"));
        assert!(!view(&state).unwrap().previewing);
        assert_eq!(state.ui.input, "keep my draft");
    }
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn peer(id: &str, title: &str, status: &str, online: bool) -> crate::team::Presence {
        crate::team::Presence {
            id: id.into(),
            session: id.into(),
            project: "project".into(),
            workspace: "/project".into(),
            title: title.into(),
            model: "model".into(),
            provider: "local".into(),
            status: status.into(),
            detail: String::new(),
            updated: 0,
            online,
            enabled: true,
            pinned: false,
            role: "Agent A".into(),
        }
    }
    #[test]
    fn new_session_uses_control_n_without_consuming_search_text() {
        let mut state = crate::init_state();
        state.insert_text("keep this draft");
        let root = state.repo_root.clone();
        open(&mut state, 10, None);
        handle_key(&mut state, key(KeyCode::F(2)));
        assert!(state.ui.open_session.is_none());
        handle_key(&mut state, key(KeyCode::Char('n')));
        assert_eq!(state.ui.panel.as_ref().unwrap().query, "n");
        handle_key(
            &mut state,
            crossterm::event::KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        );
        let next = state.ui.open_session.as_ref().unwrap();
        assert!(next.id.is_none());
        assert_eq!(next.root, root);
        assert_eq!(state.ui.input, "keep this draft");
    }

    #[test]
    fn settings_categories_open_by_keyboard_or_click_and_escape_returns_home() {
        let mut state = crate::init_state();
        state.insert_text("keep my draft");
        assert_eq!(section_for("/settings"), Some(12));
        open(&mut state, 12, None);
        assert_eq!(view(&state).unwrap().rows.len(), 11);
        handle_key(&mut state, key(KeyCode::Tab));
        assert_eq!(view(&state).unwrap().section, 12);
        assert!(choose_at(&mut state, 2, "Permissions"));
        assert_eq!(view(&state).unwrap().section, 3);
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(view(&state).unwrap().section, 12);
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(view(&state).unwrap().section, 1);
        handle_key(&mut state, key(KeyCode::Esc));
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.ui.panel.is_none());
        assert_eq!(state.ui.input, "keep my draft");
    }

    #[test]
    fn team_history_opens_from_settings_with_saved_metadata_and_read_receipts() {
        let mut state = crate::init_state();
        state.insert_text("keep my draft");
        state.ui.input_wrap_width = 120;
        open(&mut state, 12, None);
        let team = view(&state)
            .unwrap()
            .rows
            .iter()
            .position(|row| row.label == "Team")
            .unwrap();
        assert!(choose_at(&mut state, team, "Team"));
        assert_eq!(view(&state).unwrap().section, 11);
        state.ui.panel.as_mut().unwrap().team_history = vec![serde_json::json!({
            "message":{"id":42,"sender":"sender-session","sender_title":"Agent A · Backend",
                "recipient":state.session.id,"body":"Full message\nLast line", "at":0},
            "recipient_title":"Frontend", "receipts":[[state.session.id,1000]]
        })];
        let history = view(&state).unwrap();
        assert!(history.rows[0].value.contains("Read"));
        assert!(choose_at(&mut state, 0, &history.rows[0].label));
        let preview = view(&state).unwrap().preview.join("\n");
        for metadata in [
            "Agent A · Backend",
            "sender-session",
            "Frontend",
            "01 Jan 1970",
            "UTC",
            "Message  #42",
            "Read",
            "Read by",
            "Last line",
        ] {
            assert!(preview.contains(metadata), "missing {metadata}");
        }
        assert!(preview.contains(&state.session.id));
        assert!(preview.contains(&state.repo_root.display().to_string()));
        assert!(choose_at(&mut state, 0, "Show less"));
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(view(&state).unwrap().section, 12);
        assert_eq!(state.ui.input, "keep my draft");
    }

    #[test]
    fn workspace_category_exposes_editor_and_companion_controls() {
        let mut state = crate::init_state();
        open(&mut state, 12, None);
        assert!(choose_at(&mut state, 8, "Workspace"));
        let panel = view(&state).unwrap();
        assert_eq!(panel.section, 13);
        assert_eq!(panel.rows.len(), 5);
        assert!(panel
            .rows
            .iter()
            .any(|row| row.label == "Download Mac companion"));
        assert!(panel
            .rows
            .iter()
            .any(|row| row.label == "Download iPhone companion"));
        assert!(panel.rows.iter().any(|row| row.label.contains("Mac")));
        assert!(panel.rows.iter().any(|row| row.label.contains("iPhone")));
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(view(&state).unwrap().section, 12);
    }
    #[test]
    fn session_center_sorts_recent_activity_across_projects_and_keeps_selection_identity() {
        let mut state = crate::init_state();
        let mut old = peer("old", "AAA", "closed", false);
        old.workspace = "/aaa/project".into();
        old.updated = 10;
        let mut recent = peer("recent", "ZZZ", "ready", true);
        recent.workspace = "/zzz/project".into();
        recent.updated = 30;
        let mut middle = peer("middle", "BBB", "closed", false);
        middle.updated = 20;
        state.ui.team.peers = vec![old, recent, middle];
        open(&mut state, 10, None);
        assert_eq!(
            view(&state)
                .unwrap()
                .rows
                .iter()
                .map(|r| r.id.as_deref().unwrap())
                .collect::<Vec<_>>(),
            vec!["recent", "middle", "old"]
        );
        handle_key(&mut state, key(KeyCode::Down));
        state.ui.team.peers[0].updated = 40;
        let next = view(&state).unwrap();
        assert_eq!(next.rows[0].id.as_deref(), Some("old"));
        assert_eq!(next.rows[next.selected].id.as_deref(), Some("middle"));
    }

    #[test]
    fn session_center_anchors_identity_and_filters_without_touching_composer() {
        let mut state = crate::init_state();
        state.insert_text("original draft");
        state.ui.team.peers = vec![
            peer("a", "Same task", "working", true),
            peer("b", "Same task", "error", true),
            peer("c", "Closed", "closed", false),
        ];
        open(&mut state, 10, None);
        handle_key(&mut state, key(KeyCode::Down));
        assert_eq!(
            view(&state).unwrap().rows[view(&state).unwrap().selected]
                .id
                .as_deref(),
            Some("a")
        );
        // A changing title reorders rows; the selected session must remain selected.
        state.ui.team.peers[0].title = "ZZZ".into();
        let v = view(&state).unwrap();
        assert_eq!(v.rows[v.selected].id.as_deref(), Some("a"));
        handle_key(&mut state, key(KeyCode::Right));
        let v = super::view(&state).unwrap();
        assert_eq!(v.rows.len(), 1);
        assert_eq!(v.rows[0].id.as_deref(), Some("b"));
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.ui.input, "original draft");
        assert!(!state.ui.should_exit);
    }
    #[test]
    fn message_editor_never_applies_to_model_config_and_preserves_failed_drafts() {
        let mut state = crate::init_state();
        state.insert_text("keep prompt");
        state.ui.team.peers = vec![peer("unregistered", "Peer", "ready", true)];
        open(&mut state, 10, None);
        let original = state.ui.panel.as_ref().unwrap().model.model.clone();
        handle_key(&mut state, key(KeyCode::F(4)));
        paste(&mut state, "Coordinate this change");
        handle_key(&mut state, key(KeyCode::Enter));
        let panel = state.ui.panel.as_ref().unwrap();
        assert_eq!(panel.model.model, original);
        assert!(panel.message_to.is_some());
        assert_eq!(panel.edit.as_ref().unwrap().1, "Coordinate this change");
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.ui.input, "keep prompt");
        assert!(state.ui.panel.is_some());
    }
    #[test]
    fn command_center_searches_without_touching_draft_and_queues_existing_command() {
        let mut state = crate::init_state();
        state.insert_text("keep this draft");
        let cursor = state.ui.input_cursor;
        open(&mut state, 0, None);
        paste(&mut state, "permissions");
        let panel = view(&state).unwrap();
        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].label, "/permissions");
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(state.ui.panel.is_none());
        assert_eq!(state.ui.requested_command.as_deref(), Some("/permissions"));
        assert_eq!(state.ui.input, "keep this draft");
        assert_eq!(state.ui.input_cursor, cursor);
        open(&mut state, 1, None);
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(!state.ui.should_exit);
        assert_eq!(state.ui.input, "keep this draft");
    }
    #[test]
    fn model_edits_are_explicit_and_invalid_endpoint_is_not_applied() {
        let mut state = crate::init_state();
        open(&mut state, 2, None);
        state.ui.panel.as_mut().unwrap().selected = 2;
        handle_key(&mut state, key(KeyCode::Enter));
        paste(&mut state, "https://user:secret@example.invalid");
        handle_key(&mut state, key(KeyCode::Enter));
        assert!(view(&state)
            .unwrap()
            .notice
            .contains("without embedded credentials"));
        assert!(state.ui.requested_command.is_none());
        handle_key(&mut state, key(KeyCode::Esc));
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.ui.panel.is_none());
    }
    #[test]
    fn footer_selectors_preserve_drafts_choose_models_and_cancel_without_applying() {
        let mut state = crate::init_state();
        state.ui.input = "draft stays".into();
        state.ui.input_cursor = 5;
        state.ui.presentation_model = Some(ModelConfig {
            model: "current".into(),
            ..ModelConfig::default()
        });
        open_selector(&mut state, false);
        let panel = state.ui.panel.as_mut().unwrap();
        panel.models = vec!["current".into(), "chosen".into()];
        panel.load_models = false;
        panel.selected = 1;
        assert!(!choose_at(&mut state, 1, "stale catalog label"));
        assert!(choose_at(&mut state, 1, "chosen"));
        assert!(state.ui.panel.is_none());
        assert!(state
            .ui
            .requested_command
            .take()
            .unwrap()
            .starts_with("/model openai chosen"));
        assert_eq!(state.ui.input, "draft stays");
        assert_eq!(state.ui.input_cursor, 5);
        open_selector(&mut state, true);
        handle_key(&mut state, key(KeyCode::Down));
        handle_key(&mut state, key(KeyCode::Enter));
        let panel = state.ui.panel.as_ref().unwrap();
        assert_eq!(panel.model.provider, "groq");
        assert!(panel.model.base_url.is_none());
        assert!(panel.load_models);
        assert_eq!(panel.picker, Some(1));
        handle_key(&mut state, key(KeyCode::Esc));
        handle_key(&mut state, key(KeyCode::Esc));
        assert!(state.ui.panel.is_none());
        assert!(state.ui.requested_command.is_none());
    }

    #[test]
    fn stats_reveal_restarts_only_for_entry_or_sample_toggle() {
        let mut state = crate::init_state();
        open(&mut state, 8, None);
        state.ui.panel.as_mut().unwrap().opened =
            std::time::Instant::now() - std::time::Duration::from_secs(2);
        handle_key(&mut state, key(KeyCode::Down));
        assert!(view(&state).unwrap().elapsed >= 2000);
        handle_key(&mut state, key(KeyCode::Char('d')));
        assert!(view(&state).unwrap().elapsed < 500);
    }

    #[test]
    fn statusline_omits_unavailable_and_unknown_values_and_preserves_order() {
        let mut state = crate::init_state();
        state.preferences.status_fields = vec![
            "session".into(),
            "future".into(),
            "version".into(),
            "context".into(),
        ];
        let line = statusline(&state);
        assert_eq!(line.len(), 2);
        assert!(line[0].starts_with('v'));
        assert!(line[1].starts_with('~'));
    }

    #[test]
    fn permission_requests_keep_priority_over_panels_and_remapped_keys() {
        use crate::state::PendingPermission;
        let mut state = crate::init_state();
        open(&mut state, 0, None);
        state.ui.agent_running = true;
        let (tx, rx) = std::sync::mpsc::channel();
        state.ui.pending_permission = Some(PendingPermission {
            tool_name: "Shell".into(),
            args_summary: "controlled test".into(),
            reply_tx: tx,
        });
        super::super::main_ui::handle_event(
            &mut state,
            crossterm::event::Event::Key(key(KeyCode::F(1))),
            ratatui::layout::Rect::default(),
            ratatui::layout::Rect::default(),
            ratatui::layout::Rect::default(),
        );
        assert!(rx.try_recv().is_err());
        assert!(state.ui.pending_permission.is_some());
        super::super::main_ui::handle_event(
            &mut state,
            crossterm::event::Event::Key(key(KeyCode::Esc)),
            ratatui::layout::Rect::default(),
            ratatui::layout::Rect::default(),
            ratatui::layout::Rect::default(),
        );
        assert!(!rx.recv().unwrap());
        assert!(state.ui.cancel_requested);
        assert!(!state.ui.auto_approve);
    }

    #[test]
    fn permission_navigation_denies_without_changing_the_draft_and_cancel_is_idempotent() {
        use crossterm::event::Event;
        use ratatui::layout::Rect;
        let mut state = crate::init_state();
        state.insert_text("keep my draft");
        state.ui.agent_running = true;
        let (tx, rx) = std::sync::mpsc::channel();
        state.ui.pending_permission = Some(crate::state::PendingPermission {
            tool_name: "edit_file".into(),
            args_summary: "test.txt".into(),
            reply_tx: tx,
        });
        for code in [KeyCode::Down, KeyCode::Enter] {
            super::super::main_ui::handle_event(
                &mut state,
                Event::Key(key(code)),
                Rect::default(),
                Rect::default(),
                Rect::default(),
            );
        }
        assert!(!rx.recv().unwrap());
        assert_eq!(state.ui.input, "keep my draft");
        assert!(!state.ui.auto_approve);
        super::super::main_ui::handle_event(
            &mut state,
            Event::Key(key(KeyCode::Esc)),
            Rect::default(),
            Rect::default(),
            Rect::default(),
        );
        let lines = state.logs.iter().count();
        state.ui.cancel_requested = false;
        super::super::main_ui::handle_event(
            &mut state,
            Event::Key(key(KeyCode::Esc)),
            Rect::default(),
            Rect::default(),
            Rect::default(),
        );
        assert_eq!(state.logs.iter().count(), lines);
        assert!(!state.ui.cancel_requested);
    }
}

fn load_usage(state: &AgentState, section: usize) -> (Vec<crate::usage::Day>, String) {
    if !matches!(section, 7 | 8) {
        return (Vec::new(), String::new());
    }
    match crate::usage::history(&state.repo_root) {
        Ok(days) => {
            let message = if days.is_empty() {
                "No provider usage recorded yet. History begins with new reported responses."
            } else {
                "Provider-reported metadata · last 365 UTC days · at most 2,000 day/model groups"
            };
            (days, message.into())
        }
        Err(e) => (Vec::new(), format!("Usage history unavailable: {e}")),
    }
}
pub fn refresh_usage(state: &mut AgentState) {
    if let Some(section) = state
        .ui
        .panel
        .as_ref()
        .map(|p| p.section)
        .filter(|s| matches!(s, 7 | 8))
    {
        let (days, notice) = load_usage(state, section);
        if let Some(panel) = state.ui.panel.as_mut() {
            panel.days = days;
            panel.notice = notice;
        }
    }
}
