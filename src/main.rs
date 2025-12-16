use std::{error::Error, io, time::Duration};

use clap::Parser;
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

#[derive(Parser)]
struct Args {
    /// Feature branch to analyze
    branch: String,
}

#[derive(Clone)]
enum Phase {
    Init,
    DetectBase,
    Done,
    GitSanity,
}

struct AgentState {
    phase: Phase,
    base_branch: Option<String>,
    message: String,
}

fn detect_base_branch() -> String {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("git symbolic-ref refs/remotes/origin/HEAD")
        .output();

    if let Ok(out) = output {
        let s = String::from_utf8_lossy(&out.stdout);
        return s.trim().split('/').last().unwrap_or("master").to_string();
    }

    "master".to_string()
}

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

fn branch_exists(branch: &str) -> bool {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("git show-ref --verify --quiet refs/heads/{}", branch))
        .output();

    out.map(|o| o.status.success()).unwrap_or(false)
}


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

            let header = Paragraph::new("OSMOGREP — AI E2E Testing CLI Agent")
                .style(Style::default().add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::ALL));

            let phase = match state.phase {
                Phase::Init => "Initializing",
                Phase::DetectBase => "Detecting base branch",
                Phase::GitSanity => "Checking git repo for state..",
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


fn main() -> Result<(), Box<dyn Error>> {
    let _args = Args::parse();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AgentState {
        phase: Phase::Init,
        base_branch: None,
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
                state.message = "Checking git repo state..".into();
            }
            Phase::GitSanity => {
                if !is_git_repo() {
                    state.message ="❌ Not inside a git repository.".into();
                    state.phase = Phase::Done;
                } else if !is_working_tree_clean() {
                    state.message = "❌ Working tree is dirty. commit or stash changes..".into();
                    state.phase = Phase::Done;
                } else if !branch_exists(&_args.branch){
                    state.message = format!("❌ Branch '{}' does not exist.", _args.branch);
                    state.phase = Phase::Done;
                } else {
                    state.message = "Git state Verified.".into();
                    state.phase = Phase::DetectBase;
                }
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
