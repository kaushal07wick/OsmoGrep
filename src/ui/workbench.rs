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
    demo: bool,
    opened: std::time::Instant,
    picker: Option<usize>,
    models: Vec<String>,
    catalog: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
    load_models: bool,
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
    CustomModel(usize),
    ChooseProvider(String),
    ChooseModel(String),
    ApplyModel,
    CycleApi,
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
    pub title: String,
    pub selecting: bool,
    pub elapsed: u64,
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

pub fn choose_at(state: &mut AgentState, index: usize, label: &str) -> bool {
    let Some(panel) = state.ui.panel.as_ref() else {
        return false;
    };
    if panel.picker.is_none()
        || panel.edit.is_some()
        || entries(state, panel)
            .get(index)
            .is_none_or(|row| row.label != label)
    {
        return false;
    }
    state.ui.panel.as_mut().unwrap().selected = index;
    handle_key(state, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    true
}

pub fn poll(state: &mut AgentState, agent: &crate::agent::Agent) -> bool {
    let Some(panel) = state.ui.panel.as_mut() else {
        return false;
    };
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
        0 => commands::command_palette_items().iter().map(|c| row(c.cmd, c.desc, "↵", Action::Command(c.cmd.into()))).collect(),
        1 => {
            let mut rows = vec![
                row("Dark", "Black canvas · Graphite paper composer", if state.theme == UiTheme::Dark {"●"} else {""}, Action::Theme("dark")),
                row("Light", "Light canvas · Cotton paper composer", if state.theme == UiTheme::Light {"●"} else {""}, Action::Theme("light")),
                row("Reduce motion", "Settle artwork immediately", check(state.preferences.reduce_motion), Action::Motion),
                row("Ambient artwork", "Quiet central artwork on an empty conversation", check(state.preferences.ambient_art), Action::Art),
            ];
            for name in ["orange", "blue", "green", "violet", "rose"] { rows.push(row(format!("Accent · {name}"), "Workspace highlight color", if state.accent.as_str()==name {"●"}else{""}, Action::Accent(name))); }
            for name in ["compact", "standard", "spacious"] { rows.push(row(format!("Density · {name}"), "Header and transcript spacing", if state.density.as_str()==name {"●"}else{""}, Action::Density(name))); }
            rows
        },
        2 => vec![
            row("Provider", "Provider-specific APIs; credentials stay in Rust", &panel.model.provider, Action::EditModel(0)),
            row("Model", "Enter the model identifier supported by your endpoint", &panel.model.model, Action::EditModel(1)),
            row("Endpoint", "Optional http(s) URL; empty uses the provider default", panel.model.base_url.as_deref().unwrap_or("default"), Action::EditModel(2)),
            row("Apply model", "Save through the existing model configuration path", "↵", Action::ApplyModel),
            row("API key", "Open the masked credential input", "↵", Action::Command("/key".into())),
            row("API protocol", "Auto follows provider; Enter cycles for custom endpoints", panel.model.api.map(|a|a.name()).unwrap_or("auto"), Action::CycleApi),
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
        selecting: panel.picker.is_some() && panel.edit.is_none(),
        title: match panel.picker {
            Some(0) => "Choose provider".into(),
            Some(_) => format!("Choose model · {}", panel.model.provider),
            None => SECTIONS[panel.section].into(),
        },
        elapsed: panel.opened.elapsed().as_millis() as u64,
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
        preview: statusline(state),
        history: panel.days.clone(),
        demo: panel.demo,
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
        KeyCode::Esc => {
            if panel.picker.take().is_some() {
                panel.query.clear();
                panel.selected = 0;
            } else {
                close = true
            }
        }
        KeyCode::Char('d' | 'D') if panel.section == 8 => {
            panel.demo = !panel.demo;
            panel.opened = std::time::Instant::now();
        }
        KeyCode::Tab | KeyCode::BackTab => {
            let back = key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT);
            panel.section =
                (panel.section + SECTIONS.len() + if back { SECTIONS.len() - 1 } else { 1 })
                    % SECTIONS.len();
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
                "elapsed" => format!("{}m", state.started_at.elapsed().as_secs() / 60),
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
