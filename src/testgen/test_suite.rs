use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::detectors::language::Language;
use crate::testgen::runner::{
    run_test_suite, TestCaseResult, TestOutcome, TestSuiteResult,
};
use std::collections::HashSet;

#[derive(Debug)]
pub struct TestSuiteExecution {
    pub passed: bool,
    pub failures: Vec<TestSuiteFailure>,
    pub report_path: PathBuf,
    pub raw_output: String,
}

impl TestSuiteExecution {
    pub fn failed_tests(&self) -> &[TestSuiteFailure] {
        &self.failures
    }

    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }
}


#[derive(Debug)]
pub struct TestSuiteFailure {
    pub file: String,
    pub test: String,
    pub output: String,
}

#[derive(Debug, Default)]
struct ParsedSummary {
    passed: usize,
    failed: usize,
    skipped: usize,
    warnings: usize,
    subtests: usize,
    duration_s: Option<f64>,
}

#[derive(Serialize)]
struct IndexMeta {
    runner: &'static str,
    language: &'static str,
    invocation: &'static str,
    working_directory: String,
    generated_at: String,
}

#[derive(Serialize, Clone)]
struct RawSpan {
    start: usize,
    end: usize,
}

#[derive(Serialize, Clone)]
struct FailureSpan {
    start: usize,
    end: usize,
}

#[derive(Serialize, Clone)]
struct WarningEntry {
    message: String,
    file: Option<String>,
    line: Option<usize>,
}

#[derive(Serialize, Clone)]
struct SkipEntry {
    file: String,
    line: usize,
    reason: String,
    count: usize,
}

#[derive(Serialize, Clone)]
struct IndexedTest {
    file: String,
    name: String,
    outcome: String,
    raw_span: RawSpan,
    failure_span: Option<FailureSpan>,
    duration_s: Option<f64>,
}

#[derive(Serialize)]
struct FileIndex {
    passed: usize,
    failed: usize,
    skipped: usize,
    tests: Vec<IndexedTest>,
}

#[derive(Serialize)]
struct IndexSummary {
    passed: usize,
    failed: usize,
    skipped: usize,
    warnings: usize,
    total: usize,
}

#[derive(Serialize)]
struct IndexFile {
    meta: IndexMeta,
    summary: IndexSummary,
    files: BTreeMap<String, FileIndex>,
    tests: Vec<IndexedTest>,
    skips: Vec<SkipEntry>,
    warnings: Vec<WarningEntry>,
}

pub fn run_test_suite_and_report(
    language: Language,
    repo_root: &Path,
) -> io::Result<TestSuiteExecution> {
    let suite = run_test_suite(language);

    let (summary, cases) = parse_pytest_output_fully(&suite.raw_output);
    let report_path = write_test_suite_report(repo_root, &suite)?;
    
    let mut seen = HashSet::new();
    let failures = extract_verbose_failures(&suite.raw_output)
        .into_iter()
        .filter_map(|(full_name, output)| {
            if !seen.insert(full_name.clone()) {
                return None;
            }

            let (file, test) = full_name
                .split_once("::")
                .unwrap_or(("", &full_name));

            Some(TestSuiteFailure {
                file: file.to_string(),
                test: test.to_string(),
                output,
            })
        })
        .collect::<Vec<_>>();


    Ok(TestSuiteExecution {
        passed: summary.failed == 0,
        failures,
        report_path,
        raw_output: suite.raw_output,
    })
}

fn parse_pytest_output_fully(raw: &str) -> (ParsedSummary, Vec<TestCaseResult>) {
    let mut summary = ParsedSummary::default();
    let mut cases = Vec::new();
    let mut footer_line: Option<String> = None;

    // First pass: collect test results from the verbose output
    for line in raw.lines() {
        let line = line.trim();

        // Capture the footer summary line
        if line.starts_with('=') && line.ends_with('=') && line.contains(" in ") {
            footer_line = Some(line.to_string());
            continue;
        }

        // Skip lines that don't look like test results
        if !line.contains("::") {
            continue;
        }

        // Remove percentage indicator if present (e.g., "test.py::test_foo [100%]")
        let line = match line.split_once(" [") {
            Some((l, _)) => l.trim(),
            None => line,
        };

        // Split on last space to get status
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

        // Extract file and test name
        let (file, name) = match left.split_once("::") {
            Some((file_part, name_part)) => {
                // Skip subtest details - these often have " - " in them indicating assertion details
                // Example: "test_foo - assert x == y" should be skipped
                if name_part.contains(" - ") {
                    continue;
                }
                (file_part.to_string(), name_part.to_string())
            },
            None => continue,
        };

        cases.push(TestCaseResult {
            file,
            name,
            outcome,
            note: None,
        });
    }

    // Second pass: also check the "short test summary" section for additional failures
    // This catches failures that might not appear in the main output
    let failed_tests = extract_failed_tests(raw);
    for failed_full_name in failed_tests {
        if let Some((file, name)) = failed_full_name.split_once("::") {
            // Only add if not already in cases
            if !cases.iter().any(|c| c.file == file && c.name == name) {
                cases.push(TestCaseResult {
                    file: file.to_string(),
                    name: name.to_string(),
                    outcome: TestOutcome::Fail,
                    note: None,
                });
            }
        }
    }

    // Parse the footer summary line for accurate counts
    if let Some(line) = footer_line {
        parse_footer_summary_line(&line, &mut summary);
    } else {
        // Fallback: count from cases
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

fn extract_failed_tests(raw: &str) -> std::collections::HashSet<String> {
    let mut failed = std::collections::HashSet::new();

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("FAILED ") {
            // format: FAILED path/to/file.py::test_name
            failed.insert(rest.trim().to_string());
        }
    }

    failed
}

fn extract_failure_spans(raw: &str) -> HashMap<String, FailureSpan> {
    let mut spans = HashMap::new();
    let lines: Vec<&str> = raw.lines().collect();
    let mut i = 0;
    let mut byte_offset = 0;

    while i < lines.len() {
        let line = lines[i];
        
        // Look for failure section headers (starts with underscores and contains ::)
        if line.starts_with("____") && line.contains("::") {
            // Extract test name from header line
            let test_name = line
                .split_whitespace()
                .find(|s| s.contains("::"))
                .map(|s| s.trim_matches('_').to_string());

            if let Some(name) = test_name {
                let start = byte_offset;
                
                // Find the end of this failure block
                let mut j = i + 1;
                let mut end_offset = byte_offset + line.len() + 1;
                
                while j < lines.len() {
                    let next_line = lines[j];
                    
                    // End of failure block if we hit another failure header or short test summary
                    if (next_line.starts_with("____") && next_line.contains("::"))
                        || next_line.starts_with("=") && next_line.contains("short test summary")
                    {
                        break;
                    }
                    
                    end_offset += next_line.len() + 1;
                    j += 1;
                }
                
                spans.insert(name, FailureSpan { start, end: end_offset });
                i = j;
                byte_offset = end_offset;
                continue;
            }
        }
        
        byte_offset += line.len() + 1;
        i += 1;
    }

    spans
}

fn extract_durations(raw: &str) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    let mut active = false;

    for line in raw.lines() {
        if line.contains("slowest durations") {
            active = true;
            continue;
        }
        if active {
            if line.trim().is_empty() {
                break;
            }
            let mut parts = line.split_whitespace();
            let time = parts
                .next()
                .and_then(|t| t.trim_end_matches('s').parse::<f64>().ok());
            let name = parts.find(|p| p.contains("::"));

            if let (Some(t), Some(n)) = (time, name) {
                map.insert(n.to_string(), t);
            }
        }
    }

    map
}

fn extract_skips(raw: &str) -> Vec<SkipEntry> {
    let mut skips = Vec::new();

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("SKIPPED [") {
            if let Some((count_part, rest)) = rest.split_once("] ") {
                let count = count_part.parse::<usize>().unwrap_or(1);
                if let Some((loc, reason)) = rest.split_once(": ") {
                    if let Some((file, line_no)) = loc.rsplit_once(':') {
                        if let Ok(line) = line_no.parse::<usize>() {
                            skips.push(SkipEntry {
                                file: file.to_string(),
                                line,
                                reason: reason.to_string(),
                                count,
                            });
                        }
                    }
                }
            }
        }
    }

    skips
}

fn extract_warnings(raw: &str) -> Vec<WarningEntry> {
    let mut warnings = Vec::new();
    let mut in_block = false;

    for line in raw.lines() {
        if line.contains("warnings summary") {
            in_block = true;
            continue;
        }
        if in_block {
            if line.trim().is_empty() || line.starts_with("=") {
                break;
            }
            warnings.push(WarningEntry {
                message: line.to_string(),
                file: None,
                line: None,
            });
        }
    }

    warnings
}

fn extract_verbose_failures(raw: &str) -> Vec<(String, String)> {
    let mut failures = Vec::new();
    let mut current_test = String::new();
    let mut buf = String::new();
    let mut in_failures = false;

    for line in raw.lines() {
        // Start of FAILURES section
        if line.starts_with("====") && line.contains("FAILURES") {
            in_failures = true;
            buf.clear();
            current_test.clear();
            continue;
        }

        // End of FAILURES section
        if in_failures && line.starts_with("====") && line.contains("short test summary") {
            if !buf.trim().is_empty() && !current_test.is_empty() {
                failures.push((current_test.clone(), buf.clone()));
            }
            break;
        }

        if in_failures {
            // New test failure header
            if line.starts_with("____") && line.contains("::") {
                // Save previous test if exists
                if !buf.trim().is_empty() && !current_test.is_empty() {
                    failures.push((current_test.clone(), buf.clone()));
                }
                
                // Extract test name
                current_test = line
                    .split_whitespace()
                    .find(|s| s.contains("::"))
                    .map(|s| s.trim_matches('_').to_string())
                    .unwrap_or_default();
                    
                buf.clear();
                buf.push_str(line);
                buf.push('\n');
            } else {
                buf.push_str(line);
                buf.push('\n');
            }
        }
    }

    failures
}

pub fn write_test_suite_report(
    repo_root: &Path,
    suite: &TestSuiteResult,
) -> io::Result<PathBuf> {
    let artifacts = repo_root.join("artifacts/tests/latest");
    fs::create_dir_all(&artifacts)?;

    fs::write(artifacts.join("raw.log"), suite.raw_output.as_bytes())?;

    let (summary, cases) = parse_pytest_output_fully(&suite.raw_output);
    let total_tests = summary.passed + summary.failed + summary.skipped;

    let failure_spans = extract_failure_spans(&suite.raw_output);
    let durations = extract_durations(&suite.raw_output);
    let skips = extract_skips(&suite.raw_output);
    let warnings = extract_warnings(&suite.raw_output);
    let verbose_failures = extract_verbose_failures(&suite.raw_output);

    let mut by_file: BTreeMap<String, Vec<&TestCaseResult>> = BTreeMap::new();
    for c in &cases {
        by_file.entry(c.file.clone()).or_default().push(c);
    }

    let mut files_with_failures = Vec::new();
    let mut files_with_skips = Vec::new();
    let mut files_with_warnings = Vec::new();

    for (file, tests) in &by_file {
        if tests.iter().any(|t| matches!(t.outcome, TestOutcome::Fail)) {
            files_with_failures.push(file.clone());
        }
        if tests.iter().any(|t| matches!(t.outcome, TestOutcome::Skip)) {
            files_with_skips.push(file.clone());
        }
    }

    for s in &skips {
        if !files_with_warnings.contains(&s.file) {
            files_with_warnings.push(s.file.clone());
        }
    }

    let report_path = artifacts.join("report.md");
    let mut out = File::create(&report_path)?;

    writeln!(out, "# 🧪 Test Suite Report\n")?;
    writeln!(
        out,
        "## Test Summary\n**Status:** {}\n",
        if summary.failed > 0 { "❌ FAILED" } else { "✅ PASSED" }
    )?;

    writeln!(
        out,
        "- ✅ Passed: {}\n\
         - ❌ Failed: {}\n\
         - ⏭ Skipped: {}\n\
         - ⚠️ Warnings: {}\n\
         - 🧩 Subtests: {}\n\
         - ⏱ Duration: {:.2}s\n",
        summary.passed,
        summary.failed,
        summary.skipped,
        summary.warnings,
        summary.subtests,
        summary.duration_s.unwrap_or(0.0),
    )?;

    writeln!(out, "## Test Results by File\n")?;

    for (file, tests) in &by_file {
        let p = tests.iter().filter(|t| matches!(t.outcome, TestOutcome::Pass)).count();
        let f = tests.iter().filter(|t| matches!(t.outcome, TestOutcome::Fail)).count();
        let s = tests.iter().filter(|t| matches!(t.outcome, TestOutcome::Skip)).count();

        writeln!(
            out,
            "### {}\n{} ({} passed, {} failed, {} skipped)\n",
            file,
            if f > 0 { "❌ FAILED" } else { "✅ PASSED" },
            p,
            f,
            s
        )?;

        for t in tests {
            let (icon, label) = match t.outcome {
                TestOutcome::Pass => ("✅", "PASS"),
                TestOutcome::Fail => ("❌", "FAIL"),
                TestOutcome::Skip => ("⏭", "SKIP"),
                TestOutcome::Warning => ("⚠️", "WARN"),
            };
            writeln!(out, "- {} **{}** —→ {}", icon, t.name, label)?;
        }
        writeln!(out)?;
    }

    if !verbose_failures.is_empty() {
        writeln!(out, "## ❌ Detailed Failure Information\n")?;
        for (test_name, failure_output) in &verbose_failures {
            writeln!(out, "### {}\n", test_name)?;
            writeln!(out, "```")?;
            writeln!(out, "{}", failure_output.trim())?;
            writeln!(out, "```\n")?;
        }
    }

    writeln!(out, "## 🔍 Diagnostic Summary (For Automated Analysis)\n")?;

    writeln!(out, "### Execution Context")?;
    writeln!(out, "- Runner: pytest")?;
    writeln!(out, "- Language: Python")?;
    writeln!(out, "- Working directory: {}", repo_root.display())?;
    writeln!(out, "- Invocation: python -m pytest -vv -rA --durations=0")?;
    writeln!(out, "- Total tests discovered: {}", total_tests)?;
    writeln!(
        out,
        "- Execution duration: {:.2}s\n",
        summary.duration_s.unwrap_or(0.0)
    )?;

    writeln!(out, "### Outcome Classification")?;
    writeln!(
        out,
        "- Overall result: {}",
        if summary.failed > 0 { "FAILED" } else { "PASSED" }
    )?;
    writeln!(out, "- Failures detected: {}", summary.failed > 0)?;
    writeln!(out, "- Skipped tests present: {}", summary.skipped > 0)?;
    writeln!(out, "- Warnings present: {}", summary.warnings > 0)?;
    writeln!(out, "- Non-deterministic signals: unknown\n")?;

    writeln!(out, "### Attention Index")?;
    writeln!(out, "- Files with failures: {:?}", files_with_failures)?;
    writeln!(out, "- Files with skips: {:?}", files_with_skips)?;
    writeln!(out, "- Files with warnings: {:?}", files_with_warnings)?;
    writeln!(
        out,
        "- Tests with non-PASS outcomes: {}",
        summary.failed + summary.skipped
    )?;
    writeln!(out, "- Longest-running test: unknown")?;
    writeln!(out, "- Suspected hotspots: []\n")?;

    writeln!(out, "### Skips & Warnings (Extracted)")?;
    if skips.is_empty() && warnings.is_empty() {
        writeln!(out, "- None\n")?;
    } else {
        for s in &skips {
            writeln!(out, "- {}:{} — {}", s.file, s.line, s.reason)?;
        }
        for w in &warnings {
            writeln!(out, "- {}", w.message)?;
        }
        writeln!(out)?;
    }

    writeln!(out, "### Raw Artifacts")?;
    writeln!(out, "- Full stdout/stderr: artifacts/tests/latest/raw.log")?;
    writeln!(out, "- Structured index: artifacts/tests/latest/index.json")?;
    writeln!(out, "- This report: artifacts/tests/latest/report.md\n")?;

    writeln!(out, "### Repair Contract")?;
    writeln!(out, "If failures occur in future runs:")?;
    writeln!(out, "1. Identify failing tests from this report")?;
    writeln!(out, "2. Locate their raw output via byte offsets in index.json")?;
    writeln!(out, "3. Propose minimal code changes scoped to the failing file")?;
    writeln!(out, "4. Rerun only affected test files")?;
    writeln!(out, "5. Regenerate this report")?;

    let mut indexed_tests = Vec::new();
    let mut cursor = 0usize;

    for line in suite.raw_output.lines() {
        let len = line.len() + 1;
        for c in &cases {
            let full_name = format!("{}::{}", c.file, c.name);
            if line.contains(&full_name) || line.contains(&c.name) {
                indexed_tests.push(IndexedTest {
                    file: c.file.clone(),
                    name: c.name.clone(),
                    outcome: format!("{:?}", c.outcome).to_uppercase(),
                    raw_span: RawSpan {
                        start: cursor,
                        end: cursor + len,
                    },
                    failure_span: failure_spans.get(&full_name).or_else(|| failure_spans.get(&c.name)).cloned(),
                    duration_s: durations.get(&full_name).or_else(|| durations.get(&c.name)).copied(),
                });
            }
        }
        cursor += len;
    }

    let mut files = BTreeMap::new();
    for t in &indexed_tests {
        let entry = files.entry(t.file.clone()).or_insert(FileIndex {
            passed: 0,
            failed: 0,
            skipped: 0,
            tests: Vec::new(),
        });

        match t.outcome.as_str() {
            "PASS" => entry.passed += 1,
            "FAIL" => entry.failed += 1,
            "SKIP" => entry.skipped += 1,
            _ => {}
        }

        entry.tests.push(t.clone());
    }

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let index = IndexFile {
        meta: IndexMeta {
            runner: "pytest",
            language: "python",
            invocation: "python -m pytest -vv -rA --durations=0",
            working_directory: repo_root.display().to_string(),
            generated_at: ts.to_string(),
        },
        summary: IndexSummary {
            passed: summary.passed,
            failed: summary.failed,
            skipped: summary.skipped,
            warnings: summary.warnings,
            total: total_tests,
        },
        files,
        tests: indexed_tests,
        skips,
        warnings,
    };

    fs::write(
        artifacts.join("index.json"),
        serde_json::to_string_pretty(&index)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?,
    )?;

    Ok(report_path)
}