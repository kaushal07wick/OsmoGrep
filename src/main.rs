use std::{error::Error, io, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
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

struct AgentState {
    phase: Phase,
    base_branch: Option<String>,
    original_branch: Option<String>,
    agent_branch: Option<String>,
    message: String,
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
        return s.trim().split('/').last().unwrap_or("master").to_string();
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
        .arg(format!("git checkout -b {}", branch))
        .status()
        .expect("failed to create agent branch");

    if !status.success() {
        panic!("❌ Failed to create agent branch");
    }

    branch
}

/* ---------------- UI ---------------- */

fn draw_ui<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<()> {
    terminal
        .draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(7),
                    Constraint::Min(3),
                ])
                .split(f.size());

            let header = Paragraph::new("OSMOGREP — AI E2E Testing Agent")
                .style(Style::default().add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::ALL));

            let phase = match state.phase {
                Phase::Init => "Initializing",
                Phase::GitSanity => "Checking git state",
                Phase::DecideBranch => "Deciding execution branch",
                Phase::CreateAgentBranch => "Creating agent branch",
                Phase::DetectBase => "Detecting base branch",
                Phase::Done => "Done",
            };

            let body = Paragraph::new(format!(
                "Phase: {}\nBase branch: {}\n\n{}",
                phase,
                state
                    .base_branch
                    .clone()
                    .unwrap_or_else(|| "unknown".into()),
                state.message
            ))
            .block(Block::default().borders(Borders::ALL).title("Agent State"));

            let footer = Paragraph::new("Press q to quit")
                .block(Block::default().borders(Borders::ALL));

            f.render_widget(header, chunks[0]);
            f.render_widget(body, chunks[1]);
            f.render_widget(footer, chunks[2]);
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
        message: "Starting OsmoGrep agent…".into(),
    };

    loop {
        draw_ui(&mut terminal, &state)?;

        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        match state.phase {
            Phase::Init => {
                state.phase = Phase::GitSanity;
                state.message = "Checking git repository state…".into();
            }

            Phase::GitSanity => {
                if !is_git_repo() {
                    state.message = "❌ Not inside a git repository.".into();
                    state.phase = Phase::Done;
                } else if !is_working_tree_clean() {
                    state.message =
                        "❌ Working tree is dirty. Commit or stash changes.".into();
                    state.phase = Phase::Done;
                } else {
                    let current = current_branch();
                    state.original_branch = Some(current.clone());
                    state.message = format!("Git state verified. Current branch: {}", current);
                    state.phase = Phase::DecideBranch;
                }
            }

            Phase::DecideBranch => {
                let current = state.original_branch.clone().unwrap_or_default();

                if current.starts_with("osmogrep/") {
                    state.agent_branch = Some(current.clone());
                    state.message =
                        "Already on an OsmoGrep agent branch. Reusing it.".into();
                    state.phase = Phase::DetectBase;
                } else {
                    state.message =
                        "No agent branch detected. Creating a new one.".into();
                    state.phase = Phase::CreateAgentBranch;
                }
            }

            Phase::CreateAgentBranch => {
                let agent = create_agent_branch();
                state.agent_branch = Some(agent.clone());
                state.message = format!("Agent branch created: {}", agent);
                state.phase = Phase::DetectBase;
            }

            Phase::DetectBase => {
                let base = detect_base_branch();
                state.base_branch = Some(base);
                state.message = "Base branch detected successfully.".into();
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
