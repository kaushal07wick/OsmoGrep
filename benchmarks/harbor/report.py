"""Summarize Harbor trials and native events without turning missing data into success."""
import argparse
from collections import Counter
from datetime import datetime
import json
import math
from pathlib import Path


def trial_row(result, directory):
    rewards = (result.get("verifier_result") or {}).get("rewards") or {}
    reward = rewards.get("reward")
    if isinstance(reward, bool) or not isinstance(reward, (int, float)) or not math.isfinite(reward) or not 0 <= reward <= 1:
        reward = None
    outcome = ("error" if result.get("exception_info") or any(s.get("exception_info") for s in result.get("step_results") or [])
               else "ungraded" if reward is None else "pass" if reward == 1 else "fail")
    context = result.get("agent_result") or {}
    config = result.get("config") or {}
    settings = config.get("agent") or {}
    safe_settings = {key: settings.get(key) for key in ("name", "import_path", "model_name", "model_api_format", "override_timeout_sec", "max_timeout_sec")}
    safe_settings["kwargs"] = {key: value for key, value in settings.get("kwargs", {}).items() if key in ("version", "max_iterations", "reasoning_effort")}
    agent = result.get("agent_info") or {}
    timing = result.get("agent_execution") or {}
    seconds = None
    if timing.get("started_at") and timing.get("finished_at"):
        seconds = (datetime.fromisoformat(timing["finished_at"]) - datetime.fromisoformat(timing["started_at"])).total_seconds()
    events_file = directory / "agent" / "events.jsonl"
    events, incomplete = [], 0
    if events_file.exists():
        for line in events_file.read_text().splitlines():
            try:
                event = json.loads(line)
                if isinstance(event, dict):
                    events.append(event)
                else:
                    incomplete += 1
            except json.JSONDecodeError:
                incomplete += 1
    calls = [e for e in events if e.get("type") == "tool_call"]
    reads = [e["args"]["path"] for e in calls if e.get("name") == "read_file" and isinstance(e.get("args"), dict) and isinstance(e["args"].get("path"), str)]
    return {
        "task": result["task_name"], "task_checksum": result.get("task_checksum"),
        "agent": agent.get("name"), "agent_version": agent.get("version"),
        "model": agent.get("model_info"), "settings": safe_settings,
        "outcome": outcome, "reward": reward, "exception_type": (result.get("exception_info") or {}).get("exception_type"),
        "input_tokens": context.get("n_input_tokens"), "output_tokens": context.get("n_output_tokens"),
        "cached_input_tokens": context.get("n_cache_tokens"), "cost_usd": context.get("cost_usd"),
        "agent_seconds": seconds, "native_events_available": events_file.exists(),
        "tool_calls": len(calls) if events_file.exists() else None,
        "read_calls": len(reads) if events_file.exists() else None,
        "unique_read_targets": len(set(reads)) if events_file.exists() else None,
        "reported_model_responses": sum(e.get("type") == "usage" for e in events) if events_file.exists() else None,
        "completed_run_with_failed_grade": outcome == "fail" and any(e.get("type") == "done" for e in events),
        "invalid_event_lines": incomplete,
    }


def report(job_dir):
    rows = []
    for file in sorted(job_dir.glob("*/result.json")):
        value = json.loads(file.read_text())
        if "task_name" in value:
            rows.append(trial_row(value, file.parent))
    if not rows:
        raise ValueError("no Harbor trial results found")
    counts = Counter(row["outcome"] for row in rows)
    return {"scope": "Final trial results, including failed and ungraded trials. Pilot retries are disabled; this is not a matched-comparison ranking.",
            "attempts": len(rows), "outcomes": dict(counts), "trials": rows}


def check_contract(summary):
    rows = summary["trials"]
    expected = {"nop": "fail", "oracle": "pass", "osmogrep": "pass"}
    actual = {r["agent"]: r["outcome"] for r in rows}
    if len(rows) != 3 or actual != expected:
        raise ValueError(f"fixture controls failed: {actual}; expected {expected}")
    native = next(r for r in rows if r["agent"] == "osmogrep")
    if native["tool_calls"] != 2 or native["input_tokens"] != 360 or native["output_tokens"] != 90:
        raise ValueError("fixture did not preserve real tool delivery and provider usage")
    summary["scope"] = "Deterministic infrastructure fixture only. This is not a coding-success score."


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("job", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--check-contract", action="store_true")
    args = parser.parse_args()
    summary = report(args.job)
    if args.check_contract:
        check_contract(summary)
    args.output.write_text(json.dumps(summary, indent=2) + "\n")
    print(summary["scope"])
    print("Task | Agent | Outcome | Input/output tokens | Seconds")
    for row in summary["trials"]:
        print(f"{row['task']} | {row['agent']} | {row['outcome']} | {row['input_tokens']}/{row['output_tokens']} | {row['agent_seconds']}")


if __name__ == "__main__":
    main()
