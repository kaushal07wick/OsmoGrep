//! Shell commands reuse the same services as the terminal UI.
pub mod config;
pub mod lab;
mod models;
use clap::{Args, Subcommand};
pub use config::Args as ConfigArgs;
pub use models::{run as models, Args as ModelsArgs};
use std::{
    error::Error,
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

#[derive(Args)]
pub struct UpdateArgs {
    /// Show the installed and latest available version without installing
    #[arg(long, conflicts_with = "yes")]
    check: bool,
    /// Install an available update without asking
    #[arg(short, long)]
    yes: bool,
}
pub fn update(args: UpdateArgs) -> Result<(), Box<dyn Error>> {
    if std::env::current_exe().is_ok_and(|p| crate::updater::homebrew_managed(&p)) {
        println!(
            "Homebrew manages this installation. Run: brew upgrade kaushal07wick/tap/osmogrep"
        );
        return Ok(());
    }
    let Some(info) = crate::updater::check_for_update()? else {
        println!("OsmoGrep {} is up to date.", env!("CARGO_PKG_VERSION"));
        return Ok(());
    };
    println!(
        "OsmoGrep {} → {}",
        info.current_version, info.latest_version
    );
    for note in &info.notes {
        println!("  • {note}");
    }
    if args.check {
        return Ok(());
    }
    if !args.yes {
        if !io::stdin().is_terminal() {
            return Err(
                "An update is available. Run `osmogrep update --yes` to install it.".into(),
            );
        }
        print!("Install update? [y/N] ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Update deferred.");
            return Ok(());
        }
    }
    let interactive = io::stderr().is_terminal();
    let mut last = String::new();
    crate::updater::install_update(&info, |progress| {
        let text = match progress.total.filter(|total| *total > 0) {
            Some(total) => format!(
                "{} · {}%",
                progress.stage,
                (progress.received as f64 / total as f64 * 100.0).min(100.0) as u64
            ),
            None => progress.stage.into(),
        };
        if interactive {
            eprint!("\r\x1b[2K{text}");
            let _ = io::stderr().flush();
        } else if last != progress.stage {
            eprintln!("{}", progress.stage);
        }
        last = progress.stage.into();
    })?;
    if interactive {
        eprintln!();
    }
    println!(
        "Installed {}. Restart open OsmoGrep instances to use it.",
        info.latest_version
    );
    Ok(())
}

#[derive(Args)]
pub struct TeamArgs {
    /// Repository whose teammates or messages to inspect
    #[arg(long, default_value = ".", global = true)]
    repo_root: PathBuf,
    /// Include inactive sessions
    #[arg(long, global = true)]
    all: bool,
    /// Print complete structured records, including message receipts
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<TeamCommand>,
}
#[derive(Subcommand)]
enum TeamCommand {
    /// List teammates in this repository
    Peers,
    /// Read saved messages without registering a new agent or marking them read
    History {
        /// Session UUID from `osmogrep team --all`
        #[arg(long)]
        session: String,
        /// Load the page before this message ID
        #[arg(long)]
        before: Option<i64>,
    },
}
pub fn team(args: TeamArgs) -> Result<(), Box<dyn Error>> {
    let project = crate::team::project_key(&args.repo_root)?;
    let path = crate::persistence::config_dir().join("team.sqlite3");
    if !path.exists() {
        println!(
            "{}",
            if args.json {
                "[]"
            } else {
                "No team activity recorded in this repository."
            }
        );
        return Ok(());
    }
    let store = crate::team::Store::at(path)?;
    if let Some(TeamCommand::History { session, before }) = args.command {
        let history = store.history_for(&args.repo_root, &session, before)?;
        if args.json {
            println!("{}", serde_json::to_string_pretty(&history)?);
        } else {
            for entry in history.iter().rev() {
                let message = &entry["message"];
                let at = message["at"]
                    .as_i64()
                    .and_then(chrono::DateTime::from_timestamp_millis)
                    .map(|at| at.to_rfc3339())
                    .unwrap_or_default();
                println!(
                    "#{} · {} → {} · {at}\n{}\n",
                    message["id"],
                    clean(message["sender_title"].as_str().unwrap_or("Agent")),
                    clean(message["recipient"].as_str().unwrap_or("all agents")),
                    clean(message["body"].as_str().unwrap_or(""))
                );
            }
            if history.len() == 50 {
                println!("Older page: --before {}", history[0]["message"]["id"]);
            }
            if history.is_empty() {
                println!("No messages for this session.");
            }
        }
    } else {
        let peers = store
            .peers(Some(&project))?
            .into_iter()
            .filter(|peer| args.all || peer.online)
            .collect::<Vec<_>>();
        if args.json {
            println!("{}", serde_json::to_string_pretty(&peers)?);
        } else {
            for peer in &peers {
                println!(
                    "{} · {} · {} · {} / {} · {}",
                    clean(&crate::team::presence_name(peer)),
                    peer.session,
                    if peer.online {
                        peer.status.as_str()
                    } else {
                        "inactive"
                    },
                    clean(&peer.provider),
                    clean(&peer.model),
                    clean(&peer.workspace)
                );
            }
            if peers.is_empty() {
                println!("No live teammates. Use --all to include inactive sessions.");
            }
        }
    }
    Ok(())
}
pub(crate) fn clean(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .collect()
}
