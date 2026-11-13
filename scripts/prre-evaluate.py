#!/usr/bin/env python3
"""Prepare controlled Git history and evaluate analysis, never execute fixture code on the host."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

try:
    import resource
except ImportError:
    resource = None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN) if resource else None
    binary = args.binary.resolve(strict=True)
    output = args.output_dir or Path(tempfile.mkdtemp(prefix="osmogrep-prre-evaluation-"))
    output.mkdir(parents=True, exist_ok=True)
    repo = output.resolve() / "repository"
    if repo.exists():
        raise SystemExit("output repository already exists; use a new directory")
    fixture = Path(__file__).resolve().parents[1] / "fixtures/prre/pool"
    shutil.copytree(fixture, repo)

    def git(*arguments):
        return subprocess.check_output(
            ["git", "-c", "core.hooksPath=/dev/null", "-C", str(repo), *arguments],
            text=True, stderr=subprocess.PIPE, timeout=30,
        ).strip()

    def commit(message):
        git("add", ".")
        git("-c", "user.name=OsmoGrep evaluation", "-c", "user.email=evaluation@localhost",
            "commit", "-qm", message)
        return git("rev-parse", "HEAD")

    def analyze(*arguments):
        started = time.monotonic()
        result = subprocess.run(
            [str(binary), "predict", "--repo-root", str(repo), *arguments],
            text=True, capture_output=True, check=True, timeout=60,
        )
        return json.loads(result.stdout), round((time.monotonic() - started) * 1000, 2)

    git("init", "-q")
    (repo / ".gitignore").write_text(".context/\n__pycache__/\nosmogrep_repro_*\n")
    base = commit("independent contract and existing smoke tests")
    runtime = repo / "osmogrep.runtime.toml"
    original = runtime.read_text()
    runtime.write_text(original.replace("capacity = 16", "capacity = 8"))
    left = commit("PR A: reduce resource pool")
    git("checkout", "--detach", base)
    runtime.write_text(original.replace("concurrency = 8", "concurrency = 16"))
    right = commit("PR B: increase worker concurrency")
    git("checkout", "--detach", base)
    (repo / "README.md").write_text("Documentation-only negative case.\n")
    docs = commit("documentation-only negative")
    indexing = []
    for label in ["cold", "warm"]:
        started = time.monotonic()
        result = subprocess.run([str(binary), "index", "--repo-root", str(repo), "--commit", base],
                                text=True, capture_output=True, check=True, timeout=60)
        stats = json.loads(result.stdout)
        indexing.append({"pass": label, "latency_ms": round((time.monotonic() - started) * 1000, 2), **stats})
    assert indexing[1]["parsed_files"] == 0
    assert indexing[1]["reused_files"] == indexing[0]["parsed_files"]
    cases = []
    for name, arguments, expected in [
        ("docs", ["--base", base, "--commit", docs], 0),
        ("pool_alone", ["--base", base, "--commit", left], 0),
        ("workers_alone", ["--base", base, "--commit", right], 0),
        ("interacting_pair", ["--left", left, "--right", right], 1),
        ("irrelevant_pair", ["--left", left, "--right", docs], 0),
    ]:
        report, elapsed = analyze(*arguments)
        assert len(report["hypotheses"]) == expected, (name, report)
        assert report["usage"]["model_calls"] == report["usage"]["sandbox_experiments"] == 0
        if name == "docs":
            assert report["screening"]["risk"] == "skipped"
        (output / f"{name}.json").write_text(json.dumps(report, indent=2) + "\n")
        cases.append({"case": name, "expected_hypotheses": expected,
                      "actual_hypotheses": len(report["hypotheses"]), "latency_ms": elapsed})
        if name == "interacting_pair":
            combined = report["candidate"]
    # Prepare a real base/candidate difference for the existing VM service's working-tree API.
    git("checkout", "--detach", base)
    git("diff", "--quiet")
    combined_source = git("show", f"{combined}:osmogrep.runtime.toml")
    runtime.write_text(combined_source + "\n")
    experiment = {
        "path": "osmogrep_repro_pool.py",
        "source": "from worker import run_batch\n\nassert run_batch(), 'CONTRACT.md:3: every worker acquires during overlap'\n",
        "command": "PYTHONDONTWRITEBYTECODE=1 python3 osmogrep_repro_pool.py",
        "oracle": "CONTRACT.md:3 requires all configured workers to acquire during overlap.",
        "oracle_sources": ["CONTRACT.md:3"],
    }
    (output / "experiment.json").write_text(json.dumps(experiment, indent=2) + "\n")
    db = repo / ".context/intelligence/graph.sqlite3"
    usage_after = resource.getrusage(resource.RUSAGE_CHILDREN) if resource else None
    summary = {"schema_version": 1, "kind": "controlled_analysis_evaluation",
               "repository": str(repo), "base": base, "pr_a": left, "pr_b": right,
               "combined": combined, "cases": cases, "sample_size": len(cases),
               "indexing": indexing,
               "graph_bytes": sum(p.stat().st_size for p in db.parent.glob("graph.sqlite3*")),
               "model_calls": 0, "model_tokens": 0, "sandbox_experiments": 0,
               "cpu_user_seconds": round(usage_after.ru_utime - usage_before.ru_utime, 6) if resource else None,
               "cpu_system_seconds": round(usage_after.ru_stime - usage_before.ru_stime, 6) if resource else None,
               "peak_child_rss_bytes": usage_after.ru_maxrss * (1 if sys.platform == "darwin" else 1024) if resource else None,
               "runtime_verification": "pending: use authorized OsmoGrep VM service with experiment.json",
               "limitations": ["Synthetic cases from one controlled repository; not an accuracy estimate on public repositories.",
                               "No runtime code, original suite, model or production experiment executed by this script."]}
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
