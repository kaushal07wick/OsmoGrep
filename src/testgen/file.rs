// src/testgen/file.rs

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::detectors::language::Language;
use crate::testgen::candidate::TestCandidate;
use crate::testgen::resolve::TestResolution;

/* ============================================================
   Public API
   ============================================================ */

pub fn materialize_test(
    language: Language,
    candidate: &TestCandidate,
    resolution: &TestResolution,
    test_code: &str,
) -> io::Result<PathBuf> {
    match language {
        Language::Python => {
            write_python_test(candidate, resolution, test_code)
        }
        Language::Rust => {
            write_rust_test(candidate, resolution, test_code)
        }
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
    candidate: &TestCandidate,
    resolution: &TestResolution,
    test_code: &str,
) -> io::Result<PathBuf> {
    let path = match resolution {
        TestResolution::Found { file, .. } => PathBuf::from(file),
        TestResolution::Ambiguous(_) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Multiple possible Python test files found",
            ));
        }
        TestResolution::NotFound => default_python_test_path(candidate),
    };

    ensure_parent_dir(&path)?;
    append_once(&path, test_code, "# OSMOGREP TEST")?;
    Ok(path)
}

fn default_python_test_path(c: &TestCandidate) -> PathBuf {
    let name = sanitize(&c.file, &c.symbol);
    PathBuf::from(format!("tests/test_{}_regression.py", name))
}

/* ============================================================
   Rust
   ============================================================ */

fn write_rust_test(
    candidate: &TestCandidate,
    resolution: &TestResolution,
    test_code: &str,
) -> io::Result<PathBuf> {
    match resolution {
        TestResolution::Found { file, .. } => {
            let path = PathBuf::from(file);
            append_inline_rust_test(&path, test_code)?;
            Ok(path)
        }

        TestResolution::Ambiguous(_) => {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Multiple possible Rust test locations found",
            ))
        }

        TestResolution::NotFound => {
            let path = default_rust_test_path(candidate);
            ensure_parent_dir(&path)?;
            append_once(&path, test_code, "// OSMOGREP TEST")?;
            Ok(path)
        }
    }
}

fn default_rust_test_path(c: &TestCandidate) -> PathBuf {
    let name = sanitize(&c.file, &c.symbol);
    PathBuf::from(format!("tests/{}_regression.rs", name))
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

/// Append content only if sentinel is not already present
fn append_once(
    path: &Path,
    content: &str,
    sentinel: &str,
) -> io::Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    if existing.contains(sentinel) {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    writeln!(file, "\n{}\n{}", sentinel, content.trim_end())?;
    Ok(())
}

fn append_inline_rust_test(
    path: &Path,
    test_code: &str,
) -> io::Result<()> {
    let src = fs::read_to_string(path)?;

    if src.contains("// OSMOGREP TEST") {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new().append(true).open(path)?;

    writeln!(
        file,
        "\n\n#[cfg(test)]\nmod osmogrep_tests {{\n{}\n}}\n// OSMOGREP TEST\n",
        indent(test_code.trim_end(), 4)
    )?;

    Ok(())
}

fn indent(s: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    s.lines()
        .map(|l| format!("{pad}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn sanitize(file: &str, symbol: &Option<String>) -> String {
    let mut name = file.replace('/', "_");

    if let Some(sym) = symbol {
        name.push('_');
        name.push_str(sym);
    }

    name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_")
}
