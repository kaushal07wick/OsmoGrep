// src/testgen/suite.rs — runs full test suite asynchronously and writes markdown report

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::state::{AgentEvent, AgentState, LogLevel};
use crate::testgen::runner::{
    run_full_test_async, TestCaseResult, TestOutcome, TestSuiteResult,
};

#[derive(Debug, Default)]
struct ParsedSummary {
    passed: usize,
    failed: usize,
    skipped: usize,
    warnings: usize,
    subtests: usize,
    duration_s: Option<f64>,
}

pub fn run_full_test_suite(
    state: &AgentState,
    repo_root: PathBuf,
) -> io::Result<()> {
    let language = state
        .lifecycle
        .language
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "language not detected"))?;

    let tx = state.agent_tx.clone();

    run_full_test_async(language, move |suite| {
        let (summary, _) = parse_pytest_output_fully(&suite.raw_output);

        let failed = summary.failed > 0;
        let level = if failed { LogLevel::Error } else { LogLevel::Success };
        let status = if failed { "FAILED" } else { "PASSED" };

        match write_test_suite_report(&repo_root, &suite) {
            Ok(path) => {
                let _ = tx.send(AgentEvent::Log(
                    level,
                    format!(
                        "Test suite finished [{}] → report written to {}",
                        status,
                        path.display()
                    ),
                ));
            }
            Err(e) => {
                let _ = tx.send(AgentEvent::Log(
                    LogLevel::Error,
                    format!(
                        "Test suite finished [{}] but failed to write report: {e}",
                        status
                    ),
                ));
            }
        }

        let _ = tx.send(AgentEvent::Finished);
    });

    Ok(())
}

/* ---------- Parsing ---------- */

fn parse_pytest_output_fully(raw: &str) -> (ParsedSummary, Vec<TestCaseResult>) {
    let mut summary = ParsedSummary::default();
    let mut cases = Vec::new();
    let mut footer_line: Option<String> = None;

    for line in raw.lines() {
        let line = line.trim();

        if line.starts_with('=') && line.ends_with('=') && line.contains(" in ") {
            footer_line = Some(line.to_string());
            continue;
        }

        let line = match line.split_once(" [") {
            Some((l, _)) => l.trim(),
            None => line,
        };

        let (left, status) = match line.rsplit_once(' ') {
            Some(v) => v,
            None => continue,
        };

        let outcome = match status {
            "PASSED" | "SUBPASSED" => TestOutcome::Pass,
            "FAILED" => TestOutcome::Fail,
            "SKIPPED" => TestOutcome::Skip,
            _ => continue,
        };

        let (file, name) = match left.split_once("::") {
            Some(v) => (v.0.to_string(), v.1.to_string()),
            None => continue,
        };

        cases.push(TestCaseResult {
            file,
            name,
            outcome,
            note: None,
        });
    }

    if let Some(line) = footer_line {
        parse_footer_summary_line(&line, &mut summary);
    } else {
        for c in &cases {
            match c.outcome {
                TestOutcome::Pass => summary.passed += 1,
                TestOutcome::Fail => summary.failed += 1,
                TestOutcome::Skip => summary.skipped += 1,
                _ => {}
            }
        }
    }

    (summary, cases)
}

fn parse_footer_summary_line(line: &str, out: &mut ParsedSummary) {
    let inner = line.trim_matches('=').trim();

    let (counts, duration) = match inner.rsplit_once(" in ") {
        Some(v) => v,
        None => return,
    };

    out.duration_s = duration.trim_end_matches('s').parse::<f64>().ok();

    for token in counts.split(',').map(str::trim) {
        let mut parts = token.split_whitespace();
        let Some(num) = parts.next().and_then(|n| n.parse::<usize>().ok()) else {
            continue;
        };

        let label = parts.collect::<Vec<_>>().join(" ");

        match label.as_str() {
            "passed" => out.passed = num,
            "failed" => out.failed = num,
            "skipped" => out.skipped = num,
            "warnings" => out.warnings = num,
            "subtests passed" => out.subtests = num,
            _ => {}
        }
    }
}

/* ---------- Report Writing ---------- */

pub fn write_test_suite_report(
    repo_root: &Path,
    suite: &TestSuiteResult,
) -> io::Result<PathBuf> {
    let report_path = repo_root.join("test_suite_results_formatted.md");
    let mut out = File::create(&report_path)?;

    let (summary, cases) = parse_pytest_output_fully(&suite.raw_output);

    writeln!(out, "# 🧪 Test Suite Report\n")?;

    let status = if summary.failed > 0 { "❌ FAILED" } else { "✅ PASSED" };

    writeln!(out, "## Test Summary")?;
    writeln!(out, "**Status:** {}\n", status)?;
    writeln!(
        out,
        "- ✅ Passed: {}\n- ❌ Failed: {}\n- ⏭ Skipped: {}\n- ⚠️ Warnings: {}\n- 🧩 Subtests: {}\n- ⏱ Duration: {:.2}s\n",
        summary.passed,
        summary.failed,
        summary.skipped,
        summary.warnings,
        summary.subtests,
        summary.duration_s.unwrap_or(0.0),
    )?;

    let mut by_file: BTreeMap<String, Vec<&TestCaseResult>> = BTreeMap::new();
    for c in &cases {
        by_file.entry(c.file.clone()).or_default().push(c);
    }

    for (file_name, cases) in by_file {
        let passed = cases.iter().filter(|c| matches!(c.outcome, TestOutcome::Pass)).count();
        let failed = cases.iter().filter(|c| matches!(c.outcome, TestOutcome::Fail)).count();
        let skipped = cases.iter().filter(|c| matches!(c.outcome, TestOutcome::Skip)).count();

        let file_status = if failed > 0 { "❌ FAILED" } else { "✅ PASSED" };

        writeln!(
            out,
            "### {}\n{} ({} passed, {} failed, {} skipped)\n",
            file_name, file_status, passed, failed, skipped
        )?;

        for c in cases {
            let (icon, label) = match c.outcome {
                TestOutcome::Pass => ("✅", "PASS"),
                TestOutcome::Fail => ("❌", "FAIL"),
                TestOutcome::Skip => ("⏭", "SKIP"),
                TestOutcome::Warning => ("⚠️", "WARN"),
            };

            writeln!(out, "- {} **{}** —→ {}", icon, c.name, label)?;
        }

        writeln!(out)?;
    }

    Ok(report_path)
}
