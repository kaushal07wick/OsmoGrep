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
];

pub struct Panel {
    pub section: usize,
    pub query: String,
    pub selected: usize,
    pub notice: String,
    model: ModelConfig,
    edit: Option<(usize, String)>,
    capture: Option<String>,
    skills: Vec<crate::skills::Skill>,
    days: Vec<crate::usage::Day>,
}

#[derive(Clone)]
enum Action {
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
    ApplyModel,
    KeyBinding(String),
    Skill(String),
    None,
}
#[derive(Serialize)]
pub struct Row {
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
        label: label.into(),
        description: description.into(),
        value: value.into(),
        action,
    }
}
#[derive(Serialize)]
pub struct PanelView {
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
}

pub fn section_for(command: &str) -> Option<usize> {
    Some(match command {
        "/help" => 0,
        "/settings" | "/config" | "/theme" | "/color" | "/type" => 1,
        "/model" | "/providers" | "/account" => 2,
        "/permissions" | "/profile" => 3,
        "/statusline" => 4,
        "/keymap" => 5,
        "/skills" => 6,
        "/usage" => 7,
        "/stats" => 8,
        "/status" | "/metrics" => 9,
        _ => return None,
    })
}
pub fn open(state: &mut AgentState, section: usize, model: Option<&ModelConfig>) {
    state.ui.command_items.clear();
    let (days, notice) = load_usage(state, section);
    state.ui.panel = Some(Panel {
        section,
        query: String::new(),
        selected: 0,
        notice,
        model: model.cloned().unwrap_or_default(),
        edit: None,
        days,
        capture: None,
        skills: if section == 6 {
            crate::skills::discover(&state.repo_root)
        } else {
            Vec::new()
        },
    });
}

fn entries(state: &AgentState, panel: &Panel) -> Vec<Row> {
    let check = |b| if b { "on" } else { "off" };
    let mut rows = match panel.section {
        0 => commands::command_palette_items().iter().map(|c| row(c.cmd, c.desc, "↵", Action::Command(c.cmd.into()))).collect(),
        1 => {
            let mut rows = vec![
                row("Dark", "Black canvas, warm ink", if state.theme == UiTheme::Dark {"●"} else {""}, Action::Theme("dark")),
                row("Light", "Paper canvas, dark ink", if state.theme == UiTheme::Light {"●"} else {""}, Action::Theme("light")),
                row("Reduce motion", "Settle artwork immediately", check(state.preferences.reduce_motion), Action::Motion),
                row("Ambient artwork", "Quiet central artwork on an empty conversation", check(state.preferences.ambient_art), Action::Art),
            ];
            for name in ["orange", "blue", "green", "violet", "rose"] { rows.push(row(format!("Accent · {name}"), "Workspace highlight color", if state.accent.as_str()==name {"●"}else{""}, Action::Accent(name))); }
            for name in ["compact", "standard", "spacious"] { rows.push(row(format!("Density · {name}"), "Header and transcript spacing", if state.density.as_str()==name {"●"}else{""}, Action::Density(name))); }
            rows
        },
        2 => vec![
            row("Provider", "OpenAI Responses-compatible endpoint; credentials stay in Rust", &panel.model.provider, Action::EditModel(0)),
            row("Model", "Enter the model identifier supported by your endpoint", &panel.model.model, Action::EditModel(1)),
            row("Endpoint", "Optional http(s) URL; empty uses the provider default", panel.model.base_url.as_deref().unwrap_or("default"), Action::EditModel(2)),
            row("Apply model", "Save through the existing model configuration path", "↵", Action::ApplyModel),
            row("API key", "Open the masked credential input", "↵", Action::Command("/key".into())),
        ],
        3 => vec![
            row("Read only", "Block tools classified as dangerous", selected_profile(state, PermissionProfile::ReadOnly), Action::Profile(PermissionProfile::ReadOnly)),
            row("Ask for approval", "Prompt before dangerous tools unless auto-approval is enabled", selected_profile(state, PermissionProfile::WorkspaceAuto), Action::Profile(PermissionProfile::WorkspaceAuto)),
            row("Full access", "Allow dangerous tools without prompting", selected_profile(state, PermissionProfile::FullAccess), Action::Profile(PermissionProfile::FullAccess)),
            row("Auto-approve dangerous tools", "Session only · explicitly bypass dangerous-tool prompts", check(state.ui.auto_approve), Action::Approval),
            row("Policy boundary", "Tool classifications govern access; this is not an OS sandbox. Plan mode forces read only.", "", Action::None),
        ],
        4 => {
            let mut ids = state.preferences.status_fields.clone();
            ids.extend(STATUS_FIELDS.iter().filter(|(id,_,_)| !state.preferences.status_fields.iter().any(|s| s==id)).map(|(id,_,_)|id.to_string()));
            ids.iter().filter_map(|id| STATUS_FIELDS.iter().find(|(key,_,_)| key==id)).map(|(id,label,desc)| row(*label,*desc, if state.preferences.status_fields.iter().any(|s|s==id) {"[×]"} else {"[ ]"}, Action::Status(id.to_string()))).collect()
        },
        5 => super::keymap::BINDINGS.iter().map(|b| row(b.label,format!("{} · default {}{}",b.group,b.default,if b.id=="copy" {" · protected"}else{""}),format!("{}{}",super::keymap::effective(&state.preferences,b),if state.preferences.key_bindings.contains_key(b.id){" *"}else{""}),Action::KeyBinding(b.id.into()))).collect(),
        6 => panel.skills.iter().map(|skill| row(format!("${}",skill.name),format!("{} · {} · {}",skill.description,skill.source,skill.path.display()),if state.preferences.disabled_skills.contains(&skill.name){"disabled"}else{"enabled"},Action::Skill(skill.name.clone()))).collect(),
        7 => {
            let sum=|f:fn(&crate::usage::Day)->u64|panel.days.iter().map(f).sum::<u64>();
            vec![row("Reported input", "Provider counts · last 365 UTC days · this workspace",sum(|d|d.input).to_string(),Action::None),row("Reported output", "Includes reasoning tokens when reported by the provider",sum(|d|d.output).to_string(),Action::None),row("Cached input", "Subset of reported input; never added twice",sum(|d|d.cached).to_string(),Action::None),row("Model calls", "Successful responses that included valid usage metadata",sum(|d|d.calls).to_string(),Action::None),row("Provider wait", "Includes retries; not total agent runtime",format!("{:.1}s",sum(|d|d.duration_ms) as f64/1000.0),Action::None),row("Legacy text estimate", "Separate from provider counts; not billing data",format!("~{} tokens",state.usage.prompt_tokens+state.usage.completion_tokens),Action::None),row("Context estimate", "Current conversation estimate",format!("~{} tokens",state.conversation.token_estimate),Action::None),row("Pricing and quotas", "Not reported by the provider; no invented prices or account limits", "unavailable",Action::None)]
        },
        8 => {
            let mut models=std::collections::BTreeMap::<String,(u64,u64)>::new();
            for day in &panel.days {let total=models.entry(format!("{} · {}",day.model,day.provider)).or_default();total.0+=day.input+day.output;total.1+=day.calls;}
            let mut models:Vec<_>=models.into_iter().collect();models.sort_by_key(|(_,v)|std::cmp::Reverse(v.0));
            models.into_iter().map(|(model,(tokens,calls))|row(model,format!("{calls} reported model calls · input + output; cached input is a subset"),format!("{tokens} tokens"),Action::None)).collect()
        },
        9 => vec![
            row("Workspace", state.repo_root.display().to_string(), state.ui.repo_branch.as_deref().unwrap_or("no branch"), Action::None),
            row("Run", state.ui.current_tool.as_deref().unwrap_or("No active tool"), &state.ui.run_phase, Action::None),
            row("Session", state.session_name.as_deref().unwrap_or("Unnamed conversation"), format!("{} messages", state.conversation.messages.len()), Action::None),
            row("Background work", "Existing queued and running jobs", state.jobs.len().to_string(), Action::None),
            row("Verification", "Inspect the execution evidence ledger", "↵", Action::Command("/verify".into())),
            row("Connected tools", "Inspect MCP server configuration", "↵", Action::Command("/mcp".into())),
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
        sections: SECTIONS.to_vec(),
        section: panel.section,
        selected: panel.selected.min(rows.len().saturating_sub(1)),
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
        notice: if panel.notice.is_empty() {
            "Workspace settings · saved locally · draft preserved".into()
        } else {
            panel.notice.clone()
        },
        editing: panel.edit.is_some() || panel.capture.is_some(),
        footer: if panel.capture.is_some() {
            "press new chord · backspace default · delete unbind · esc cancel"
        } else if panel.section == 5 {
            "enter rebind · * customized · tab section · esc close"
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
        preview: statusline(state),
        history: panel.days.clone(),
    })
}

pub fn handle_key(state: &mut AgentState, key: KeyEvent) -> bool {
    let Some(mut panel) = state.ui.panel.take() else {
        return false;
    };
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
            panel.notice = "Use a function key or Ctrl/Alt chord".into();
        }
        state.ui.panel = Some(panel);
        return true;
    }
    if let Some((field, text)) = panel.edit.as_mut() {
        match key.code {
            KeyCode::Esc => panel.edit = None,
            KeyCode::Enter => {
                let text = text.trim().to_string();
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
    let index = panel.selected.min(rows.len().saturating_sub(1));
    let mut close = false;
    let mut changed = false;
    match key.code {
        KeyCode::Esc => close = true,
        KeyCode::Tab | KeyCode::BackTab => {
            let back = key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
            panel.section =
                (panel.section + SECTIONS.len() + if back { SECTIONS.len() - 1 } else { 1 })
                    % SECTIONS.len();
            panel.query.clear();
            panel.selected = 0;
            panel.notice.clear();
            if matches!(panel.section, 7 | 8) {
                let (days, notice) = load_usage(state, panel.section);
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
                    Action::EditModel(field) => {
                        let text = match field {
                            0 => panel.model.provider.clone(),
                            1 => panel.model.model.clone(),
                            _ => panel.model.base_url.clone().unwrap_or_default(),
                        };
                        panel.edit = Some((*field, text));
                    }
                    Action::ApplyModel => {
                        state.ui.requested_command = Some(format!(
                            "/model {} {} {}",
                            panel.model.provider,
                            panel.model.model,
                            panel.model.base_url.as_deref().unwrap_or("")
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

pub fn statusline(state: &AgentState) -> Vec<String> {
    state
        .preferences
        .status_fields
        .iter()
        .filter_map(|id| {
            Some(match id.as_str() {
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
                "elapsed" => format!("{}m", state.started_at.elapsed().as_secs() / 60),
                "version" => format!("v{}", env!("CARGO_PKG_VERSION")),
                _ => return None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
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
