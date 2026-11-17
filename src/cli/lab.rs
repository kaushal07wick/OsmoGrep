//! Explicit Lab actions share the agent service; reading a report never executes it.
use clap::{Args, Subcommand};
use serde_json::json;
use std::path::PathBuf;
#[derive(Args)]
pub struct LabArgs {
    #[arg(long, default_value = ".", global = true)]
    repo_root: PathBuf,
    #[command(subcommand)]
    command: LabCommand,
}
#[derive(Subcommand)]
enum LabCommand {
    /// Read exact JSON request examples without opening storage or executing anything
    Guide { kind: Option<String> },
    /// Execute an explicit Lab JSON request; unlike create, this may run experiments
    Execute { spec: PathBuf },
    /// Accept the inspected contract as local-user; optionally subscribe to drift notices
    AcceptContract {
        id: String,
        #[arg(long)]
        definition_sha256: String,
        #[arg(long)]
        subscribe: bool,
    },
    /// Check source drift and acceptance without modifying the contract
    CheckContract { id: String },
    /// Prepare a comparison of two immutable replay candidates
    Compare {
        left: String,
        right: String,
        #[arg(long, default_value = "Compare candidate fixes")]
        title: String,
    },
    /// Run both candidates through the same isolated VM server
    RunComparison {
        id: String,
        #[arg(long)]
        server: String,
        #[arg(long, default_value_t = 120)]
        timeout_seconds: u64,
    },
    /// Apply an explicitly chosen replay candidate if its original files still match
    Apply { id: String },
    /// List saved reports and experiments in this project
    List {
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        before: Option<i64>,
    },
    /// Read a saved report without executing its contents
    Show { id: String },
    /// Create a report from a JSON request (use - to read stdin)
    Create { spec: PathBuf },
    /// Export selected replay evidence to a new workspace file
    Export { id: String, path: String },
    /// Inspect and import a replay package without running it
    Import { path: String },
    /// Execute a captured reproducer through an isolated VM server
    Replay {
        id: String,
        #[arg(long)]
        server: String,
        #[arg(long, default_value_t = 120)]
        timeout_seconds: u64,
    },
}
pub fn run(args: LabArgs) -> Result<(), Box<dyn std::error::Error>> {
    let value = match args.command {
        LabCommand::Guide { kind } => json!({"action":"help","kind":kind}),
        LabCommand::Execute { spec } => read_spec(&spec)?,
        LabCommand::AcceptContract {
            id,
            definition_sha256,
            subscribe,
        } => {
            json!({"action":"accept_contract","id":id,"data":{"definition_sha256":definition_sha256,"subscribe":subscribe}})
        }
        LabCommand::CheckContract { id } => json!({"action":"check_contract","id":id}),
        LabCommand::Compare { left, right, title } => {
            json!({"action":"plan_comparison","title":title,"data":{"left":left,"right":right}})
        }
        LabCommand::RunComparison {
            id,
            server,
            timeout_seconds,
        } => {
            json!({"action":"run_comparison","id":id,"server":server,"timeout_seconds":timeout_seconds})
        }
        LabCommand::Apply { id } => json!({"action":"apply","id":id}),
        LabCommand::List { kind, before } => json!({"action":"list","kind":kind,"before":before}),
        LabCommand::Show { id } => json!({"action":"show","id":id}),
        LabCommand::Export { id, path } => json!({"action":"export","id":id,"path":path}),
        LabCommand::Import { path } => json!({"action":"import","path":path}),
        LabCommand::Replay {
            id,
            server,
            timeout_seconds,
        } => json!({"action":"replay","id":id,"server":server,"timeout_seconds":timeout_seconds}),
        LabCommand::Create { spec } => {
            let value = read_spec(&spec)?;
            if !matches!(
                value["action"].as_str(),
                Some(
                    "capture_replay"
                        | "create_contract"
                        | "amend_contract"
                        | "plan_flow"
                        | "plan_scenario"
                        | "correct_context"
                )
            ) {
                return Err("Create accepts replay, contract, flow, scenario and correction metadata; use an explicit execution command to run experiments".into());
            }
            value
        }
    };
    let request = serde_json::from_value(value)?;
    let result = crate::tools::in_root(&args.repo_root, || {
        crate::lab::api::execute_as_user(&args.repo_root, request, &|| false)
    })?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    if result["data"]["state"] == "failed" || result.get("error").is_some() {
        return Err("Lab execution failed; report saved above".into());
    }
    Ok(())
}

fn read_spec(spec: &std::path::Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let bytes = if spec.as_os_str() == "-" {
        use std::io::Read;
        let mut bytes = vec![];
        std::io::stdin()
            .take(crate::lab::MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        bytes
    } else {
        crate::lab::bounded_file(spec, crate::lab::MAX_BYTES)?
    };
    if bytes.len() > crate::lab::MAX_BYTES {
        return Err("Lab request exceeds 8 MiB".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}
