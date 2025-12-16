use std::{error::Error, io, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

#[derive(Clone)]
enum Phase {
    Init,
    GitSanity,
    DecideBranch,
    CreateAgentBranch,
    DetectBase,
    Done,
}

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
    logs: Vec<LogLine>,
}

/* ---------------- Logging ---------------- */

fn log(state: &mut AgentState, level: LogLevel, msg: impl Into<String>) {
    state.logs.push(LogLine {
        level,
        text: msg.into(),
    });
}

/* ---------------- Git helpers ---------------- */

fn is_git_repo() -> bool {
    std::path::Path::new(".git").exists()
}

fn is_working_tree_clean() -> bool {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg("git status --porcelain")
        .output()
        .expect("git status failed");

    out.stdout.is_empty()
}

fn current_branch() -> String {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg("git branch --show-current")
        .output()
        .expect("failed to get current branch");

    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn detect_base_branch() -> String {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg("git symbolic-ref refs/remotes/origin/HEAD")
        .output();

    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        if let Some(b) = s.trim().split('/').last() {
            return b.to_string();
        }
    }

    "master".to_string()
}

fn create_agent_branch() -> String {
    let branch = format!(
        "osmogrep/{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    );

    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("git checkout -q -b {}", branch))
        .status()
        .expect("failed to create agent branch");

    if !status.success() {
        panic!("Failed to create agent branch");
    }

    branch
}

/* ---------------- UI ---------------- */

fn draw_ui<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<()> {
    terminal.draw(|f| {
        let main = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Length(3), // status
                Constraint::Min(1),    // logs + footer
            ])
            .split(f.size());

        let header = Paragraph::new("OSMOGREP — AI E2E Testing Agent")
            .style(Style::default().add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));

        let phase_text = match state.phase {
            Phase::Init => "Initializing",
            Phase::GitSanity => "Checking git state",
            Phase::DecideBranch => "Deciding execution branch",
            Phase::CreateAgentBranch => "Creating agent branch",
            Phase::DetectBase => "Detecting base branch",
            Phase::Done => "Done",
        };

        let status = Paragraph::new(format!(
            "Phase: {}\nBase branch: {}",
            phase_text,
            state.base_branch.clone().unwrap_or_else(|| "unknown".into())
        ))
        .block(Block::default().borders(Borders::ALL).title("Status"));

        let bottom = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // logs
                Constraint::Length(1), // footer
            ])
            .split(main[2]);

        let log_lines: Vec<Line> = state
            .logs
            .iter()
            .rev()
            .take(12)
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

        let log_view = Paragraph::new(log_lines)
            .block(Block::default().borders(Borders::ALL).title("Execution Log"));

        let footer = Paragraph::new("q = quit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Right);

        f.render_widget(header, main[0]);
        f.render_widget(status, main[1]);
        f.render_widget(log_view, bottom[0]);
        f.render_widget(footer, bottom[1]);
    })
    .map(|_| ())
}

/* ---------------- Main ---------------- */

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AgentState {
        phase: Phase::Init,
        base_branch: None,
        original_branch: None,
        agent_branch: None,
        logs: vec![LogLine {
            level: LogLevel::Info,
            text: "Starting OsmoGrep agent…".into(),
        }],
    };

    loop {
        draw_ui(&mut terminal, &state)?;

        if event::poll(Duration::from_millis(400))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        match state.phase {
            Phase::Init => {
                log(&mut state, LogLevel::Info, "Checking git repository…");
                state.phase = Phase::GitSanity;
            }

            Phase::GitSanity => {
                if !is_git_repo() {
                    log(&mut state, LogLevel::Error, "Not inside a git repository.");
                    state.phase = Phase::Done;
                } else if !is_working_tree_clean() {
                    log(
                        &mut state,
                        LogLevel::Error,
                        "Working tree is dirty. Commit or stash changes.",
                    );
                    state.phase = Phase::Done;
                } else {
                    let cur = current_branch();
                    state.original_branch = Some(cur.clone());
                    log(
                        &mut state,
                        LogLevel::Success,
                        format!("Git clean. Current branch: {}", cur),
                    );
                    state.phase = Phase::DecideBranch;
                }
            }

            Phase::DecideBranch => {
                let cur = state.original_branch.clone().unwrap_or_default();
                if cur.starts_with("osmogrep/") {
                    log(
                        &mut state,
                        LogLevel::Warn,
                        "Already on agent branch. Reusing existing state.",
                    );
                    state.agent_branch = Some(cur);
                    state.phase = Phase::DetectBase;
                } else {
                    log(
                        &mut state,
                        LogLevel::Info,
                        "Creating isolated agent branch…",
                    );
                    log(
                        &mut state,
                        LogLevel::Info,
                        "↳ running: git checkout -b <agent-branch>",
                    );
                    state.phase = Phase::CreateAgentBranch;
                }
            }

            Phase::CreateAgentBranch => {
                let agent = create_agent_branch();
                state.agent_branch = Some(agent.clone());
                log(
                    &mut state,
                    LogLevel::Success,
                    format!("✔ Switched to agent branch {}", agent),
                );
                state.phase = Phase::DetectBase;
            }

            Phase::DetectBase => {
                let base = detect_base_branch();
                state.base_branch = Some(base.clone());
                log(
                    &mut state,
                    LogLevel::Success,
                    format!("Base branch detected: {}", base),
                );
                state.phase = Phase::Done;
            }

            Phase::Done => {}
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
