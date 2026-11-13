//! A root-scoped stdio MCP surface. It uses the same graph, policy and verification code as CLI.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{BufRead, Read, Write},
    path::Path,
};

use super::{
    git,
    predict::{self, Policy},
    Result,
};
use crate::{context::graph::Store, tools::Tool};

pub fn tools() -> Value {
    let schema = |name: &str, description: &str, properties: Value, required: Value| {
        json!({"name":name,"description":description,
        "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
    };
    json!([
        schema(
            "query_code_graph",
            "Read bounded revision-aware structural context.",
            json!({"revision":{"type":"string"},"seeds":{"type":"array","items":{"type":"string"}},"resource_history":{"type":"string"},"depth":{"type":"integer"},"limit":{"type":"integer"}}),
            json!(["revision"])
        ),
        schema(
            "analyze_change_impact",
            "Read affected graph neighborhoods for two commits.",
            json!({"base":{"type":"string"},"candidate":{"type":"string"}}),
            json!(["base", "candidate"])
        ),
        schema(
            "predict_runtime_regressions",
            "Screen and selectively predict; zero automatic execution.",
            json!({"base":{"type":"string"},"candidate":{"type":"string"}}),
            json!(["base", "candidate"])
        ),
        schema(
            "analyze_pr_interactions",
            "Analyze one related pair of fetched PR commit references.",
            json!({"left":{"type":"string"},"right":{"type":"string"}}),
            json!(["left", "right"])
        ),
        schema(
            "verify_runtime_hypothesis",
            "Submit an explicit experiment to an authorized isolated-execution bridge.",
            json!({"run_id":{"type":"string"},"experiment":{"type":"object"}}),
            json!(["run_id", "experiment"])
        ),
        schema(
            "get_regression_report",
            "Retrieve an immutable report and associated verification jobs.",
            json!({"run_id":{"type":"string"}}),
            json!(["run_id"])
        )
    ])
}

pub fn call(
    root: &Path,
    name: &str,
    args: Value,
    policy: &Policy,
    sandbox: Option<&str>,
) -> Result<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Change {
        base: String,
        candidate: String,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Pair {
        left: String,
        right: String,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ReportId {
        run_id: String,
    }
    match name {
        "query_code_graph" => crate::tools::CodeGraph {
            root: root.to_path_buf(),
        }
        .call(args)
        .map_err(Into::into),
        "analyze_change_impact" => {
            let change: Change = serde_json::from_value(args)?;
            let base = git::revision(root, &change.base)?;
            let candidate = git::revision(root, &change.candidate)?;
            let paths = git::changed(root, &base, &candidate)?;
            Ok(
                json!({"base":base,"candidate":candidate,"graph":Store::read(root)?.neighborhood(&candidate,&paths,2,policy.max_graph_nodes)?}),
            )
        }
        "predict_runtime_regressions" => {
            let change: Change = serde_json::from_value(args)?;
            Ok(serde_json::to_value(predict::analyze(
                root,
                &change.base,
                &change.candidate,
                policy.clone(),
                &|| false,
            )?)?)
        }
        "analyze_pr_interactions" => {
            let pair: Pair = serde_json::from_value(args)?;
            Ok(serde_json::to_value(predict::analyze_pair(
                root,
                &pair.left,
                &pair.right,
                policy.clone(),
                &|| false,
            )?)?)
        }
        "get_regression_report" => {
            let report: ReportId = serde_json::from_value(args)?;
            Store::read(root)?.report(&report.run_id)
        }
        "verify_runtime_hypothesis" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Verify {
                run_id: String,
                experiment: super::verify::Experiment,
            }
            let request: Verify = serde_json::from_value(args)?;
            let server = sandbox.ok_or(
                "server was started without an authorized --sandbox bridge; verification disabled",
            )?;
            let mut report: predict::Report =
                serde_json::from_value(Store::read(root)?.report(&request.run_id)?)?;
            // Server limits may tighten a stored report, never expand it.
            report.policy.max_experiments =
                report.policy.max_experiments.min(policy.max_experiments);
            report.policy.experiment_timeout_secs = report
                .policy
                .experiment_timeout_secs
                .min(policy.experiment_timeout_secs);
            if matches!(policy.mode, predict::Mode::Off | predict::Mode::Screen) {
                return Err("server policy disables verification".into());
            }
            super::cli::verify_report(root, &mut report, request.experiment, server, &|| false)
        }
        _ => Err("unknown intelligence tool".into()),
    }
}

pub fn serve(root: &Path, policy: &Policy, sandbox: Option<&str>) -> Result<()> {
    serve_io(
        root,
        policy,
        sandbox,
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    )
}

fn serve_io(
    root: &Path,
    policy: &Policy,
    sandbox: Option<&str>,
    mut input: impl BufRead,
    mut output: impl Write,
) -> Result<()> {
    policy.validate()?;
    loop {
        let mut line = Vec::new();
        let read = input
            .by_ref()
            .take(1024 * 1024 + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        if line.len() > 1024 * 1024 {
            return Err("MCP request exceeds 1 MiB".into());
        }
        let request: Value = match serde_json::from_slice(&line) {
            Ok(value) => value,
            Err(_) => {
                writeln!(
                    output,
                    "{}",
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"invalid JSON"}})
                )?;
                output.flush()?;
                continue;
            }
        };
        let Some(id) = request.get("id") else {
            continue;
        };
        let response = match request["method"].as_str().unwrap_or("") {
            "initialize" => {
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"osmogrep-intelligence","version":env!("CARGO_PKG_VERSION")}})
            }
            "ping" => json!({}),
            "tools/list" => json!({"tools":tools()}),
            "tools/call" => match call(
                root,
                request["params"]["name"].as_str().unwrap_or(""),
                request["params"]["arguments"].clone(),
                policy,
                sandbox,
            ) {
                Ok(value) => {
                    json!({"content":[{"type":"text","text":serde_json::to_string(&value)?}]})
                }
                Err(error) => {
                    json!({"isError":true,"content":[{"type":"text","text":error.to_string()}]})
                }
            },
            _ => {
                writeln!(
                    output,
                    "{}",
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"method not found"}})
                )?;
                output.flush()?;
                continue;
            }
        };
        writeln!(
            output,
            "{}",
            json!({"jsonrpc":"2.0","id":id,"result":response})
        )?;
        output.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stdio_initialization_and_tool_listing_use_jsonrpc() {
        let requests=b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n";
        let mut out = vec![];
        serve_io(
            Path::new("."),
            &Policy::default(),
            None,
            &requests[..],
            &mut out,
        )
        .unwrap();
        let values: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(
            values[0]["result"]["serverInfo"]["name"],
            "osmogrep-intelligence"
        );
        assert_eq!(values[1]["result"]["tools"].as_array().unwrap().len(), 6);
        assert!(call(Path::new("."),"verify_runtime_hypothesis",json!({"run_id":"x","experiment":{
            "path":"osmogrep_repro_x.py","source":"pass","command":"python x","oracle":"contract","oracle_sources":["spec"]
        }}),&Policy::default(),None).is_err());
    }
}
