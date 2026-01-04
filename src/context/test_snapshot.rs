// src/context/test_snapshot.rs
//
// Repo-level test context discovery.
// Robust, multi-file, source-based detection.

use std::fs;
use std::path::{Path, PathBuf};

use crate::context::snapshot::parse_file;
use crate::context::types::{
    Import, SymbolDef,
    TestContextSnapshot, TestFramework, TestStyle,
};

const MAX_TEST_FILES_TO_SCAN: usize = 20;

/// Build a repo-level test context snapshot.
/// Safe to cache. Deterministic. Source-truth based.
pub fn build_test_context_snapshot(repo_root: &Path) -> TestContextSnapshot {
    let test_roots = detect_test_roots(repo_root);

    if test_roots.is_empty() {
        return empty_snapshot(false, test_roots);
    }

    let test_files = collect_test_files_recursive(&test_roots);

    if test_files.is_empty() {
        return empty_snapshot(true, test_roots);
    }

    let mut all_symbols = Vec::new();
    let mut all_imports = Vec::new();

    let mut saw_unittest = false;
    let mut saw_pytest = false;

    for file in test_files.iter().take(MAX_TEST_FILES_TO_SCAN) {
        let source = fs::read_to_string(file).unwrap_or_default();
        let (symbols, imports) = parse_file(file, &source);

        all_symbols.extend(symbols.clone());
        all_imports.extend(imports);

        match detect_framework_from_source(&source, &symbols) {
            TestFramework::Unittest => saw_unittest = true,
            TestFramework::Pytest => saw_pytest = true,
            _ => {}
        }
    }

    let framework = match (saw_unittest, saw_pytest) {
    (true, false) => Some(TestFramework::Unittest),
    (false, true) => Some(TestFramework::Pytest),
    (true, true) => Some(TestFramework::Unittest),
    (false, false) => Some(TestFramework::Pytest), // default
    };



    let helpers = detect_helpers(&all_symbols);
    let references = detect_references(&all_imports);
    let style = detect_style(&helpers, &references, &all_symbols);

    TestContextSnapshot {
        exists: true,
        test_roots,
        framework,
        style,
        helpers,
        references,
    }
}

/// ---------- helpers ----------

fn empty_snapshot(exists: bool, test_roots: Vec<PathBuf>) -> TestContextSnapshot {
    TestContextSnapshot {
        exists,
        test_roots,
        framework: None,
        style: None,
        helpers: Vec::new(),
        references: Vec::new(),
    }
}

fn detect_test_roots(repo_root: &Path) -> Vec<PathBuf> {
    ["test", "tests"]
        .iter()
        .map(|d| repo_root.join(d))
        .filter(|p| p.exists() && p.is_dir())
        .collect()
}

/// Recursively collect ALL python files under test roots.
/// Do not rely on naming alone.
fn collect_test_files_recursive(test_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in test_roots {
        collect_py_files(root, &mut files);
    }
    files.sort();
    files
}

fn collect_py_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_py_files(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("py") {
                out.push(p);
            }
        }
    }
}

fn detect_framework_from_source(
    source: &str,
    symbols: &[SymbolDef],
) -> TestFramework {
    let mut saw_unittest = false;
    let mut saw_pytest = false;

    // ---------- LINE-BASED SIGNALS ----------
    for line in source.lines() {
        let l = line.trim();

        // unittest imports
        if l.starts_with("import ") && l.contains("unittest") {
            saw_unittest = true;
        }

        if l.starts_with("from unittest import") {
            saw_unittest = true;
        }

        // pytest imports / decorators
        if l.starts_with("import pytest") || l.contains("@pytest.") {
            saw_pytest = true;
        }
    }

    // ---------- STRUCTURAL SIGNALS ----------
    for s in symbols {
        let name = s.name.as_str();

        // class TestX(unittest.TestCase)
        if name.contains("Test") && source.contains("TestCase") {
            saw_unittest = true;
        }

        // pytest-style test functions
        if name.starts_with("test_") {
            saw_pytest = true;
        }

        // unittest-style test classes / methods
        if name.starts_with("Test") {
            saw_unittest = true;
        }
    }

    // ---------- EXPLICIT PYTEST SIGNAL ----------
    // conftest.py exists ONLY for pytest and ONLY under test roots
    if source.contains("pytest") && source.contains("fixture") {
        saw_pytest = true;
    }

    // ---------- DECISION ----------
    match (saw_unittest, saw_pytest) {
        (true, false) => TestFramework::Unittest,
        (false, true) => TestFramework::Pytest,
        (true, true) => TestFramework::Unittest, // explicit beats implicit
        _ => TestFramework::Unknown,
    }
}


fn detect_helpers(symbols: &[SymbolDef]) -> Vec<String> {
    let mut helpers = Vec::new();

    for s in symbols {
        let name = s.name.as_str();

        // --- explicit helper prefixes ---
        if name.starts_with("helper_")
            || name.starts_with("fixture_")
            || name.starts_with("setup_")
            || name.starts_with("teardown_")
        {
            helpers.push(name.to_string());
            continue;
        }

        // --- pytest-style fixtures ---
        if name.starts_with("fixture")
            || name.ends_with("_fixture")
        {
            helpers.push(name.to_string());
            continue;
        }

        // --- factory / builder helpers ---
        if name.starts_with("make_")
            || name.starts_with("build_")
            || name.starts_with("create_")
            || name.starts_with("gen_")
        {
            helpers.push(name.to_string());
            continue;
        }
    }

    helpers.sort();
    helpers.dedup();
    helpers
}

fn detect_references(imports: &[Import]) -> Vec<String> {
    let mut refs = Vec::new();

    for imp in imports {
        let m = imp.module.as_str();
        if m.contains("torch") {
            refs.push("torch".to_string());
        }
        if m.contains("jax") {
            refs.push("jax".to_string());
        }
        if m.contains("numpy") {
            refs.push("numpy".to_string());
        }
    }

    refs.sort();
    refs.dedup();
    refs
}

fn detect_style(
    helpers: &[String],
    references: &[String],
    symbols: &[SymbolDef],
) -> Option<TestStyle> {
    // Helper + reference strongly implies equivalence testing
    if !helpers.is_empty() && !references.is_empty() {
        return Some(TestStyle::Equivalence);
    }

    // Regression / bug naming
    if symbols.iter().any(|s| {
        s.name.contains("regression") || s.name.contains("bug")
    }) {
        return Some(TestStyle::Edge);
    }

    None
}
