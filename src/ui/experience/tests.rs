use super::actions::branch_context;
use super::*;
#[test]
fn context_search_matches_full_outputs_ranges_and_pins_without_sending_them() {
    let mut state = crate::init_state();
    state.logs.push(
        LogLevel::Info,
        format!("{} needle", "long output ".repeat(100)),
    );
    state.preferences.pins.push(super::super::references::Pin {
        id: "one".into(),
        title: "Pinned".into(),
        body: "needle private reference".into(),
    });
    open(&mut state, Page::Context);
    let mut panel = state.ui.experience.take().unwrap();
    panel.index = None;
    panel.query = "needle".into();
    assert_eq!(entries(&state, &panel).len(), 2);
    assert!(super::super::references::message_for(&state, "hello")
        .unwrap()
        .is_none());
    panel.files.push(Source {
        path: "src/main.rs".into(),
        line: None,
        end: None,
        symbol: None,
    });
    panel.query = "src/main.rs:10-20".into();
    let matches = entries(&state, &panel);
    assert_eq!(matches.len(), 1);
    assert!(matches!(&matches[0].target,Target::File(s) if s.line==Some(10)&&s.end==Some(20)));
}
#[test]
fn queue_editor_uses_shared_cursor_and_restores_draft_on_escape() {
    let mut state = crate::init_state();
    state.ui.input = "my draft".into();
    state.ui.input_cursor = 3;
    state.ui.queued_agent_prompts.push_back("queued 🦀".into());
    open(&mut state, Page::Queue);
    let button = view(&state).unwrap().buttons[0].id.clone();
    assert!(choose(&mut state, &button));
    assert!(editing(&state));
    let empty = ratatui::layout::Rect::default();
    crate::ui::main_ui::handle_event(
        &mut state,
        crossterm::event::Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
        empty,
        empty,
        empty,
    );
    assert_eq!(state.ui.input_cursor, 7);
    crate::ui::main_ui::handle_event(
        &mut state,
        crossterm::event::Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
        empty,
        empty,
        empty,
    );
    assert_eq!(state.ui.input, "queued x🦀");
    assert!(handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    ));
    assert_eq!(state.ui.input, "my draft");
    assert_eq!(state.ui.input_cursor, 3);
    assert_eq!(state.ui.queued_agent_prompts[0], "queued 🦀");
    assert!(state.ui.queue_paused);
}
#[test]
fn retry_recovers_references_without_hidden_context_in_the_branch() {
    let mut state = crate::init_state();
    super::super::references::attach_text(&mut state, "excerpt", "saved content").unwrap();
    let request = state.ui.input.clone();
    let attached = super::super::references::message_for(&state, &request)
        .unwrap()
        .unwrap();
    state.conversation.set_messages(vec![
        serde_json::json!({"role":"system","content":"identity"}),
        attached,
        serde_json::json!({"role":"user","content":request}),
    ]);
    state.ui.references.clear();
    let (prefix, refs, images) = branch_context(&state, 2).unwrap();
    assert_eq!(prefix.len(), 1);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].body, "saved content");
    assert!(images.is_empty());
    state.ui.references = refs;
    assert!(
        super::super::references::message_for(&state, "replacement without chip")
            .unwrap()
            .is_none()
    );
}
#[test]
fn masked_input_cannot_be_queued_or_exposed() {
    let mut state = crate::init_state();
    state.ui.input_mode = InputMode::ApiKey;
    state.ui.input = "SECRET".into();
    open(&mut state, Page::Find);
    assert!(view(&state).is_none());
    assert!(queue_draft(&mut state).is_err());
    assert!(steer_draft(&mut state).is_err());
    assert!(state.ui.queued_agent_prompts.is_empty());
}

#[test]
fn source_actions_reject_outside_paths_and_only_prepare_a_shell_draft() {
    let root = std::env::temp_dir().join(format!("source-actions-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.rs"), "bad\n").unwrap();
    let mut state = crate::init_state();
    state.repo_root = root.clone();
    state.logs.push(LogLevel::Info, "SHELL : $ cargo check");
    state.logs.push(LogLevel::Error, "a.rs:1: error");
    assert!(source(&mut state, r#"{"path":"../outside.rs","line":1}"#).is_err());
    source(&mut state, r#"{"path":"a.rs","line":1}"#).unwrap();
    let mut panel = state.ui.experience.take().unwrap();
    let entry = panel
        .contextual
        .iter()
        .find(|e| matches!(e.target, Target::Shell(_)))
        .unwrap()
        .clone();
    assert!(actions::act(&mut state, &mut panel, "rerun", &entry).unwrap());
    assert_eq!(state.ui.input, "!cargo check");
    assert!(!state.ui.execution_pending);
    assert!(state.ui.requested_command.is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_fallback_keeps_workspace_controls_reachable() {
    let mut state = crate::init_state();
    open(&mut state, Page::Queue);
    let backend = ratatui::backend::TestBackend::new(100, 30);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    crate::ui::tui::draw_ui(&mut terminal, &state).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("Queue"));
    assert!(text.contains("Resume queue"));
    assert!(handle_key(
        &mut state,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    ));
    assert!(state.ui.experience.is_none());
}
