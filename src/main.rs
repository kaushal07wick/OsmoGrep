use std::{error::Error, io, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

/* ===================== STATE ===================== */

#[derive(Clone, Debug)]
enum Phase {
    Init,
    DetectBase,
    Idle,
    ExecuteAgent,
    CreateNewAgent,
    Rollback,
    Done,
}

#[derive(Clone)]
enum LogLevel {
    Info,
    Success,
    Warn,
    Error,
}

struct LogLine {
    level: LogLevel,
    text: String,
}

struct AgentState {
    phase: Phase,
    base_branch: Option<String>,
    original_branch: Option<String>,
    agent_branch: Option<String>,
    input: String,
    input_focused: bool,
    logs: Vec<LogLine>,
}

/* ===================== LOGGING ===================== */

fn log(state: &mut AgentState, level: LogLevel, msg: impl Into<String>) {
    state.logs.push(LogLine {
        level,
        text: msg.into(),
    });
}

/* ===================== GIT ===================== */

fn is_git_repo() -> bool {
    std::path::Path::new(".git").exists()
}

fn current_branch() -> String {
    let out = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .expect("git failed");

    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn detect_base_branch() -> String {
    let out = std::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output();

    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        if let Some(b) = s.trim().split('/').last() {
            return b.to_string();
        }
    }
    "master".into()
}

fn working_tree_dirty() -> bool {
    let out = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .expect("git status failed");

    !out.stdout.is_empty()
}

fn find_existing_agent() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["branch"])
        .output()
        .ok()?;

    let s = String::from_utf8_lossy(&out.stdout);
    for l in s.lines() {
        let name = l.trim().trim_start_matches('*').trim();
        if name.starts_with("osmogrep/") {
            return Some(name.to_string());
        }
    }
    None
}

fn create_agent_branch() -> String {
    let name = format!(
        "osmogrep/{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    );

    std::process::Command::new("git")
        .args(["branch", &name])
        .status()
        .expect("branch create failed");

    name
}

fn checkout(branch: &str) {
    std::process::Command::new("git")
        .args(["checkout", "-q", branch])
        .status()
        .expect("checkout failed");
}

fn apply_working_tree(state: &mut AgentState) {
    // Check if there is anything to apply
    let diff = std::process::Command::new("git")
        .args(["diff"])
        .output()
        .expect("git diff failed");

    if diff.stdout.is_empty() {
        log(
            state,
            LogLevel::Info,
            "No working tree changes to apply.",
        );
        return;
    }

    // Apply diff safely
    let status = std::process::Command::new("git")
        .args(["apply", "-"])
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.as_mut().unwrap().write_all(&diff.stdout)?;
            child.wait_with_output()
        })
        .expect("git apply failed");

    if status.status.success() {
        log(
            state,
            LogLevel::Success,
            "Working tree applied.",
        );
    } else {
        let err = String::from_utf8_lossy(&status.stderr);
        log(
            state,
            LogLevel::Error,
            format!("Failed to apply diff: {}", err.trim()),
        );
    }
}


/* ===================== UI ===================== */

fn draw_ui<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<Rect> {
    let mut input_rect = Rect::default();

    terminal.draw(|f| {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),  // header
                Constraint::Length(3),  // status
                Constraint::Min(8),     // logs (FIXED)
                Constraint::Length(3),  // command
            ])
            .split(f.size());

        let header = Paragraph::new("OSMOGREP — AI E2E Testing Agent")
            .style(Style::default().add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));

        let status = Paragraph::new(format!(
            "Phase: {:?}\nBase: {}",
            state.phase,
            state.base_branch.clone().unwrap_or_else(|| "unknown".into())
        ))
        .block(Block::default().borders(Borders::ALL).title("Status"));

        let logs: Vec<Line> = state
            .logs
            .iter()
            .rev()
            .take(30)
            .rev()
            .map(|l| {
                let color = match l.level {
                    LogLevel::Info => Color::White,
                    LogLevel::Success => Color::Green,
                    LogLevel::Warn => Color::Yellow,
                    LogLevel::Error => Color::Red,
                };
                Line::from(Span::styled(&l.text, Style::default().fg(color)))
            })
            .collect();

        let log_view =
            Paragraph::new(logs).block(Block::default().borders(Borders::ALL).title("Execution"));

        let input_style = if state.input_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let input = Paragraph::new(format!("> {}", state.input))
            .style(input_style)
            .block(Block::default().borders(Borders::ALL).title("Command"));

        input_rect = layout[3];

        f.render_widget(header, layout[0]);
        f.render_widget(status, layout[1]);
        f.render_widget(log_view, layout[2]);
        f.render_widget(input, layout[3]);

        if state.input_focused {
            let x = input_rect.x + 2 + state.input.len() as u16;
            let y = input_rect.y + 1;
            f.set_cursor(x, y);
        }
    })?;

    Ok(input_rect)
}

/* ===================== COMMANDS ===================== */

fn handle_command(state: &mut AgentState, cmd: &str) {
    match cmd {
        "help" => log(
            state,
            LogLevel::Info,
            "Commands: exec | new | rollback | quit | help",
        ),
        "exec" => state.phase = Phase::ExecuteAgent,
        "new" => state.phase = Phase::CreateNewAgent,
        "rollback" => state.phase = Phase::Rollback,
        "quit" => state.phase = Phase::Done,
        _ => log(state, LogLevel::Warn, "Unknown command"),
    }
}

/* ===================== STATE MACHINE ===================== */

fn step(state: &mut AgentState) {
    match state.phase {
        Phase::Init => {
            if !is_git_repo() {
                log(state, LogLevel::Error, "Not a git repository.");
                state.phase = Phase::Done;
                return;
            }
            state.original_branch = Some(current_branch());
            state.phase = Phase::DetectBase;
        }

        Phase::DetectBase => {
            let base = detect_base_branch();
            state.base_branch = Some(base.clone());
            log(state, LogLevel::Success, format!("Base branch: {}", base));

            if let Some(agent) = find_existing_agent() {
                state.agent_branch = Some(agent.clone());
                log(state, LogLevel::Info, format!("Reusing {}", agent));
            }

            if working_tree_dirty() {
                log(state, LogLevel::Warn, "Uncommitted changes detected.");
            }

            state.phase = Phase::Idle;
        }

        Phase::Idle => {}

        Phase::CreateNewAgent => {
            let branch = create_agent_branch();
            state.agent_branch = Some(branch.clone());
            log(state, LogLevel::Success, format!("Created {}", branch));
            state.phase = Phase::Idle;
        }

        Phase::ExecuteAgent => {
            if let Some(branch) = state.agent_branch.clone() {
                checkout(&branch);
                apply_working_tree(state);
                log(state, LogLevel::Info, "Ready for tests.");
                log(state, LogLevel::Success, format!("Checked out {}", branch));
                log(state, LogLevel::Info, "Working tree applied. Ready for tests.");
            }
            state.phase = Phase::Idle;
        }

        Phase::Rollback => {
            if let Some(orig) = state.original_branch.clone() {
                checkout(&orig);
                log(state, LogLevel::Success, "Returned to original branch.");
            }
            state.phase = Phase::Idle;
        }

        Phase::Done => {}
    }
}

/* ===================== MAIN ===================== */

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut state = AgentState {
        phase: Phase::Init,
        base_branch: None,
        original_branch: None,
        agent_branch: None,
        input: String::new(),
        input_focused: true,
        logs: vec![],
    };

    loop {
        let input_rect = draw_ui(&mut terminal, &state)?;

        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(k) if state.input_focused => match k.code {
                    KeyCode::Char(c) => state.input.push(c),
                    KeyCode::Backspace => {
                        state.input.pop();
                    }
                    KeyCode::Enter => {
                        let cmd = state.input.trim().to_string();
                        state.input.clear();
                        handle_command(&mut state, &cmd);
                    }
                    _ => {}
                },

                Event::Mouse(m) => {
                    if let MouseEventKind::Down(_) = m.kind {
                        state.input_focused =
                            input_rect.contains((m.column, m.row).into());
                    }
                }

                _ => {}
            }
        }

        step(&mut state);

        if let Phase::Done = state.phase {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
