use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::detectors::language::Language;
use crate::testgen::candidate::TestCandidate;

/* ============================================================
   Public entry
   ============================================================ */

pub fn materialize_test(
    repo_root: &Path,
    language: Language,
    candidate: &TestCandidate,
    test_code: &str,
) -> io::Result<PathBuf> {
    match language {
        Language::Python => write_python_test(repo_root, candidate, test_code),
        Language::Rust => write_rust_test(repo_root, candidate, test_code),
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            "Unsupported language for test generation",
        )),
    }
}

/* ============================================================
   Python
   ============================================================ */

fn write_python_test(
    repo_root: &Path,
    candidate: &TestCandidate,
    test_code: &str,
) -> io::Result<PathBuf> {
    let root = find_test_root(repo_root)?;
    let name = sanitize_name(&candidate.file, &candidate.symbol);
    let path = root.join(format!("test_{name}.py"));

    ensure_parent_dir(&path)?;
    append_if_missing(&path, test_code)?;
    Ok(path)
}

/* ============================================================
   Rust
   ============================================================ */

fn write_rust_test(
    repo_root: &Path,
    candidate: &TestCandidate,
    test_code: &str,
) -> io::Result<PathBuf> {
    let root = find_test_root(repo_root)?;
    let name = sanitize_name(&candidate.file, &candidate.symbol);
    let path = root.join(format!("{name}.rs"));

    ensure_parent_dir(&path)?;
    append_if_missing(&path, test_code)?;
    Ok(path)
}

/* ============================================================
   Test root resolution
   ============================================================ */

fn find_test_root(repo_root: &Path) -> io::Result<PathBuf> {
    for name in ["tests", "test"] {
        let path = repo_root.join(name);
        if path.is_dir() {
            return Ok(path);
        }
    }

    let root = repo_root.join("tests");
    fs::create_dir_all(&root)?;
    Ok(root)
}

/* ============================================================
   Helpers
   ============================================================ */

fn ensure_parent_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn append_if_missing(path: &Path, content: &str) -> io::Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();

    if existing.contains(content.trim()) {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    writeln!(file, "\n{}", content.trim())?;
    Ok(())
}

fn sanitize_name(file: &str, symbol: &Option<String>) -> String {
    let mut name = Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("test")
        .to_string();

    if let Some(sym) = symbol {
        name.push('_');
        name.push_str(sym);
    }

    name = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();

    if name.chars().next().unwrap_or('_').is_numeric() {
        format!("test_{name}")
    } else {
        name
    }
}
